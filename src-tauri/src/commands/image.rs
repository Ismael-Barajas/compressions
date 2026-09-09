use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::commands::job::{keep_original_if_larger, register_job, unregister_job};
use crate::compression::image as img_compress;
use crate::history::storage as history;
use crate::state::{CancelFlag, NativeJobGuard, NativeJobs};
use crate::types::{
    BatchEntry, CompressionResult, HistoryEntry, ImageFormat, ImageOptions, ProgressEvent,
};
use crate::utils::OutputClaim;
use crate::validate::validate_image_options;

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
        "-nostdin".into(),
        "-y".into(),
        "-i".into(),
        input.into(),
        "-pix_fmt".into(),
        "rgba".into(),
        output.into(),
    ]
}

/// Spawn an FFmpeg helper for this job, registered under `job_id` so Cancel All and
/// the close prompt see it, and wait for it to exit. Returns the exit code.
async fn run_ffmpeg_helper(
    app: &AppHandle,
    job_id: &str,
    output: &str,
    args: &[String],
    what: &str,
) -> Result<Option<i32>, String> {
    if crate::commands::queue::is_cancelled(app) {
        return Err("Cancelled".to_string());
    }
    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar for {}: {}", what, e))?
        .args(args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg for {}: {}", what, e))?;
    register_job(app, job_id, child, output)?;

    let mut code = None;
    let mut terminated = false;
    while let Some(event) = rx.recv().await {
        if let CommandEvent::Terminated(status) = event {
            code = status.code;
            terminated = true;
            break;
        }
    }
    unregister_job(app, job_id);
    if !terminated {
        return Err(format!("FFmpeg {} process ended unexpectedly", what));
    }
    Ok(code)
}

/// Decode an AVIF or HEIC/HEIF file to a temporary QOI via FFmpeg (the image crate
/// cannot decode these). The temp file is removed if the decode fails.
async fn decode_via_ffmpeg(
    app: &AppHandle,
    job_id: &str,
    job_output: &str,
    input: &str,
    kind: &str,
) -> Result<String, String> {
    let temp_path = image_decode_temp_path(kind);
    let temp_str = temp_path.to_string_lossy().to_string();
    let args = build_ffmpeg_image_decode_args(input, &temp_str);

    let what = format!("{} decode", kind.to_uppercase());
    let code = run_ffmpeg_helper(app, job_id, job_output, &args, &what).await;
    match code {
        Ok(Some(0)) => Ok(temp_str),
        Ok(code) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(format!("FFmpeg {} failed (code {:?})", what, code))
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            Err(e)
        }
    }
}

/// Everything after `Started`: decode (if needed), encode, clean up the decode temp.
/// Kept as one fallible function so the caller can turn any `Err` into a proper
/// `Error` event instead of propagating it and leaving the file "processing".
async fn encode_image(
    app: &AppHandle,
    job_id: &str,
    input: &str,
    output: &str,
    options: ImageOptions,
    encoder_threads: usize,
) -> Result<(), String> {
    let decode_temp = if is_avif_input(input) {
        Some(decode_via_ffmpeg(app, job_id, output, input, "avif").await?)
    } else if is_heic_input(input) {
        Some(decode_via_ffmpeg(app, job_id, output, input, "heic").await?)
    } else {
        None
    };
    let effective_input = decode_temp.as_deref().unwrap_or(input);

    // AVIF with metadata preservation routes through FFmpeg sidecar
    let needs_ffmpeg_avif = matches!(options.format, ImageFormat::Avif) && !options.strip_metadata;

    let result = if needs_ffmpeg_avif {
        // For AVIF output with metadata, use original input (FFmpeg handles the full pipeline)
        compress_avif_with_ffmpeg(app, job_id, input, output, options.quality).await
    } else {
        let input_for_compress = effective_input.to_string();
        let output_clone = output.to_string();
        let native_guard = app
            .try_state::<NativeJobs>()
            .map(|n| NativeJobGuard::new(&n));
        let joined = tokio::task::spawn_blocking(move || {
            let _guard = native_guard;
            img_compress::compress_with_threads(
                &input_for_compress,
                &output_clone,
                &options,
                encoder_threads,
            )
        })
        .await;
        match joined {
            Ok(r) => r,
            Err(e) => Err(format!("Task join error: {}", e)),
        }
    };

    if let Some(ref temp) = decode_temp {
        let _ = tokio::fs::remove_file(temp).await;
    }
    result
}

/// Compress one image. `encoder_threads` bounds the threads used by encoders that
/// parallelize internally (AVIF, PNG) so a batch of N concurrent jobs does not
/// oversubscribe the machine.
pub async fn compress_image(
    app: AppHandle,
    input: String,
    output: String,
    options: ImageOptions,
    encoder_threads: usize,
    on_progress: Channel<ProgressEvent>,
) -> Result<CompressionResult, String> {
    let job_id = Uuid::new_v4().to_string();
    tracing::info!(input = %input, format = ?options.format, "Starting image compression");
    let file_name = Path::new(&input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = tokio::fs::metadata(&input)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // Ensure the output directory exists (needed for subfolder export mode)
    if let Some(parent) = Path::new(&output).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    // Atomic claim with auto-cleanup of the 0-byte marker on every exit path.
    let _output_claim = OutputClaim::claim(&output)?;
    let output = _output_claim.path().to_string();

    let _ = on_progress.send(ProgressEvent::Started {
        job_id: job_id.clone(),
        file_name: file_name.clone(),
        input_path: input.clone(),
    });

    let start = Instant::now();

    // Resolve Original → concrete format using the real input path (before AVIF temp substitution)
    let mut resolved_options = options.clone();
    resolved_options.format = options.format.resolve_for_input(&input);

    // No `?` from here on: every failure must produce a terminal event.
    let compression_result = encode_image(
        &app,
        &job_id,
        &input,
        &output,
        resolved_options,
        encoder_threads,
    )
    .await;

    let duration_ms = start.elapsed().as_millis() as u64;

    match compression_result {
        Ok(()) => {
            let output_size = tokio::fs::metadata(&output)
                .await
                .map(|m| m.len())
                .unwrap_or(0);

            // If compressed is larger, keep the smaller original.
            let output_size =
                keep_original_if_larger(&input, &output, input_size, output_size).await;

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
            // A partially written output is worse than none.
            if let Err(rm) = tokio::fs::remove_file(&output).await {
                if rm.kind() != std::io::ErrorKind::NotFound {
                    tracing::warn!(path = %output, error = %rm, "Failed to remove failed image output");
                }
            }
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

/// Map quality 0-100 to CRF 63-0 (FFmpeg libaom-av1: lower CRF = higher quality).
fn avif_crf_for_quality(quality: u8) -> u8 {
    let inverted = 100u32.saturating_sub(quality as u32);
    (inverted * 63 / 100).min(63) as u8
}

async fn compress_avif_with_ffmpeg(
    app: &AppHandle,
    job_id: &str,
    input: &str,
    output: &str,
    quality: u8,
) -> Result<(), String> {
    let crf = avif_crf_for_quality(quality);

    let args: Vec<String> = vec![
        "-nostdin".into(),
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

    match run_ffmpeg_helper(app, job_id, output, &args, "AVIF encode").await? {
        Some(0) => Ok(()),
        code => Err(format!(
            "FFmpeg AVIF encoding failed (code {:?}). Metadata may not be supported.",
            code
        )),
    }
}

#[tauri::command]
pub async fn compress_images_batch(
    app: AppHandle,
    files: Vec<BatchEntry>,
    options: ImageOptions,
    on_progress: Channel<ProgressEvent>,
) -> Result<Vec<CompressionResult>, String> {
    validate_image_options(&options)?;

    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let max_concurrent = cores.min(8);
    // Give each in-flight encoder a slice of the machine rather than letting every
    // AVIF/PNG job spin up its own full-size thread pool.
    let encoder_threads = image_encoder_threads(cores, max_concurrent, files.len());
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let mut handles = Vec::with_capacity(files.len());
    let mut entry_paths = Vec::with_capacity(files.len());

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
        entry_paths.push((entry.input.clone(), entry.output.clone()));
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
            compress_image(
                app_clone,
                entry.input,
                entry.output,
                opts,
                encoder_threads,
                channel_clone,
            )
            .await
        });
        handles.push(handle);
    }

    let mut results = Vec::with_capacity(handles.len());
    for (handle, (input_path, output_path)) in handles.into_iter().zip(entry_paths) {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => {
                // Setup-level failure (output dir, claim): no Started/Error event was
                // sent, so the result carries the paths for the frontend to reconcile.
                tracing::warn!(error = %e, input = %input_path, "Image task failed before start, continuing batch");
                results.push(CompressionResult {
                    job_id: Uuid::new_v4().to_string(),
                    input_path,
                    output_path,
                    input_size: 0,
                    output_size: 0,
                    duration_ms: 0,
                    success: false,
                    error: Some(e),
                });
            }
            Err(e) => {
                tracing::warn!(error = %e, input = %input_path, "Image task join error, continuing batch");
                results.push(CompressionResult {
                    job_id: Uuid::new_v4().to_string(),
                    input_path,
                    output_path,
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

/// Threads per encoder: when the batch is wide enough to keep every worker busy,
/// each encoder gets `cores / workers`; a short batch lets the few jobs use more.
fn image_encoder_threads(cores: usize, max_concurrent: usize, batch_len: usize) -> usize {
    let in_flight = batch_len.clamp(1, max_concurrent.max(1));
    (cores / in_flight).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoder_threads_split_cores_across_workers() {
        assert_eq!(image_encoder_threads(8, 8, 100), 1);
        assert_eq!(image_encoder_threads(8, 8, 2), 4);
        assert_eq!(image_encoder_threads(16, 8, 100), 2);
        assert_eq!(image_encoder_threads(4, 4, 1), 4);
        assert_eq!(image_encoder_threads(2, 2, 0), 2);
    }

    #[test]
    fn decode_temp_path_uses_qoi_extension() {
        let temp = image_decode_temp_path("avif");

        assert_eq!(temp.extension().and_then(|e| e.to_str()), Some("qoi"));
    }

    #[test]
    fn ffmpeg_decode_args_use_rgba_for_qoi() {
        let args = build_ffmpeg_image_decode_args("in.avif", "out.qoi");

        assert_eq!(args.first().map(String::as_str), Some("-nostdin"));
        assert!(args.windows(2).any(|pair| pair == ["-pix_fmt", "rgba"]));
        assert_eq!(args.last().map(String::as_str), Some("out.qoi"));
    }

    #[test]
    fn avif_crf_never_wraps() {
        assert_eq!(avif_crf_for_quality(100), 0);
        assert_eq!(avif_crf_for_quality(0), 63);
        assert_eq!(avif_crf_for_quality(50), 31);
        // Out-of-range quality (rejected upstream by validation) still cannot wrap.
        assert_eq!(avif_crf_for_quality(255), 0);
    }
}
