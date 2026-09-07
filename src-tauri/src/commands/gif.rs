use tauri::{ipc::Channel, AppHandle};

use crate::commands::job::{
    finish_job, prepare_job, resolve_duration, run_batch, run_sidecar, send_started, SidecarSpec,
};
use crate::ffmpeg::args::build_gif_single_pass_args;
use crate::types::{BatchEntry, CompressionResult, GifConversionOptions, ProgressEvent};
use crate::validate::validate_gif_options;

pub async fn convert_video_to_gif_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: GifConversionOptions,
    duration_hint: Option<f64>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_gif_options(&options)?;
    tracing::info!(input = %input, output = %output, "Starting GIF conversion");

    let ctx = prepare_job(&input, &output, "gif").await?;
    let total_duration = resolve_duration(app, &input, duration_hint).await;

    // Single pass: palettegen and paletteuse share one decode via `split`, so the
    // source is read once and no temporary palette file is needed.
    let args = build_gif_single_pass_args(&ctx.input, &ctx.output, &options);

    send_started(&ctx, on_progress);

    let outcome = run_sidecar(
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
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    convert_video_to_gif_inner(&app, input, output, options, duration, &on_progress).await
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
                &on_progress,
            )
            .await
        }
    })
    .await)
}
