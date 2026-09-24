use std::path::{Path, PathBuf};
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use uuid::Uuid;

use crate::commands::job::{keep_original_if_larger, run_batch};
use crate::compression::image as img_compress;
use crate::history::storage as history;
use crate::types::{
    BatchEntry, CompressionResult, HistoryEntry, ImageFormat, ImageOptions, ProgressEvent,
    ResizeMode, Resolution,
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

    // AVIF with metadata preservation routes through FFmpeg sidecar, which reads the
    // original input itself.
    let needs_ffmpeg_avif =
        matches!(effective_format, ImageFormat::Avif) && !resolved_options.strip_metadata;

    // The image crate can't decode AVIF/HEIC: the native path decodes those via FFmpeg
    // to a temp QOI first. Failures from here on are reported as a failed job (with an
    // Error event), never as an early `?` return: `Started` has already been sent.
    let decode_temp = if needs_ffmpeg_avif {
        Ok(None)
    } else if is_avif_input(&input) {
        decode_avif_via_ffmpeg(&app, &input).await.map(Some)
    } else if is_heic_input(&input) {
        decode_heic_via_ffmpeg(&app, &input).await.map(Some)
    } else {
        Ok(None)
    };

    let compression_result = match &decode_temp {
        Err(e) => Err(e.clone()),
        Ok(_) if needs_ffmpeg_avif => {
            compress_avif_with_ffmpeg(&app, &input, &output, &resolved_options, encoder_threads)
                .await
        }
        Ok(temp) => {
            let input_for_compress = temp.clone().unwrap_or_else(|| input.clone());
            let output_clone = output.clone();
            tokio::task::spawn_blocking(move || {
                img_compress::compress_with_threads(
                    &input_for_compress,
                    &output_clone,
                    &resolved_options,
                    encoder_threads,
                )
            })
            .await
            .unwrap_or_else(|e| Err(format!("Task join error: {}", e)))
        }
    };

    // Clean up decode temp file (AVIF or HEIC)
    if let Ok(Some(ref temp)) = decode_temp {
        let _ = tokio::fs::remove_file(temp).await;
    }

    let duration_ms = start.elapsed().as_millis() as u64;

    match compression_result {
        Ok(()) => {
            let output_size = tokio::fs::metadata(&output)
                .await
                .map(|m| m.len())
                .unwrap_or(0);

            // If compressed is larger or equal, keep the smaller original.
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

/// libaom's default `cpu-used` (1) is its slowest preset. Measured on a 3 MP image:
/// 63.7 s at the default vs 10.9 s at 6, with the same output size.
const FFMPEG_AVIF_CPU_USED: &str = "6";

/// FFmpeg scale filter matching the native pipeline's resize (`target_dimensions`):
/// `Fit` preserves aspect ratio and treats a zero dimension as unconstrained,
/// `Exact` stretches.
fn ffmpeg_resize_filter(resize: Option<&Resolution>, mode: &ResizeMode) -> Option<String> {
    let r = resize?;
    let scale = match mode {
        ResizeMode::Exact => format!("scale={}:{}", r.width.max(1), r.height.max(1)),
        ResizeMode::Fit => match (r.width, r.height) {
            (0, 0) => return None,
            (w, 0) => format!("scale={}:-1", w),
            (0, h) => format!("scale=-1:{}", h),
            (w, h) => format!("scale={}:{}:force_original_aspect_ratio=decrease", w, h),
        },
    };
    Some(format!("{}:flags=lanczos", scale))
}

fn build_ffmpeg_avif_args(
    input: &str,
    output: &str,
    options: &ImageOptions,
    threads: usize,
) -> Vec<String> {
    // Map quality 0-100 to CRF 63-0 (FFmpeg libaom-av1: lower CRF = higher quality)
    let crf = ((100 - options.quality.min(100)) as f32 * 63.0 / 100.0) as u8;

    let mut args: Vec<String> = vec!["-y".into(), "-i".into(), input.into()];
    if let Some(filter) = ffmpeg_resize_filter(options.resize.as_ref(), &options.resize_mode) {
        args.push("-vf".into());
        args.push(filter);
    }
    args.extend([
        "-c:v".into(),
        "libaom-av1".into(),
        "-crf".into(),
        crf.to_string(),
        "-cpu-used".into(),
        FFMPEG_AVIF_CPU_USED.into(),
        "-row-mt".into(),
        "1".into(),
        "-threads".into(),
        threads.max(1).to_string(),
        "-still-picture".into(),
        "1".into(),
        "-map_metadata".into(),
        "0".into(),
        output.into(),
    ]);
    args
}

async fn compress_avif_with_ffmpeg(
    app: &AppHandle,
    input: &str,
    output: &str,
    options: &ImageOptions,
    threads: usize,
) -> Result<(), String> {
    let args = build_ffmpeg_avif_args(input, output, options, threads);

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
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let max_concurrent = cores.min(8);
    // Give each in-flight encoder a slice of the machine rather than letting every
    // AVIF/PNG job spin up its own full-size thread pool.
    let encoder_threads = image_encoder_threads(cores, max_concurrent, files.len());

    Ok(run_batch(&app, files, max_concurrent, move |app, entry| {
        let options = options.clone();
        let on_progress = on_progress.clone();
        async move {
            compress_image(
                app,
                entry.input,
                entry.output,
                options,
                encoder_threads,
                on_progress,
            )
            .await
        }
    })
    .await)
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

    fn avif_opts(resize: Option<Resolution>, resize_mode: ResizeMode) -> ImageOptions {
        ImageOptions {
            format: ImageFormat::Avif,
            quality: 80,
            resize,
            resize_mode,
            strip_metadata: false,
        }
    }

    #[test]
    fn ffmpeg_avif_uses_fast_preset_threads_and_keeps_metadata() {
        let args =
            build_ffmpeg_avif_args("in.jpg", "out.avif", &avif_opts(None, ResizeMode::Fit), 3);
        assert!(args.windows(2).any(|p| p == ["-cpu-used", "6"]));
        assert!(args.windows(2).any(|p| p == ["-row-mt", "1"]));
        assert!(args.windows(2).any(|p| p == ["-threads", "3"]));
        assert!(args.windows(2).any(|p| p == ["-map_metadata", "0"]));
        assert!(args.windows(2).any(|p| p == ["-crf", "12"]));
        assert!(!args.contains(&"-vf".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("out.avif"));
    }

    #[test]
    fn ffmpeg_avif_applies_resize_like_native_pipeline() {
        let res = |w, h| {
            Some(Resolution {
                width: w,
                height: h,
            })
        };
        let filter = |r, m| ffmpeg_resize_filter(Option::as_ref(&r), &m);
        assert_eq!(
            filter(res(800, 600), ResizeMode::Fit).as_deref(),
            Some("scale=800:600:force_original_aspect_ratio=decrease:flags=lanczos")
        );
        assert_eq!(
            filter(res(800, 0), ResizeMode::Fit).as_deref(),
            Some("scale=800:-1:flags=lanczos")
        );
        assert_eq!(
            filter(res(0, 600), ResizeMode::Fit).as_deref(),
            Some("scale=-1:600:flags=lanczos")
        );
        assert_eq!(filter(res(0, 0), ResizeMode::Fit), None);
        assert_eq!(filter(None, ResizeMode::Fit), None);
        assert_eq!(
            filter(res(800, 600), ResizeMode::Exact).as_deref(),
            Some("scale=800:600:flags=lanczos")
        );

        let args = build_ffmpeg_avif_args(
            "in.jpg",
            "out.avif",
            &avif_opts(res(800, 0), ResizeMode::Fit),
            1,
        );
        let vf = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf + 1], "scale=800:-1:flags=lanczos");
    }

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
