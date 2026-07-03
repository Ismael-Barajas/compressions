use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::compression::image as img_compress;
use crate::history::storage as history;
use crate::state::CancelFlag;
use crate::types::{
    BatchEntry, CompressionResult, HistoryEntry, ImageFormat, ImageOptions, ProgressEvent,
};
use crate::utils::OutputClaim;

fn is_avif_input(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("avif"))
        .unwrap_or(false)
}

fn is_heic_input(path: &str) -> bool {
    Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("heic") || e.eq_ignore_ascii_case("heif"))
        .unwrap_or(false)
}

// QOI: fast single-pass encode (unlike PNG deflate), still compressed (unlike BMP),
// and preserves RGBA — FFmpeg's 32-bit BMP output uses BI_RGB, which the image
// crate decodes as RGB32 and drops the alpha channel.
fn image_decode_temp_path(kind: &str) -> PathBuf {
    std::env::temp_dir().join(format!("compressions_{}_{}.qoi", kind, Uuid::new_v4()))
}

fn build_ffmpeg_image_decode_args(input: &str, output: &str) -> Vec<String> {
    vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-pix_fmt".into(),
        "rgba".into(),
        output.into(),
    ]
}

/// Decode an AVIF file to a temporary QOI via FFmpeg sidecar (preserves RGBA transparency).
async fn decode_avif_via_ffmpeg(app: &AppHandle, input: &str) -> Result<String, String> {
    let temp_path = image_decode_temp_path("avif");
    let temp_str = temp_path.to_string_lossy().to_string();
    let args = build_ffmpeg_image_decode_args(input, &temp_str);

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

/// Decode a HEIC/HEIF file to a temporary QOI via FFmpeg sidecar.
async fn decode_heic_via_ffmpeg(app: &AppHandle, input: &str) -> Result<String, String> {
    let temp_path = image_decode_temp_path("heic");
    let temp_str = temp_path.to_string_lossy().to_string();
    let args = build_ffmpeg_image_decode_args(input, &temp_str);

    let (mut rx, _child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar for HEIC decode: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg for HEIC decode: {}", e))?;

    while let Some(event) = rx.recv().await {
        if let CommandEvent::Terminated(status) = event {
            if status.code == Some(0) {
                return Ok(temp_str);
            } else {
                return Err(format!(
                    "FFmpeg HEIC decoding failed (code {:?})",
                    status.code
                ));
            }
        }
    }

    Err("FFmpeg HEIC decode process ended unexpectedly".to_string())
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

    // Atomic claim with auto-cleanup on early return (e.g. AVIF/HEIC decode failure).
    let _output_claim = OutputClaim::claim(&output);
    let output = _output_claim.path().to_string();

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
        input_path: input.clone(),
    });

    let start = Instant::now();

    // Resolve Original → concrete format using the real input path (before AVIF temp substitution)
    let effective_format = options.format.resolve_for_input(&input);
    let mut resolved_options = options.clone();
    resolved_options.format = effective_format.clone();

    // If input is AVIF or HEIC, decode via FFmpeg to a temp QOI first (image crate can't decode these)
    let decode_temp = if is_avif_input(&input) {
        Some(decode_avif_via_ffmpeg(&app, &input).await?)
    } else if is_heic_input(&input) {
        Some(decode_heic_via_ffmpeg(&app, &input).await?)
    } else {
        None
    };
    let effective_input = decode_temp.as_deref().unwrap_or(&input);

    // AVIF with metadata preservation routes through FFmpeg sidecar
    let needs_ffmpeg_avif =
        matches!(effective_format, ImageFormat::Avif) && !resolved_options.strip_metadata;

    let compression_result = if needs_ffmpeg_avif {
        // For AVIF output with metadata, use original input (FFmpeg handles the full pipeline)
        compress_avif_with_ffmpeg(&app, &input, &output, resolved_options.quality).await
    } else {
        let input_for_compress = effective_input.to_string();
        let output_clone = output.clone();
        tokio::task::spawn_blocking(move || {
            img_compress::compress(&input_for_compress, &output_clone, &resolved_options)
        })
        .await
        .map_err(|e| format!("Task join error: {}", e))?
    };

    // Clean up decode temp file (AVIF or HEIC)
    if let Some(ref temp) = decode_temp {
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
            let cancelled = crate::commands::queue::is_cancelled(&app);
            if cancelled {
                tracing::info!(input = %input, "Image compression cancelled by user");
            } else {
                tracing::warn!(input = %input, error = %e, "Image compression failed");
                let _ = on_progress.send(ProgressEvent::Error {
                    job_id: job_id.clone(),
                    message: e.clone(),
                });
            }
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
            if !cancelled {
                let _ =
                    history::append_entry(&app, HistoryEntry::from_result(&err_result, "image"));
            }
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
    let max_concurrent = std::thread::available_parallelism()
        .map(|n| n.get().min(8))
        .unwrap_or(4);
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::new();

    let cancel_flag = app
        .try_state::<CancelFlag>()
        .map(|s| s.0.clone())
        .unwrap_or_else(|| Arc::new(std::sync::atomic::AtomicBool::new(false)));

    for entry in files {
        if cancel_flag.load(Ordering::SeqCst) {
            break;
        }
        let opts = options.clone();
        let app_clone = app.clone();
        let channel_clone = on_progress.clone();
        let sem = semaphore.clone();
        let cancel = cancel_flag.clone();
        let handle = tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
            // Re-check after acquiring the permit — by the time a queued task runs,
            // the user may have cancelled. Skip cleanly without touching the file.
            if cancel.load(Ordering::SeqCst) {
                return Ok(CompressionResult {
                    job_id: Uuid::new_v4().to_string(),
                    input_path: entry.input.clone(),
                    output_path: entry.output.clone(),
                    input_size: 0,
                    output_size: 0,
                    duration_ms: 0,
                    success: false,
                    error: Some("Cancelled".to_string()),
                });
            }
            compress_image(app_clone, entry.input, entry.output, opts, channel_clone).await
        });
        handles.push(handle);
    }

    let mut results = Vec::new();
    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "Image task failed at IPC level, continuing batch");
                // Collect the error as a failed result instead of aborting the entire batch
                results.push(CompressionResult {
                    job_id: Uuid::new_v4().to_string(),
                    input_path: String::new(),
                    output_path: String::new(),
                    input_size: 0,
                    output_size: 0,
                    duration_ms: 0,
                    success: false,
                    error: Some(e),
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, "Image task join error, continuing batch");
                results.push(CompressionResult {
                    job_id: Uuid::new_v4().to_string(),
                    input_path: String::new(),
                    output_path: String::new(),
                    input_size: 0,
                    output_size: 0,
                    duration_ms: 0,
                    success: false,
                    error: Some(format!("Task join error: {}", e)),
                });
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_temp_path_uses_qoi_extension() {
        let temp = image_decode_temp_path("avif");

        assert_eq!(temp.extension().and_then(|e| e.to_str()), Some("qoi"));
    }

    #[test]
    fn ffmpeg_decode_args_use_rgba_for_qoi() {
        let args = build_ffmpeg_image_decode_args("in.avif", "out.qoi");

        assert!(args.windows(2).any(|pair| pair == ["-pix_fmt", "rgba"]));
        assert_eq!(args.last().map(String::as_str), Some("out.qoi"));
    }
}
