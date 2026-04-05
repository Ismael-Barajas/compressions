use std::sync::Mutex;
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

use crate::compression::progress::parse_progress_line;
use crate::ffmpeg::args::build_video_args;
use crate::ffmpeg::probe::probe_video_duration;
use crate::history::storage as history;
use crate::state::AppState;
use crate::utils::resolve_output_conflict;
use crate::types::{
    BatchEntry, CompressionResult, HistoryEntry, ProgressEvent, ProgressPayload, VideoCodec,
    VideoOptions,
};

/// Resolve which HW encoder (if any) to use for the given codec.
fn resolve_hw_encoder(codec: &VideoCodec, hw_encoders: &std::collections::HashSet<String>) -> Option<String> {
    let name = match codec {
        VideoCodec::H264 => {
            if hw_encoders.contains("h264_videotoolbox") {
                Some("h264_videotoolbox")
            } else if hw_encoders.contains("h264_nvenc") {
                Some("h264_nvenc")
            } else {
                None
            }
        }
        VideoCodec::H265 => {
            if hw_encoders.contains("hevc_videotoolbox") {
                Some("hevc_videotoolbox")
            } else if hw_encoders.contains("hevc_nvenc") {
                Some("hevc_nvenc")
            } else {
                None
            }
        }
        VideoCodec::AV1 => None, // No good HW AV1 encoder available
    };
    name.map(|s| s.to_string())
}

/// Spawn FFmpeg, track progress, return exit code and duration.
async fn run_ffmpeg(
    app: &AppHandle,
    state: &State<'_, Mutex<AppState>>,
    job_id: &str,
    file_name: &str,
    args: &[String],
    total_duration: f64,
    on_progress: &Channel<ProgressEvent>,
) -> Result<(Option<i32>, u64), String> {
    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

    {
        // We re-use the same job_id for the output path stored earlier
        let output = args.last().cloned().unwrap_or_default();
        let mut app_state = state.lock().map_err(|e| e.to_string())?;
        app_state
            .active_jobs
            .insert(job_id.to_string(), (child, output));
    }

    let start = Instant::now();
    let mut last_progress = ProgressPayload {
        job_id: job_id.to_string(),
        file_name: file_name.to_string(),
        percent: 0.0,
        current_frame: None,
        total_frames: None,
        speed: None,
        eta_seconds: None,
    };

    let mut exit_code = None;
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                if let Some(progress) = parse_progress_line(&line, total_duration) {
                    last_progress.percent = progress.percent;
                    last_progress.current_frame = progress.current_frame;
                    last_progress.speed = progress.speed.clone();
                    last_progress.eta_seconds = progress.eta_seconds;
                    let _ = on_progress.send(ProgressEvent::Progress(last_progress.clone()));
                }
            }
            CommandEvent::Terminated(status) => {
                if let Ok(mut app_state) = state.lock() {
                    app_state.active_jobs.remove(job_id);
                }
                exit_code = status.code;
                break;
            }
            _ => {}
        }
    }

    let duration_ms = start.elapsed().as_millis() as u64;
    Ok((exit_code, duration_ms))
}

#[tauri::command]
pub async fn compress_video(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    input: String,
    output: String,
    options: VideoOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, output = %output, "Starting video compression");
    let file_name = std::path::Path::new(&input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = std::fs::metadata(&input).map(|m| m.len()).unwrap_or(0);

    // Ensure the output directory exists (needed for subfolder export mode)
    if let Some(parent) = std::path::Path::new(&output).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Avoid overwriting existing files — append _2, _3, etc.
    let output = resolve_output_conflict(&output);

    let total_duration = probe_video_duration(&app, &input).await.unwrap_or(0.0);

    // Resolve HW encoder if available
    let hw_encoder = {
        let app_state = state.lock().map_err(|e| e.to_string())?;
        resolve_hw_encoder(&options.codec, &app_state.hw_encoders)
    };

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
    });

    // Try HW encoder first, fall back to software on failure
    let mut opts = options.clone();
    let mut used_hw = false;
    if let Some(ref hw) = hw_encoder {
        opts.hw_encoder = Some(hw.clone());
        used_hw = true;
        tracing::info!(encoder = %hw, "Trying HW encoder");
    }

    let args = build_video_args(&input, &output, &opts);
    let (exit_code, duration_ms) = run_ffmpeg(&app, &state, &job_id, &file_name, &args, total_duration, &on_progress).await?;

    // HW encoder failed — retry with software
    let (exit_code, duration_ms) = if exit_code != Some(0) && used_hw {
        tracing::warn!("HW encoder failed, falling back to software");
        let _ = std::fs::remove_file(&output);
        opts.hw_encoder = None;
        let sw_args = build_video_args(&input, &output, &opts);
        run_ffmpeg(&app, &state, &job_id, &file_name, &sw_args, total_duration, &on_progress).await?
    } else {
        (exit_code, duration_ms)
    };

    let output_size = std::fs::metadata(&output).map(|m| m.len()).unwrap_or(0);
    let success = exit_code == Some(0);
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
            Some(format!("FFmpeg exited with code {:?}", exit_code))
        },
    };

    if success {
        let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
        tracing::info!(input = %result.input_path, output_size = result.output_size, duration_ms = result.duration_ms, "Video compression completed");
    } else {
        let _ = std::fs::remove_file(&output);
        let _ = on_progress.send(ProgressEvent::Error {
            job_id: job_id.clone(),
            message: result.error.clone().unwrap_or_default(),
        });
        tracing::warn!(input = %result.input_path, error = ?result.error, "Video compression failed");
    }
    let _ = history::append_entry(&app, HistoryEntry::from_result(&result, "video"));
    Ok(result)
}

#[tauri::command]
pub async fn compress_videos_batch(
    app: AppHandle,
    state: State<'_, Mutex<AppState>>,
    files: Vec<BatchEntry>,
    options: VideoOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let mut results = Vec::new();
    for entry in files {
        let result = compress_video(
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
pub fn cancel_compression(state: State<'_, Mutex<AppState>>, job_id: String) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    if let Some((child, _)) = app_state.active_jobs.remove(&job_id) {
        child
            .kill()
            .map_err(|e| format!("Failed to kill FFmpeg process: {}", e))?;
        // Partial file cleanup happens in the Terminated event handler once the process exits
    }
    Ok(())
}
