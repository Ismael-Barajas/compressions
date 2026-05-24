use std::sync::Mutex;
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

use crate::compression::progress::parse_progress_line;
use crate::ffmpeg::args::{build_audio_compression_args, build_audio_extraction_args};
use crate::ffmpeg::probe::probe_video_duration;
use crate::history::storage as history;
use crate::state::AppState;
use crate::types::{
    AudioCompressionOptions, AudioExtractionOptions, BatchEntry, CompressionResult, HistoryEntry,
    ProgressEvent, ProgressPayload,
};
use crate::utils::OutputClaim;
use crate::validate::{validate_audio_compression_options, validate_audio_options};

#[tauri::command]
pub async fn extract_audio(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    input: String,
    output: String,
    options: AudioExtractionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_audio_options(&options)?;
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, output = %output, "Starting audio extraction");
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

    // Atomic claim with auto-cleanup on early return.
    let _output_claim = OutputClaim::claim(&output);
    let output = _output_claim.path().to_string();

    let total_duration = probe_video_duration(&app, &input).await.unwrap_or(0.0);

    let args = build_audio_extraction_args(&input, &output, &options);

    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

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
        input_path: input.clone(),
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

                let cancelled = crate::commands::queue::is_cancelled(&app);
                if success {
                    let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
                } else {
                    if let Err(e) = std::fs::remove_file(&output) {
                        tracing::warn!(path = %output, error = %e, "Failed to remove failed output");
                    }
                    if !cancelled {
                        let _ = on_progress.send(ProgressEvent::Error {
                            job_id: job_id.clone(),
                            message: result.error.clone().unwrap_or_default(),
                        });
                    }
                }

                if result.success {
                    tracing::info!(input = %result.input_path, duration_ms = result.duration_ms, "Audio extraction completed");
                } else if cancelled {
                    tracing::info!(input = %result.input_path, "Audio extraction cancelled by user");
                } else {
                    tracing::warn!(input = %result.input_path, error = ?result.error, "Audio extraction failed");
                }
                if success || !cancelled {
                    if let Err(e) =
                        history::append_entry(&app, HistoryEntry::from_result(&result, "audio"))
                    {
                        tracing::warn!(error = %e, "Failed to save history entry");
                    }
                }
                return Ok(result);
            }
            _ => {}
        }
    }

    Err("FFmpeg process ended unexpectedly".to_string())
}

#[tauri::command]
pub async fn extract_audio_batch(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    files: Vec<BatchEntry>,
    options: AudioExtractionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let mut results = Vec::new();
    for entry in files {
        let result = extract_audio(
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

#[tauri::command]
pub async fn compress_audio(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    input: String,
    output: String,
    options: AudioCompressionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_audio_compression_options(&options)?;
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, output = %output, "Starting audio compression");
    let file_name = std::path::Path::new(&input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);

    if let Some(parent) = std::path::Path::new(&output).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let _output_claim = OutputClaim::claim(&output);
    let output = _output_claim.path().to_string();

    let total_duration = probe_video_duration(&app, &input).await.unwrap_or(0.0);

    // Resolve Original → concrete format, then build args using AudioExtractionOptions
    let resolved_format = options.format.resolve_for_input(&input);
    let resolved_opts = AudioExtractionOptions {
        format: resolved_format,
        bitrate: options.bitrate.clone(),
        sample_rate: options.sample_rate,
    };
    let args = build_audio_compression_args(&input, &output, &resolved_opts);

    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

    {
        let mut app_state = state.lock().map_err(|e| e.to_string())?;
        app_state
            .active_jobs
            .insert(job_id.clone(), (child, output.clone()));
    }

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
        input_path: input.clone(),
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

                let cancelled = crate::commands::queue::is_cancelled(&app);
                if success {
                    let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
                } else {
                    if let Err(e) = std::fs::remove_file(&output) {
                        tracing::warn!(path = %output, error = %e, "Failed to remove failed output");
                    }
                    if !cancelled {
                        let _ = on_progress.send(ProgressEvent::Error {
                            job_id: job_id.clone(),
                            message: result.error.clone().unwrap_or_default(),
                        });
                    }
                }

                if result.success {
                    tracing::info!(input = %result.input_path, duration_ms = result.duration_ms, "Audio compression completed");
                } else if cancelled {
                    tracing::info!(input = %result.input_path, "Audio compression cancelled by user");
                } else {
                    tracing::warn!(input = %result.input_path, error = ?result.error, "Audio compression failed");
                }
                if success || !cancelled {
                    if let Err(e) =
                        history::append_entry(&app, HistoryEntry::from_result(&result, "audio"))
                    {
                        tracing::warn!(error = %e, "Failed to save history entry");
                    }
                }
                return Ok(result);
            }
            _ => {}
        }
    }

    Err("FFmpeg process ended unexpectedly".to_string())
}

#[tauri::command]
pub async fn compress_audio_batch(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    files: Vec<BatchEntry>,
    options: AudioCompressionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let mut results = Vec::new();
    for entry in files {
        let result = compress_audio(
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
