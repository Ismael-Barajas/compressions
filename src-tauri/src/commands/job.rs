//! Shared plumbing for sidecar-backed compression jobs (FFmpeg, Ghostscript).
//!
//! Every media command used to carry its own copy of the same ~100-line sequence:
//! claim output → spawn → register for cancellation → stream stderr → on exit compute
//! sizes → keep-original-if-larger → emit `Completed`/`Error` → append history.
//! This module owns that sequence once so the per-media commands only describe
//! *what* to run.

use std::future::Future;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Semaphore;
use uuid::Uuid;

use crate::compression::progress::{ProgressParser, ProgressThrottle};
use crate::ffmpeg::probe::probe_video_duration;
use crate::history::storage as history;
use crate::state::{AppState, CancelFlag};
use crate::types::{BatchEntry, CompressionResult, HistoryEntry, ProgressEvent, ProgressPayload};
use crate::utils::OutputClaim;

/// Everything known about a job before its sidecar is spawned.
pub struct JobContext {
    pub job_id: String,
    pub input: String,
    /// Final (conflict-resolved) output path.
    pub output: String,
    pub file_name: String,
    pub input_size: u64,
    pub media_type: &'static str,
    // Held for the lifetime of the job: its Drop removes an orphaned 0-byte marker.
    _claim: OutputClaim,
}

/// Resolve the output path, create its directory, and claim it atomically.
pub async fn prepare_job(
    input: &str,
    output: &str,
    media_type: &'static str,
) -> Result<JobContext, String> {
    let file_name = Path::new(input)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let input_size = tokio::fs::metadata(input)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    // Ensure the output directory exists (needed for subfolder export mode).
    if let Some(parent) = Path::new(output).parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create output directory: {}", e))?;
    }

    let claim = OutputClaim::claim(output);
    let output = claim.path().to_string();

    Ok(JobContext {
        job_id: Uuid::new_v4().to_string(),
        input: input.to_string(),
        output,
        file_name,
        input_size,
        media_type,
        _claim: claim,
    })
}

/// Use the duration the frontend already probed when adding the file; only spawn
/// ffprobe when no hint is available (e.g. a file whose probe failed or timed out).
pub async fn resolve_duration(app: &AppHandle, path: &str, hint: Option<f64>) -> f64 {
    match hint {
        Some(d) if d.is_finite() && d > 0.0 => d,
        _ => probe_video_duration(app, path).await.unwrap_or(0.0),
    }
}

pub struct SidecarSpec<'a> {
    pub sidecar: &'static str,
    pub args: &'a [String],
    /// When `Some`, stderr is parsed as FFmpeg `-progress` output and forwarded
    /// (throttled) on the channel using this total duration.
    pub progress: Option<(f64, &'a Channel<ProgressEvent>)>,
    /// Keep the last few KB of stderr for error reporting (Ghostscript).
    pub capture_stderr: bool,
}

pub struct SidecarOutcome {
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    pub stderr: String,
}

const STDERR_CAP: usize = 4096;

/// Spawn a sidecar, register it for cancellation under `ctx.job_id`, stream its
/// stderr until it exits, and unregister it. Blocks only on the event channel.
pub async fn run_sidecar(
    app: &AppHandle,
    ctx: &JobContext,
    spec: SidecarSpec<'_>,
) -> Result<SidecarOutcome, String> {
    let (mut rx, child) = app
        .shell()
        .sidecar(spec.sidecar)
        .map_err(|e| format!("Failed to create {} sidecar: {}", spec.sidecar, e))?
        .args(spec.args)
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", spec.sidecar, e))?;

    register_job(app, &ctx.job_id, child, &ctx.output)?;

    let start = Instant::now();
    let mut parser = spec.progress.map(|(total, _)| ProgressParser::new(total));
    let mut throttle = ProgressThrottle::default();
    let mut stderr_tail = String::new();
    let mut exit_code = None;
    let mut terminated = false;

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                if let (Some(parser), Some((_, channel))) = (parser.as_mut(), spec.progress) {
                    if let Some(p) = parser.feed_line(&line) {
                        if throttle.should_send(p.percent) {
                            let _ = channel.send(ProgressEvent::Progress(ProgressPayload {
                                job_id: ctx.job_id.clone(),
                                file_name: ctx.file_name.clone(),
                                percent: p.percent,
                                current_frame: p.current_frame,
                                total_frames: None,
                                speed: p.speed,
                                eta_seconds: p.eta_seconds,
                            }));
                        }
                    }
                }
                if spec.capture_stderr && stderr_tail.len() < STDERR_CAP {
                    let remaining = STDERR_CAP - stderr_tail.len();
                    if line.len() <= remaining {
                        stderr_tail.push_str(&line);
                        stderr_tail.push('\n');
                    } else {
                        stderr_tail.push_str(&line[..remaining]);
                        stderr_tail.push_str("... [truncated]");
                    }
                }
            }
            CommandEvent::Terminated(status) => {
                exit_code = status.code;
                terminated = true;
                break;
            }
            _ => {}
        }
    }

    unregister_job(app, &ctx.job_id);

    if !terminated {
        return Err(format!("{} process ended unexpectedly", spec.sidecar));
    }

    Ok(SidecarOutcome {
        exit_code,
        duration_ms: start.elapsed().as_millis() as u64,
        stderr: stderr_tail,
    })
}

fn register_job(
    app: &AppHandle,
    job_id: &str,
    child: tauri_plugin_shell::process::CommandChild,
    output: &str,
) -> Result<(), String> {
    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "AppState not managed".to_string())?;
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    guard
        .active_jobs
        .insert(job_id.to_string(), (child, output.to_string()));
    Ok(())
}

fn unregister_job(app: &AppHandle, job_id: &str) {
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        if let Ok(mut guard) = state.lock() {
            guard.active_jobs.remove(job_id);
        }
    }
}

/// If the encoder produced a file no smaller than the input, replace it with a copy
/// of the original so the user always gets the smaller file. Uses a filesystem
/// reflink where supported (APFS, Btrfs, XFS, ReFS) so multi-GB videos are cloned
/// in milliseconds; falls back to a regular copy. Runs off the async runtime.
pub async fn keep_original_if_larger(
    input: &str,
    output: &str,
    input_size: u64,
    output_size: u64,
) -> u64 {
    if input_size == 0 || output_size < input_size {
        return output_size;
    }
    let input_owned = input.to_string();
    let output_owned = output.to_string();
    let copied = tokio::task::spawn_blocking(move || {
        // Remove the bloated output first: reflink refuses to overwrite.
        let _ = std::fs::remove_file(&output_owned);
        reflink_copy::reflink_or_copy(&input_owned, &output_owned).map(|_| ())
    })
    .await;
    match copied {
        Ok(Ok(())) => input_size,
        Ok(Err(e)) => {
            tracing::warn!(path = %output, error = %e, "Failed to copy original over bloated output");
            output_size
        }
        Err(e) => {
            tracing::warn!(error = %e, "Join error copying original");
            output_size
        }
    }
}

/// Turn a finished sidecar run into a `CompressionResult`, emit the terminal event,
/// and record history. Cancel-aware: when the child died because of Cancel All, no
/// error event is emitted and nothing is written to history.
pub async fn finish_job(
    app: &AppHandle,
    ctx: &JobContext,
    exit_code: Option<i32>,
    duration_ms: u64,
    error_detail: Option<String>,
    on_progress: &Channel<ProgressEvent>,
) -> CompressionResult {
    let success = exit_code == Some(0);
    let mut output_size = tokio::fs::metadata(&ctx.output)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    if success {
        output_size =
            keep_original_if_larger(&ctx.input, &ctx.output, ctx.input_size, output_size).await;
    }

    let result = CompressionResult {
        job_id: ctx.job_id.clone(),
        input_path: ctx.input.clone(),
        output_path: ctx.output.clone(),
        input_size: ctx.input_size,
        output_size,
        duration_ms,
        success,
        error: if success {
            None
        } else {
            Some(
                error_detail
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| format!("Process exited with code {:?}", exit_code)),
            )
        },
    };

    let cancelled = crate::commands::queue::is_cancelled(app);
    if success {
        let _ = on_progress.send(ProgressEvent::Completed(result.clone()));
        tracing::info!(
            media = ctx.media_type,
            input = %result.input_path,
            output_size = result.output_size,
            duration_ms = result.duration_ms,
            "Job completed"
        );
    } else {
        if let Err(e) = tokio::fs::remove_file(&ctx.output).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %ctx.output, error = %e, "Failed to remove failed output");
            }
        }
        if cancelled {
            tracing::info!(media = ctx.media_type, input = %result.input_path, "Job cancelled by user");
        } else {
            let _ = on_progress.send(ProgressEvent::Error {
                job_id: ctx.job_id.clone(),
                message: result.error.clone().unwrap_or_default(),
            });
            tracing::warn!(media = ctx.media_type, input = %result.input_path, error = ?result.error, "Job failed");
        }
    }

    if success || !cancelled {
        if let Err(e) =
            history::append_entry(app, HistoryEntry::from_result(&result, ctx.media_type))
        {
            tracing::warn!(error = %e, "Failed to save history entry");
        }
    }

    result
}

pub fn send_started(ctx: &JobContext, on_progress: &Channel<ProgressEvent>) {
    let _ = on_progress.send(ProgressEvent::Started {
        job_id: ctx.job_id.clone(),
        file_name: ctx.file_name.clone(),
        input_path: ctx.input.clone(),
    });
}

/// Number of sidecar jobs to run at once for encoders that are single-threaded
/// (audio codecs, Ghostscript). Leaves headroom for the UI and the webview.
pub fn single_threaded_batch_concurrency(cap: usize) -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
        .saturating_sub(1)
        .clamp(1, cap)
}

/// Run `job` for every entry with at most `max_concurrent` in flight. Results are
/// returned in input order. Honors `CancelFlag`: entries that have not started when
/// Cancel All fires are skipped (reported as failed, cancelled).
pub async fn run_batch<F, Fut>(
    app: &AppHandle,
    files: Vec<BatchEntry>,
    max_concurrent: usize,
    job: F,
) -> Vec<CompressionResult>
where
    F: Fn(AppHandle, BatchEntry) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompressionResult, String>> + Send + 'static,
{
    let job = Arc::new(job);
    let semaphore = Arc::new(Semaphore::new(max_concurrent.max(1)));
    let cancel_flag = app
        .try_state::<CancelFlag>()
        .map(|s| s.0.clone())
        .unwrap_or_default();

    let mut handles = Vec::with_capacity(files.len());
    let mut entry_paths = Vec::with_capacity(files.len());
    for entry in files {
        if cancel_flag.load(Ordering::SeqCst) {
            break;
        }
        let app = app.clone();
        let job = Arc::clone(&job);
        let sem = Arc::clone(&semaphore);
        let cancel = Arc::clone(&cancel_flag);
        let input_path = entry.input.clone();
        let output_path = entry.output.clone();
        handles.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string())?;
            // Re-check after waiting for a permit: the user may have cancelled meanwhile.
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
            job(app, entry).await
        }));
        entry_paths.push((input_path, output_path));
    }

    let mut results = Vec::with_capacity(handles.len());
    for (handle, (input_path, output_path)) in handles.into_iter().zip(entry_paths) {
        match handle.await {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(e)) => {
                // Setup-level failure (validation, spawn): no Started/Error event was
                // ever sent, so the result itself carries the paths for the frontend
                // to reconcile against.
                tracing::warn!(error = %e, input = %input_path, "Batch job failed before start, continuing batch");
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
                tracing::warn!(error = %e, "Batch job join error, continuing batch");
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
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn keep_original_replaces_bloated_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.bin");
        let output = dir.path().join("out.bin");
        std::fs::write(&input, b"original").unwrap();
        std::fs::write(&output, b"much larger output").unwrap();

        let size =
            keep_original_if_larger(input.to_str().unwrap(), output.to_str().unwrap(), 8, 18).await;

        assert_eq!(size, 8);
        assert_eq!(std::fs::read(&output).unwrap(), b"original");
    }

    #[tokio::test]
    async fn keep_original_leaves_smaller_output_alone() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.bin");
        let output = dir.path().join("out.bin");
        std::fs::write(&input, b"a much larger original").unwrap();
        std::fs::write(&output, b"small").unwrap();

        let size =
            keep_original_if_larger(input.to_str().unwrap(), output.to_str().unwrap(), 22, 5).await;

        assert_eq!(size, 5);
        assert_eq!(std::fs::read(&output).unwrap(), b"small");
    }

    #[test]
    fn batch_concurrency_is_bounded() {
        let c = single_threaded_batch_concurrency(4);
        assert!((1..=4).contains(&c));
        assert_eq!(single_threaded_batch_concurrency(1), 1);
    }
}
