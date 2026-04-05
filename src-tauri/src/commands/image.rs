use std::path::Path;
use std::time::Instant;

use tauri::{AppHandle, ipc::Channel};
use tauri_plugin_shell::ShellExt;
use tauri_plugin_shell::process::CommandEvent;
use uuid::Uuid;

use crate::compression::image as img_compress;
use crate::history::storage as history;
use crate::types::{BatchEntry, CompressionResult, HistoryEntry, ImageFormat, ImageOptions, ProgressEvent};

fn is_avif_input(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("avif"))
        .unwrap_or(false)
}

/// Decode an AVIF file to a temporary PNG via FFmpeg sidecar (preserves RGBA transparency).
async fn decode_avif_via_ffmpeg(app: &AppHandle, input: &str) -> Result<String, String> {
    let temp_dir = std::env::temp_dir();
    let temp_name = format!("compressions_avif_{}.png", Uuid::new_v4());
    let temp_path = temp_dir.join(temp_name);
    let temp_str = temp_path.to_string_lossy().to_string();

    let args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-pix_fmt".into(),
        "rgba".into(),
        temp_str.clone(),
    ];

    let (mut rx, _child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar for AVIF decode: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg for AVIF decode: {}", e))?;

    while let Some(event) = rx.recv().await {
        if let CommandEvent::Terminated(status) = event {
            if status.code == Some(0) {
                return Ok(temp_str);
            } else {
                return Err(format!(
                    "FFmpeg AVIF decoding failed (code {:?})",
                    status.code
                ));
            }
        }
    }

    Err("FFmpeg AVIF decode process ended unexpectedly".to_string())
}

#[tauri::command]
pub async fn compress_image(
    app: AppHandle,
    input: String,
    output: String,
    options: ImageOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, format = ?options.format, "Starting image compression");
    let file_name = Path::new(&input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);

    // Ensure the output directory exists (needed for subfolder export mode)
    if let Some(parent) = Path::new(&output).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
    });

    let start = Instant::now();

    // If input is AVIF, decode via FFmpeg to a temp PNG first (image crate can't decode AVIF)
    let avif_temp = if is_avif_input(&input) {
        Some(decode_avif_via_ffmpeg(&app, &input).await?)
    } else {
        None
    };
    let effective_input = avif_temp.as_deref().unwrap_or(&input);

    // AVIF with metadata preservation routes through FFmpeg sidecar
    let needs_ffmpeg_avif = matches!(options.format, ImageFormat::Avif) && !options.strip_metadata;

    let compression_result = if needs_ffmpeg_avif {
        // For AVIF output with metadata, use original input (FFmpeg handles the full pipeline)
        compress_avif_with_ffmpeg(&app, &input, &output, options.quality).await
    } else {
        let input_for_compress = effective_input.to_string();
        let output_clone = output.clone();
        tokio::task::spawn_blocking(move || {
            img_compress::compress(&input_for_compress, &output_clone, &options)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    };

    // Clean up AVIF temp file
    if let Some(ref temp) = avif_temp {
        let _ = std::fs::remove_file(temp);
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    match compression_result {
        Ok(()) => {
            let mut output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

            // If compressed is larger or equal, copy original to output (keep the smaller file)
            if output_size >= input_size && input_size > 0 {
                if let Err(e) = std::fs::copy(&input, &output) {
                    return Err(format!("Failed to copy original to output: {}", e));
                }
                output_size = input_size;
            }

            let result = CompressionResult {
                job_id: job_id.clone(),
                input_path: input,
                output_path: output,
                input_size,
                output_size,
                duration_ms,
                success: true,
                error: None,
            };

            tracing::info!(input = %result.input_path, output_size = result.output_size, duration_ms = result.duration_ms, "Image compression completed");
            let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
            let _ = history::append_entry(&app, HistoryEntry::from_result(&result, "image"));
            Ok(result)
        }
        Err(e) => {
            tracing::warn!(input = %input, error = %e, "Image compression failed");
            let _ = on_progress.send(ProgressEvent::Error {
                job_id: job_id.clone(),
                message: e.clone(),
            });
            let err_result = CompressionResult {
                job_id,
                input_path: input,
                output_path: output,
                input_size,
                output_size: 0,
                duration_ms,
                success: false,
                error: Some(e),
            };
            let _ = history::append_entry(&app, HistoryEntry::from_result(&err_result, "image"));
            Ok(err_result)
        }
    }
}

async fn compress_avif_with_ffmpeg(
    app: &AppHandle,
    input: &str,
    output: &str,
    quality: u8,
) -> Result<(), String> {
    // Map quality 0-100 to CRF 63-0 (FFmpeg libaom-av1: lower CRF = higher quality)
    let crf = ((100 - quality) as f32 * 63.0 / 100.0) as u8;

    let args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-c:v".into(),
        "libaom-av1".into(),
        "-crf".into(),
        crf.to_string(),
        "-still-picture".into(),
        "1".into(),
        "-map_metadata".into(),
        "0".into(),
        output.into(),
    ];

    let (mut rx, _child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg for AVIF: {}", e))?;

    while let Some(event) = rx.recv().await {
        if let CommandEvent::Terminated(status) = event {
            if status.code == Some(0) {
                return Ok(());
            } else {
                return Err(format!(
                    "FFmpeg AVIF encoding failed (code {:?}). Metadata may not be supported.",
                    status.code
                ));
            }
        }
    }

    Err("FFmpeg AVIF process ended unexpectedly".to_string())
}

#[tauri::command]
pub async fn compress_images_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: ImageOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let mut handles = Vec::new();

    for entry in files {
        let opts = options.clone();
        let app_clone = app.clone();
        let channel_clone = on_progress.clone();
        let handle = tokio::spawn(async move {
            compress_image(app_clone, entry.input, entry.output, opts, channel_clone).await
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => return Err(e),
            Err(e) => return Err(format!("Task join error: {}", e)),
        }
    }

    Ok(results)
}
