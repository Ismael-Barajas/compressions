use std::path::PathBuf;

use tauri::{ipc::Channel, AppHandle};
use uuid::Uuid;

use crate::commands::job::{
    finish_job, prepare_job, resolve_duration, run_batch, run_sidecar, send_started, JobContext,
    ProgressTarget, SidecarOutcome, SidecarSpec,
};
use crate::ffmpeg::args::{
    build_gif_palette_args, build_gif_paletteuse_args, build_gif_single_pass_args,
    gif_prefers_single_pass,
};
use crate::types::{
    BatchEntry, CompressionResult, GifConversionOptions, ProgressEvent, Resolution,
};
use crate::validate::validate_gif_options;

/// Temporary palette image for a two-pass conversion; removed on drop so early
/// returns and cancellation never leave it behind.
struct TempPalette(PathBuf);

impl TempPalette {
    fn new() -> Self {
        Self(std::env::temp_dir().join(format!("compressions_palette_{}.png", Uuid::new_v4())))
    }

    fn path(&self) -> String {
        self.0.to_string_lossy().to_string()
    }
}

impl Drop for TempPalette {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

async fn run_ffmpeg(
    app: &AppHandle,
    ctx: &JobContext,
    args: &[String],
    progress: ProgressTarget<'_>,
) -> Result<SidecarOutcome, String> {
    run_sidecar(
        app,
        ctx,
        SidecarSpec {
            sidecar: "ffmpeg",
            args,
            progress: Some(progress),
            capture_stderr: false,
        },
    )
    .await
}

/// Palette pass then encode pass, each reading the source. Twice the decode work
/// of single pass, but memory stays flat however long the clip is.
///
/// `palettegen` emits nothing until end of stream, so FFmpeg reports no progress
/// during pass 1: the row shows its indeterminate "Processing…" state, then fills
/// from 50% as pass 2 runs.
async fn convert_two_pass(
    app: &AppHandle,
    ctx: &JobContext,
    options: &GifConversionOptions,
    total_duration: f64,
    on_progress: &Channel<ProgressEvent>,
) -> Result<SidecarOutcome, String> {
    let palette = TempPalette::new();
    let palette_args = build_gif_palette_args(&ctx.input, &palette.path(), options);
    let pass1 = run_ffmpeg(
        app,
        ctx,
        &palette_args,
        ProgressTarget::range(total_duration, on_progress, 0.0, 50.0),
    )
    .await?;
    if pass1.exit_code != Some(0) {
        return Ok(pass1);
    }
    if crate::commands::queue::is_cancelled(app) {
        // Cancelled between passes: report a non-zero exit so the job is treated
        // as cancelled (silently) rather than as a completed, empty GIF.
        return Ok(SidecarOutcome {
            exit_code: None,
            ..pass1
        });
    }

    let encode_args = build_gif_paletteuse_args(&ctx.input, &palette.path(), &ctx.output, options);
    let pass2 = run_ffmpeg(
        app,
        ctx,
        &encode_args,
        ProgressTarget::range(total_duration, on_progress, 50.0, 100.0),
    )
    .await?;
    Ok(SidecarOutcome {
        duration_ms: pass1.duration_ms + pass2.duration_ms,
        ..pass2
    })
}

pub async fn convert_video_to_gif_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: GifConversionOptions,
    duration_hint: Option<f64>,
    resolution: Option<Resolution>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_gif_options(&options)?;
    tracing::info!(input = %input, output = %output, "Starting GIF conversion");

    let ctx = prepare_job(&input, &output, "gif").await?;
    let total_duration = resolve_duration(app, &input, duration_hint).await;
    let single_pass = gif_prefers_single_pass(&options, Some(total_duration), resolution.as_ref());

    send_started(&ctx, on_progress);

    let outcome = if single_pass {
        // Short clip: palettegen and paletteuse share one decode via `split`.
        let args = build_gif_single_pass_args(&ctx.input, &ctx.output, &options);
        run_ffmpeg(
            app,
            &ctx,
            &args,
            ProgressTarget::full(total_duration, on_progress),
        )
        .await?
    } else {
        tracing::info!(input = %input, "Using two-pass GIF conversion to bound memory");
        convert_two_pass(app, &ctx, &options, total_duration, on_progress).await?
    };

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

#[tauri::command]
pub async fn convert_video_to_gif(
    app: AppHandle,
    input: String,
    output: String,
    options: GifConversionOptions,
    duration: Option<f64>,
    resolution: Option<Resolution>,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    convert_video_to_gif_inner(
        &app,
        input,
        output,
        options,
        duration,
        resolution,
        &on_progress,
    )
    .await
}

/// GIF conversion decodes and filters the full video; sequential like video encoding.
#[tauri::command]
pub async fn convert_videos_to_gif_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: GifConversionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    Ok(run_batch(&app, files, 1, move |app, entry| {
        let options = options.clone();
        let on_progress = on_progress.clone();
        async move {
            convert_video_to_gif_inner(
                &app,
                entry.input,
                entry.output,
                options,
                entry.duration,
                entry.resolution,
                &on_progress,
            )
            .await
        }
    })
    .await)
}
