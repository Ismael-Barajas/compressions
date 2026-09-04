use tauri::{ipc::Channel, AppHandle};

use crate::commands::job::{
    finish_job, prepare_job, resolve_duration, run_batch, run_sidecar, send_started,
    single_threaded_batch_concurrency, SidecarSpec,
};
use crate::ffmpeg::args::{build_audio_compression_args, build_audio_extraction_args};
use crate::types::{
    AudioCompressionOptions, AudioExtractionOptions, BatchEntry, CompressionResult, ProgressEvent,
};
use crate::validate::{validate_audio_compression_options, validate_audio_options};

/// Audio encoders (libmp3lame, aac, libopus, flac, pcm) are single-threaded, so a
/// batch runs several files at once to use the remaining cores.
const AUDIO_BATCH_CONCURRENCY_CAP: usize = 4;

async fn run_audio_job(
    app: &AppHandle,
    input: String,
    output: String,
    args_for: impl FnOnce(&str, &str) -> Vec<String>,
    duration_hint: Option<f64>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    let ctx = prepare_job(&input, &output, "audio").await?;
    let total_duration = resolve_duration(app, &input, duration_hint).await;
    let args = args_for(&ctx.input, &ctx.output);

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

pub async fn extract_audio_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: AudioExtractionOptions,
    duration_hint: Option<f64>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_audio_options(&options)?;
    tracing::info!(input = %input, output = %output, "Starting audio extraction");
    run_audio_job(
        app,
        input,
        output,
        |i, o| build_audio_extraction_args(i, o, &options),
        duration_hint,
        on_progress,
    )
    .await
}

pub async fn compress_audio_inner(
    app: &AppHandle,
    input: String,
    output: String,
    options: AudioCompressionOptions,
    duration_hint: Option<f64>,
    on_progress: &Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    validate_audio_compression_options(&options)?;
    tracing::info!(input = %input, output = %output, "Starting audio compression");

    // Resolve Original → concrete format, then build args using AudioExtractionOptions
    let resolved_opts = AudioExtractionOptions {
        format: options.format.resolve_for_input(&input),
        bitrate: options.bitrate.clone(),
        sample_rate: options.sample_rate,
    };
    run_audio_job(
        app,
        input,
        output,
        |i, o| build_audio_compression_args(i, o, &resolved_opts),
        duration_hint,
        on_progress,
    )
    .await
}

#[tauri::command]
pub async fn extract_audio(
    app: AppHandle,
    input: String,
    output: String,
    options: AudioExtractionOptions,
    duration: Option<f64>,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    extract_audio_inner(&app, input, output, options, duration, &on_progress).await
}

#[tauri::command]
pub async fn extract_audio_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: AudioExtractionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let concurrency = single_threaded_batch_concurrency(AUDIO_BATCH_CONCURRENCY_CAP);
    Ok(run_batch(&app, files, concurrency, move |app, entry| {
        let options = options.clone();
        let on_progress = on_progress.clone();
        async move {
            extract_audio_inner(
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
pub async fn compress_audio_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: AudioCompressionOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    let concurrency = single_threaded_batch_concurrency(AUDIO_BATCH_CONCURRENCY_CAP);
    Ok(run_batch(&app, files, concurrency, move |app, entry| {
        let options = options.clone();
        let on_progress = on_progress.clone();
        async move {
            compress_audio_inner(
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
