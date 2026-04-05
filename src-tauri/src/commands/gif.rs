use std::sync::Mutex;
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

use crate::compression::progress::parse_progress_line;
use crate::ffmpeg::args::{build_gif_encode_args, build_gif_palette_args};
use crate::ffmpeg::probe::probe_video_duration;
use crate::history::storage as history;
use crate::state::AppState;
use crate::types::{
    BatchEntry, CompressionResult, GifConversionOptions, HistoryEntry, ProgressEvent,
    ProgressPayload,
};
use crate::utils::resolve_output_conflict;

#[tauri::command]
pub async fn convert_video_to_gif(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    input: String,
    output: String,
    options: GifConversionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, output = %output, "Starting GIF conversion");
    let file_name = std::path::Path::new(&input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);

    // Ensure the output directory exists
    if let Some(parent) = std::path::Path::new(&output).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Avoid overwriting existing files — append _2, _3, etc.
    let output = resolve_output_conflict(&output);

    let total_duration = probe_video_duration(&app, &input).await.unwrap_or(0.0);

    // --- Pass 1: Generate palette ---
    let palette_path = {
        let temp_dir = std::env::temp_dir();
        let palette_name = format!("palette_{}.png", Uuid::new_v4());
        temp_dir.join(palette_name).to_string_lossy().to_string()
    };

    let palette_args = build_gif_palette_args(&input, &palette_path, &options);

    let (mut rx, _child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(&palette_args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg (palette pass): {}", e))?;

    // Wait for palette pass to complete
    let mut palette_ok = false;
    while let Some(event) = rx.recv().await {
        if let CommandEvent::Terminated(status) = event {
            palette_ok = status.code == Some(0);
            break;
        }
    }

    if !palette_ok {
        let _ = std::fs::remove_file(&palette_path);
        return Err("FFmpeg palette generation failed".to_string());
    }

    // --- Pass 2: Encode GIF using palette ---
    let encode_args = build_gif_encode_args(&input, &palette_path, &output, &options);

    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(&encode_args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg (encode pass): {}", e))?;

    // Store child handle for cancellation
    {
        let mut app_state = state.lock().map_err(|e| e.to_string())?;
        app_state
            .active_jobs
            .insert(job_id.clone(), (child, output.clone()));
    }

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
    });

    let start = Instant::now();
    let mut last_progress = ProgressPayload {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
        percent: 0.0,
        current_frame: None,
        total_frames: None,
        speed: None,
        eta_seconds: None,
    };

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                if let Some(progress) = parse_progress_line(&line, total_duration) {
                    last_progress.percent = progress.percent;
                    last_progress.speed = progress.speed.clone();
                    last_progress.eta_seconds = progress.eta_seconds;
                    let _ = on_progress.send(ProgressEvent::Progress(last_progress.clone()));
                }
            }
            CommandEvent::Terminated(status) => {
                // Clean up palette temp file
                let _ = std::fs::remove_file(&palette_path);

                if let Ok(mut app_state) = state.lock() {
                    app_state.active_jobs.remove(&job_id);
                }

                let duration_ms = start.elapsed().as_millis() as u64;
                let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);

                let success = status.code == Some(0);
                let result = CompressionResult {
                    job_id: job_id.clone(),
                    input_path: input.clone(),
                    output_path: output.clone(),
                    input_size,
                    output_size,
                    duration_ms,
                    success,
                    error: if success {
                        None
                    } else {
                        Some(format!("FFmpeg exited with code {:?}", status.code))
                    },
                };

                if success {
                    let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
                } else {
                    let _ = std::fs::remove_file(&output);
                    let _ = on_progress.send(ProgressEvent::Error {
                        job_id: job_id.clone(),
                        message: result.error.clone().unwrap_or_default(),
                    });
                }

                if result.success {
                    tracing::info!(input = %result.input_path, output_size = result.output_size, duration_ms = result.duration_ms, "GIF conversion completed");
                } else {
                    tracing::warn!(input = %result.input_path, error = ?result.error, "GIF conversion failed");
                }
                let _ = history::append_entry(&app, HistoryEntry::from_result(&result, "gif"));
                return Ok(result);
            }
            _ => {}
        }
    }

    // Clean up palette on unexpected exit
    let _ = std::fs::remove_file(&palette_path);
    Err("FFmpeg process ended unexpectedly".to_string())
}

#[tauri::command]
pub async fn convert_videos_to_gif_batch(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    files: Vec<BatchEntry>,
    options: GifConversionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let mut results = Vec::new();
    for entry in files {
        let result = convert_video_to_gif(
            app.clone(),
            state.clone(),
            entry.input,
            entry.output,
            options.clone(),
            on_progress.clone(),
        )
        .await?;
        results.push(result);
    }
    Ok(results)
}
