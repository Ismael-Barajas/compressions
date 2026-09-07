use tauri::{ipc::Channel, AppHandle, Manager, State};

use crate::commands::job::{
    finish_job, prepare_job, resolve_duration, run_batch, run_sidecar, send_started, SidecarSpec,
};
use crate::ffmpeg::args::build_video_args;
use crate::state::{AppState, HwEncoders};
use crate::types::{BatchEntry, CompressionResult, ProgressEvent, VideoCodec, VideoOptions};
use crate::validate::validate_video_options;

/// Resolve which HW encoder (if any) to use for the given codec.
fn resolve_hw_encoder(
    codec: &VideoCodec,
    hw_encoders: &std::collections::HashSet<String>,
) -> Option<String> {
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

fn pick_hw_encoder(app: &AppHandle, codec: &VideoCodec) -> Option<String> {
    let hw = app.try_state::<HwEncoders>()?;
    let encoders = hw.0.read().ok()?;
    resolve_hw_encoder(codec, &encoders)
}

/// A HW encoder that failed at runtime is dropped from the detected set so the
/// remaining files in the batch go straight to software instead of paying for a
/// failed spawn each.
fn disable_hw_encoder(app: &AppHandle, name: &str) {
    if let Some(hw) = app.try_state::<HwEncoders>() {
        if let Ok(mut set) = hw.0.write() {
            if set.remove(name) {
                tracing::warn!(encoder = %name, "HW encoder failed at runtime; disabled for this session");
            }
        }
    }
}

pub async fn compress_video_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: VideoOptions,
    duration_hint: Option<f64>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_video_options(&options)?;
    tracing::info!(input = %input, output = %output, "Starting video compression");

    let ctx = prepare_job(&input, &output, "video").await?;
    let total_duration = resolve_duration(app, &input, duration_hint).await;
    let hw_encoder = pick_hw_encoder(app, &options.codec);

    send_started(&ctx, on_progress);

    let mut opts = options.clone();
    if let Some(ref hw) = hw_encoder {
        opts.hw_encoder = Some(hw.clone());
        tracing::info!(encoder = %hw, "Trying HW encoder");
    }

    let args = build_video_args(&ctx.input, &ctx.output, &opts);
    let mut outcome = run_sidecar(
        app,
        &ctx,
        SidecarSpec {
            sidecar: "ffmpeg",
            args: &args,
            progress: Some((total_duration, on_progress)),
            capture_stderr: false,
        },
    )
    .await?;

    // HW encoder failed — retry with software (unless the user cancelled).
    if outcome.exit_code != Some(0)
        && hw_encoder.is_some()
        && !crate::commands::queue::is_cancelled(app)
    {
        if let Some(ref hw) = hw_encoder {
            disable_hw_encoder(app, hw);
        }
        tracing::warn!("HW encoder failed, falling back to software");
        if let Err(e) = tokio::fs::remove_file(&ctx.output).await {
            tracing::warn!(path = %ctx.output, error = %e, "Failed to remove partial HW output");
        }
        opts.hw_encoder = None;
        let sw_args = build_video_args(&ctx.input, &ctx.output, &opts);
        outcome = run_sidecar(
            app,
            &ctx,
            SidecarSpec {
                sidecar: "ffmpeg",
                args: &sw_args,
                progress: Some((total_duration, on_progress)),
                capture_stderr: false,
            },
        )
        .await?;
    }

    let error = (outcome.exit_code != Some(0))
        .then(|| format!("FFmpeg exited with code {:?}", outcome.exit_code));
    Ok(finish_job(
        app,
        &ctx,
        outcome.exit_code,
        outcome.duration_ms,
        error,
        on_progress,
    )
    .await)
}

/// Video encoding is sequential on purpose: libx264/x265/SVT-AV1 already saturate
/// every core, so a second concurrent encode only adds contention.
#[tauri::command]
pub async fn compress_videos_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: VideoOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    Ok(run_batch(&app, files, 1, move |app, entry| {
        let options = options.clone();
        let on_progress = on_progress.clone();
        async move {
            compress_video_inner(
                &app,
                entry.input,
                entry.output,
                options,
                entry.duration,
                &on_progress,
            )
            .await
        }
    })
    .await)
}

#[tauri::command]
pub fn cancel_compression(
    state: State<'_, std::sync::Mutex<AppState>>,
    job_id: String,
) -> Result<(), String> {
    let mut app_state = state.lock().map_err(|e| e.to_string())?;
    if let Some((child, _)) = app_state.active_jobs.remove(&job_id) {
        child
            .kill()
            .map_err(|e| format!("Failed to kill process: {}", e))?;
        // Partial file cleanup happens in the job runner once the process exits
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn prefers_videotoolbox_over_nvenc() {
        let hw = set(&["h264_videotoolbox", "h264_nvenc"]);
        assert_eq!(
            resolve_hw_encoder(&VideoCodec::H264, &hw).as_deref(),
            Some("h264_videotoolbox")
        );
    }

    #[test]
    fn falls_back_to_nvenc() {
        let hw = set(&["hevc_nvenc"]);
        assert_eq!(
            resolve_hw_encoder(&VideoCodec::H265, &hw).as_deref(),
            Some("hevc_nvenc")
        );
        assert_eq!(resolve_hw_encoder(&VideoCodec::H264, &hw), None);
    }

    #[test]
    fn av1_never_uses_hw() {
        let hw = set(&["h264_nvenc", "hevc_nvenc", "h264_videotoolbox"]);
        assert_eq!(resolve_hw_encoder(&VideoCodec::AV1, &hw), None);
    }
}
