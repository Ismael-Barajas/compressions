//! Shared plumbing for sidecar-backed compression jobs (FFmpeg, Ghostscript).
//!
//! Every media command used to carry its own copy of the same ~100-line sequence:
//! claim output → spawn → register for cancellation → stream stderr → on exit compute
//! sizes → keep-original-if-larger → emit `Completed`/`Error` → append history.
//! This module owns that sequence once so the per-media commands only describe
//! *what* to run.

use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tauri::{ipc::Channel, AppHandle, Manager};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use uuid::Uuid;

use crate::compression::progress::{ProgressParser, ProgressThrottle};
use crate::ffmpeg::probe::probe_video_duration;
use crate::history::storage as history;
use crate::state::{AppState, CancelFlag, PauseFlag};
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

    let claim = OutputClaim::claim(output)?;
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

/// Where a sidecar's parsed FFmpeg progress goes. A job made of several FFmpeg runs
/// (two-pass GIF) maps each run onto its own slice of the bar via `from..to`.
#[derive(Clone, Copy)]
pub struct ProgressTarget<'a> {
    pub total_duration: f64,
    pub channel: &'a Channel<ProgressEvent>,
    pub from: f32,
    pub to: f32,
}

impl<'a> ProgressTarget<'a> {
    pub fn full(total_duration: f64, channel: &'a Channel<ProgressEvent>) -> Self {
        Self::range(total_duration, channel, 0.0, 100.0)
    }

    pub fn range(
        total_duration: f64,
        channel: &'a Channel<ProgressEvent>,
        from: f32,
        to: f32,
    ) -> Self {
        Self {
            total_duration,
            channel,
            from,
            to,
        }
    }

    fn scale(&self, percent: f32) -> f32 {
        self.from + percent * (self.to - self.from) / 100.0
    }
}

pub struct SidecarSpec<'a> {
    pub sidecar: &'static str,
    pub args: &'a [String],
    /// When `Some`, stderr is parsed as FFmpeg `-progress` output and forwarded
    /// (throttled) on the target's channel.
    pub progress: Option<ProgressTarget<'a>>,
    /// Keep the first few KB of the child's output (stderr and stdout) for error
    /// reporting. Ghostscript prints some errors to stdout.
    pub capture_output: bool,
}

pub struct SidecarOutcome {
    pub exit_code: Option<i32>,
    pub duration_ms: u64,
    /// Captured stderr + stdout when `capture_output` was set.
    pub output: String,
    /// The user stopped this specific job (per-file Stop). Distinct from Cancel All,
    /// which is a global flag read via `queue::is_cancelled`.
    pub cancelled: bool,
}

const STDERR_CAP: usize = 4096;

/// Spawn a sidecar, register it for cancellation under `ctx.job_id`, stream its
/// stderr until it exits, and unregister it. Blocks only on the event channel.
///
/// Cancel All is checked both before the spawn and right after registration: a
/// cancel that lands between the two would otherwise miss the child entirely.
pub async fn run_sidecar(
    app: &AppHandle,
    ctx: &JobContext,
    spec: SidecarSpec<'_>,
) -> Result<SidecarOutcome, String> {
    if crate::commands::queue::is_cancelled(app) {
        return Err("Cancelled".to_string());
    }

    let (mut rx, child) = app
        .shell()
        .sidecar(spec.sidecar)
        .map_err(|e| format!("Failed to create {} sidecar: {}", spec.sidecar, e))?
        .args(spec.args)
        .spawn()
        .map_err(|e| format!("Failed to spawn {}: {}", spec.sidecar, e))?;

    // Windowless sidecars are "background" to Windows and land on the E-cores.
    crate::qos::opt_out_child(child.pid());

    if let Err(e) = register_job(app, &ctx.job_id, child, &ctx.output) {
        // Never leave an unregistered child running.
        kill_job(app, &ctx.job_id);
        return Err(e);
    }

    if crate::commands::queue::is_cancelled(app) {
        // Cancel All drained `active_jobs` before we registered: kill it ourselves.
        kill_job(app, &ctx.job_id);
    }

    let start = Instant::now();
    let mut parser = spec.progress.map(|t| ProgressParser::new(t.total_duration));
    let mut throttle = ProgressThrottle::default();
    let mut captured = String::new();
    let mut exit_code = None;
    let mut terminated = false;

    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stderr(bytes) => {
                let line = String::from_utf8_lossy(&bytes);
                if let (Some(parser), Some(target)) = (parser.as_mut(), spec.progress) {
                    if let Some(p) = parser.feed_line(&line) {
                        if throttle.should_send(p.percent) {
                            let _ = target
                                .channel
                                .send(ProgressEvent::Progress(ProgressPayload {
                                    job_id: ctx.job_id.clone(),
                                    file_name: ctx.file_name.clone(),
                                    percent: target.scale(p.percent),
                                    current_frame: p.current_frame,
                                    total_frames: None,
                                    speed: p.speed,
                                    eta_seconds: p.eta_seconds,
                                }));
                        }
                    }
                }
                if spec.capture_output {
                    capture_line(&mut captured, &line);
                }
            }
            CommandEvent::Stdout(bytes) => {
                if spec.capture_output {
                    capture_line(&mut captured, &String::from_utf8_lossy(&bytes));
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

    let cancelled = unregister_job(app, &ctx.job_id);

    if !terminated {
        return Err(format!("{} process ended unexpectedly", spec.sidecar));
    }

    Ok(SidecarOutcome {
        exit_code,
        duration_ms: start.elapsed().as_millis() as u64,
        output: captured,
        cancelled,
    })
}

/// Append one line of child output, up to `STDERR_CAP` bytes in total.
fn capture_line(captured: &mut String, line: &str) {
    if captured.len() >= STDERR_CAP {
        return;
    }
    let remaining = STDERR_CAP - captured.len();
    if line.len() <= remaining {
        captured.push_str(line);
        captured.push('\n');
    } else {
        captured.push_str(truncate_at_char_boundary(line, remaining));
        captured.push_str("... [truncated]");
    }
}

/// Longest prefix of `s` that is at most `max_bytes` long and ends on a char boundary.
fn truncate_at_char_boundary(s: &str, max_bytes: usize) -> &str {
    let mut end = max_bytes.min(s.len());
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Insert the child into `active_jobs`. If the user already pressed Stop on this job
/// id (possible when the request lands during spawn), the child is killed at once.
pub(crate) fn register_job(
    app: &AppHandle,
    job_id: &str,
    child: tauri_plugin_shell::process::CommandChild,
    output: &str,
) -> Result<(), String> {
    let state = app
        .try_state::<Mutex<AppState>>()
        .ok_or_else(|| "AppState not managed".to_string())?;
    let mut guard = state.lock().map_err(|e| e.to_string())?;
    if guard.cancelled_jobs.contains(job_id) {
        tracing::info!(job_id = %job_id, "Job was stopped before its process registered; killing");
        let _ = child.kill();
        return Ok(());
    }
    guard
        .active_jobs
        .insert(job_id.to_string(), (child, output.to_string()));
    Ok(())
}

/// Remove the job from `active_jobs` and report whether it had been stopped by a
/// per-file Stop (clearing that mark).
pub(crate) fn unregister_job(app: &AppHandle, job_id: &str) -> bool {
    let Some(state) = app.try_state::<Mutex<AppState>>() else {
        return false;
    };
    let Ok(mut guard) = state.lock() else {
        return false;
    };
    guard.active_jobs.remove(job_id);
    guard.cancelled_jobs.remove(job_id)
}

/// Kill a registered child (no-op if it is not registered).
fn kill_job(app: &AppHandle, job_id: &str) {
    if let Some(state) = app.try_state::<Mutex<AppState>>() {
        if let Ok(mut guard) = state.lock() {
            if let Some((child, _)) = guard.active_jobs.remove(job_id) {
                let _ = child.kill();
            }
        }
    }
}

/// If the encoder produced a file larger than the input, replace it with a copy of
/// the original so the user always gets the smaller file. Only when the output is
/// the same format as the input: a conversion (PNG → JPEG, MP3 → WAV, video → GIF,
/// extracted audio) is what the user asked for, and copying the original's bytes
/// under the new extension would produce a mislabeled file. Uses a filesystem reflink
/// where supported (APFS, Btrfs, XFS, ReFS) so multi-GB videos are cloned in
/// milliseconds; falls back to a regular copy. Runs off the async runtime.
///
/// The copy goes to a sibling temp file that is then renamed over the output, so a
/// failed copy (disk full, locked file) leaves the compressed output in place
/// instead of leaving nothing at all.
pub async fn keep_original_if_larger(
    input: &str,
    output: &str,
    input_size: u64,
    output_size: u64,
) -> u64 {
    if input_size == 0 || output_size <= input_size {
        return output_size;
    }
    if !same_format(input, output) {
        tracing::info!(input = %input, output = %output, "Converted output is larger than the input; keeping it (different format)");
        return output_size;
    }
    let input_owned = input.to_string();
    let output_owned = output.to_string();
    let copied = tokio::task::spawn_blocking(move || {
        let tmp = format!("{}.orig.tmp", output_owned);
        let _ = std::fs::remove_file(&tmp);
        let result = reflink_copy::reflink_or_copy(&input_owned, &tmp)
            .map(|_| ())
            .and_then(|()| std::fs::rename(&tmp, &output_owned));
        if result.is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
        result
    })
    .await;
    match copied {
        Ok(Ok(())) => input_size,
        Ok(Err(e)) => {
            tracing::warn!(path = %output, error = %e, "Failed to copy original over bloated output; keeping compressed file");
            output_size
        }
        Err(e) => {
            tracing::warn!(error = %e, "Join error copying original");
            output_size
        }
    }
}

/// True when both paths have the same file format by extension (case-insensitive,
/// treating `.jpg`/`.jpeg` and `.tif`/`.tiff` as one format).
fn same_format(a: &str, b: &str) -> bool {
    fn normalized_ext(path: &str) -> Option<String> {
        let ext = Path::new(path).extension()?.to_str()?.to_ascii_lowercase();
        Some(match ext.as_str() {
            "jpg" => "jpeg".to_string(),
            "tif" => "tiff".to_string(),
            _ => ext,
        })
    }
    match (normalized_ext(a), normalized_ext(b)) {
        (Some(x), Some(y)) => x == y,
        _ => false,
    }
}

/// Plain-language reason for a failed FFmpeg run. FFmpeg exits with negated AVERROR
/// codes, which mean nothing to a user as raw numbers.
pub fn ffmpeg_failure_message(exit_code: Option<i32>) -> String {
    match exit_code {
        // AVERROR_INVALIDDATA ("INDA")
        Some(-1094995529) => {
            "Could not read the file: it is corrupt or not a supported format".to_string()
        }
        // AVERROR(EINVAL): the muxer/encoder refused the settings, or the requested
        // stream (e.g. audio when extracting) does not exist.
        Some(-22) => "FFmpeg could not apply these settings to this file (unsupported codec for the container, or no matching audio/video track)".to_string(),
        Some(-2) => "File not found".to_string(),
        Some(-13) => "Permission denied".to_string(),
        Some(code) => format!("FFmpeg failed (exit code {})", code),
        None => "FFmpeg was stopped before it finished".to_string(),
    }
}

/// Turn a finished sidecar run into a `CompressionResult`, emit the terminal event,
/// and record history. Cancel-aware: when the child died because of Cancel All or a
/// per-file Stop (`job_cancelled`), no error event is emitted and nothing is written
/// to history.
pub async fn finish_job(
    app: &AppHandle,
    ctx: &JobContext,
    exit_code: Option<i32>,
    duration_ms: u64,
    error_detail: Option<String>,
    job_cancelled: bool,
    on_progress: &Channel<ProgressEvent>,
) -> CompressionResult {
    let success = exit_code == Some(0) && !job_cancelled;
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
                    .unwrap_or_else(|| match exit_code {
                        Some(code) => format!("Process exited with code {}", code),
                        None => "Process was stopped before it finished".to_string(),
                    }),
            )
        },
    };

    let cancelled = job_cancelled || crate::commands::queue::is_cancelled(app);
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
///
/// Measured throughput keeps rising up to half the logical cores, so the cap is
/// `cores / 2`, kept between `min_cap` and `max_cap`.
pub fn single_threaded_batch_concurrency(min_cap: usize, max_cap: usize) -> usize {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2);
    let concurrency = batch_concurrency_for_cores(cores, min_cap, max_cap);
    tracing::info!(cores, concurrency, "Single-threaded batch concurrency");
    concurrency
}

fn batch_concurrency_for_cores(cores: usize, min_cap: usize, max_cap: usize) -> usize {
    let cap = (cores / 2).clamp(min_cap, max_cap);
    cores.saturating_sub(1).clamp(1, cap)
}

/// While the queue is paused, hold off starting the next file. Jobs already running
/// finish normally. Cancel All releases the wait.
async fn wait_while_paused(pause: &AtomicBool, cancel: &AtomicBool) {
    let mut logged = false;
    while pause.load(Ordering::SeqCst) && !cancel.load(Ordering::SeqCst) {
        if !logged {
            tracing::info!("Queue paused; waiting before starting the next file");
            logged = true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    if logged {
        tracing::info!("Queue resumed");
    }
}

/// Wait until the next entry may start: holding as many permits as `plan` asks for
/// (capped at `total_permits`), not paused, and not cancelled. Returns `None` on
/// Cancel All. Permits are taken one wait at a time and `plan` re-asked after each,
/// so the answer that is returned is the one current when the entry starts. Only
/// the batch loop takes permits, so a partly filled request cannot deadlock: it is
/// completed as running entries finish.
async fn acquire_slot<T>(
    semaphore: &Arc<Semaphore>,
    total_permits: u32,
    pause: &AtomicBool,
    cancel: &AtomicBool,
    mut plan: impl FnMut() -> (u32, T),
) -> Option<(OwnedSemaphorePermit, T)> {
    let mut permit = Arc::clone(semaphore).acquire_owned().await.ok()?;
    loop {
        wait_while_paused(pause, cancel).await;
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let (wanted, planned) = plan();
        let wanted = wanted.clamp(1, total_permits);
        let held = u32::try_from(permit.num_permits()).unwrap_or(u32::MAX);
        if wanted <= held {
            return Some((permit, planned));
        }
        let more = Arc::clone(semaphore)
            .acquire_many_owned(wanted - held)
            .await
            .ok()?;
        permit.merge(more);
    }
}

/// Run `job` for every entry with at most `max_concurrent` in flight. Results are
/// returned in input order and entries start in input order. Honors `PauseFlag`
/// (no new entry starts while paused) and `CancelFlag` (entries that have not
/// started when Cancel All fires are dropped from the results; the frontend has
/// already reset them to queued).
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
    let permits = u32::try_from(max_concurrent).unwrap_or(u32::MAX);
    run_batch_planned(
        app,
        files,
        permits,
        |_| (1, ()),
        move |app, entry, ()| job(app, entry),
    )
    .await
}

/// [`run_batch`] where entries are not all the same weight. Just before an entry
/// starts, `plan` says how many of the `total_permits` it needs and returns a value
/// handed to `job` (video: the encoder it will use). An entry that needs more
/// permits than are free waits, still in queue order and still unstarted, until
/// running entries finish. `plan` is asked again after every wait, so a change in
/// the meantime (an encoder disabled, hardware detection finishing) is seen.
pub async fn run_batch_planned<P, T, F, Fut>(
    app: &AppHandle,
    files: Vec<BatchEntry>,
    total_permits: u32,
    plan: P,
    job: F,
) -> Vec<CompressionResult>
where
    P: Fn(&AppHandle) -> (u32, T) + Send + Sync,
    T: Send + 'static,
    F: Fn(AppHandle, BatchEntry, T) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<CompressionResult, String>> + Send + 'static,
{
    let job = Arc::new(job);
    let total_permits = total_permits.max(1);
    let semaphore = Arc::new(Semaphore::new(total_permits as usize));
    let cancel_flag = app
        .try_state::<CancelFlag>()
        .map(|s| s.0.clone())
        .unwrap_or_default();

    let pause_flag = app
        .try_state::<PauseFlag>()
        .map(|s| s.0.clone())
        .unwrap_or_default();

    // Slots are taken here, in input order, before each task is spawned. Spawning
    // every task up front and letting them race for the semaphore does not keep
    // order (Tokio runs the most recently spawned task first), which broke the
    // frontend's smallest-first sort for videos.
    let mut handles = Vec::with_capacity(files.len());
    let mut entry_paths = Vec::with_capacity(files.len());
    for entry in files {
        let slot = acquire_slot(&semaphore, total_permits, &pause_flag, &cancel_flag, || {
            plan(app)
        })
        .await;
        let Some((permit, planned)) = slot else {
            tracing::info!(input = %entry.input, "Cancel All: not starting remaining batch entries");
            break;
        };
        let app = app.clone();
        let job = Arc::clone(&job);
        entry_paths.push((entry.input.clone(), entry.output.clone()));
        handles.push(tokio::spawn(async move {
            let _permit = permit;
            job(app, entry, planned).await
        }));
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

    use std::sync::atomic::AtomicU32;
    use std::time::Duration;

    /// True when `fut` has not finished within a short wait.
    async fn still_waiting<F: Future>(fut: &mut std::pin::Pin<Box<F>>) -> bool {
        tokio::time::timeout(Duration::from_millis(150), fut.as_mut())
            .await
            .is_err()
    }

    #[tokio::test]
    async fn two_light_entries_run_together_and_a_third_waits() {
        let gate = Arc::new(Semaphore::new(2));
        let (pause, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
        let a = acquire_slot(&gate, 2, &pause, &cancel, || (1, "a")).await;
        let b = acquire_slot(&gate, 2, &pause, &cancel, || (1, "b")).await;
        assert!(a.is_some() && b.is_some());

        let mut c = Box::pin(acquire_slot(&gate, 2, &pause, &cancel, || (1, "c")));
        assert!(still_waiting(&mut c).await);
        drop(a);
        let (_permit, planned) = c.await.unwrap();
        assert_eq!(planned, "c");
    }

    #[tokio::test]
    async fn heavy_entry_waits_for_the_running_light_one() {
        let gate = Arc::new(Semaphore::new(2));
        let (pause, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
        let light = acquire_slot(&gate, 2, &pause, &cancel, || (1, ())).await;

        // A software encode needs both permits: it holds one and waits for the other,
        // so no third entry can slip in beside the NVENC one either.
        let mut heavy = Box::pin(acquire_slot(&gate, 2, &pause, &cancel, || (2, ())));
        assert!(still_waiting(&mut heavy).await);
        assert_eq!(gate.available_permits(), 0);
        drop(light);
        let (permit, ()) = heavy.await.unwrap();
        assert_eq!(permit.num_permits(), 2);
    }

    #[tokio::test]
    async fn plan_is_asked_again_after_waiting() {
        let gate = Arc::new(Semaphore::new(2));
        let (pause, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
        let running = acquire_slot(&gate, 2, &pause, &cancel, || (1, 0)).await;

        // Asked first while the encoder looks unusable (needs 2), then the answer
        // changes while it waits; what the entry starts with is the latest answer.
        let wanted = AtomicU32::new(2);
        let asked = AtomicU32::new(0);
        let mut next = Box::pin(acquire_slot(&gate, 2, &pause, &cancel, || {
            let n = asked.fetch_add(1, Ordering::SeqCst) + 1;
            (wanted.load(Ordering::SeqCst), n)
        }));
        assert!(still_waiting(&mut next).await);
        wanted.store(1, Ordering::SeqCst);
        drop(running);
        let (_permit, asked_count) = next.await.unwrap();
        assert_eq!(asked_count, 2);
    }

    #[tokio::test]
    async fn waiting_entry_honours_pause_and_cancel() {
        let gate = Arc::new(Semaphore::new(2));
        let (pause, cancel) = (AtomicBool::new(true), AtomicBool::new(false));
        let mut paused = Box::pin(acquire_slot(&gate, 2, &pause, &cancel, || (1, ())));
        assert!(still_waiting(&mut paused).await);
        pause.store(false, Ordering::SeqCst);
        assert!(paused.await.is_some());

        // Cancel All while waiting for permits: the entry never starts.
        let held = acquire_slot(&gate, 2, &pause, &cancel, || (2, ())).await;
        let mut waiting = Box::pin(acquire_slot(&gate, 2, &pause, &cancel, || (1, ())));
        assert!(still_waiting(&mut waiting).await);
        cancel.store(true, Ordering::SeqCst);
        drop(held);
        assert!(waiting.await.is_none());
    }

    #[tokio::test]
    async fn plan_asking_for_more_than_exists_is_capped() {
        let gate = Arc::new(Semaphore::new(2));
        let (pause, cancel) = (AtomicBool::new(false), AtomicBool::new(false));
        let (permit, ()) = acquire_slot(&gate, 2, &pause, &cancel, || (9, ()))
            .await
            .unwrap();
        assert_eq!(permit.num_permits(), 2);
    }

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
    async fn keep_original_keeps_compressed_when_copy_fails() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("out.bin");
        std::fs::write(&output, b"compressed bytes").unwrap();
        let missing_input = dir.path().join("does-not-exist.bin");

        let size = keep_original_if_larger(
            missing_input.to_str().unwrap(),
            output.to_str().unwrap(),
            8,
            16,
        )
        .await;

        // Copy failed: the compressed output must survive and its size is reported.
        assert_eq!(size, 16);
        assert_eq!(std::fs::read(&output).unwrap(), b"compressed bytes");
        assert!(!dir.path().join("out.bin.orig.tmp").exists());
    }

    #[tokio::test]
    async fn keep_original_treats_equal_size_as_compressed() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.bin");
        let output = dir.path().join("out.bin");
        std::fs::write(&input, b"original").unwrap();
        std::fs::write(&output, b"same len").unwrap();

        let size =
            keep_original_if_larger(input.to_str().unwrap(), output.to_str().unwrap(), 8, 8).await;

        assert_eq!(size, 8);
        assert_eq!(std::fs::read(&output).unwrap(), b"same len");
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let s = "héllo wörld";
        for n in 0..=s.len() {
            let t = truncate_at_char_boundary(s, n);
            assert!(t.len() <= n);
            assert!(s.starts_with(t));
        }
        assert_eq!(truncate_at_char_boundary("abc", 10), "abc");
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

    #[tokio::test]
    async fn keep_original_never_relabels_a_conversion() {
        // PNG input, larger JPEG output: the JPEG must stay a JPEG.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.jpeg");
        std::fs::write(&input, b"png").unwrap();
        std::fs::write(&output, b"larger jpeg bytes").unwrap();

        let size =
            keep_original_if_larger(input.to_str().unwrap(), output.to_str().unwrap(), 3, 17).await;

        assert_eq!(size, 17);
        assert_eq!(std::fs::read(&output).unwrap(), b"larger jpeg bytes");
    }

    #[test]
    fn same_format_normalizes_extension_aliases() {
        assert!(same_format("a/photo.JPG", "b/photo_compressed.jpeg"));
        assert!(same_format("scan.tif", "scan_c.TIFF"));
        assert!(same_format("clip.mp4", "clip_compressed.mp4"));
        assert!(!same_format("song.mp3", "song_compressed.wav"));
        assert!(!same_format("clip.mp4", "clip_compressed.gif"));
        assert!(!same_format("noext", "noext_compressed"));
    }

    #[test]
    fn ffmpeg_failure_message_explains_common_codes() {
        assert!(ffmpeg_failure_message(Some(-1094995529)).contains("corrupt"));
        assert!(ffmpeg_failure_message(Some(-22)).contains("settings"));
        assert_eq!(
            ffmpeg_failure_message(Some(1)),
            "FFmpeg failed (exit code 1)"
        );
        assert!(!ffmpeg_failure_message(None).contains("None"));
    }

    #[test]
    fn batch_concurrency_is_bounded() {
        let c = single_threaded_batch_concurrency(4, 16);
        assert!((1..=16).contains(&c));
        assert_eq!(single_threaded_batch_concurrency(1, 1), 1);
    }

    #[test]
    fn audio_batch_concurrency_by_core_count() {
        use crate::commands::audio::{AUDIO_BATCH_MAX, AUDIO_BATCH_MIN};
        let audio = |cores| batch_concurrency_for_cores(cores, AUDIO_BATCH_MIN, AUDIO_BATCH_MAX);
        assert_eq!(audio(2), 1);
        assert_eq!(audio(4), 3);
        assert_eq!(audio(8), 4);
        assert_eq!(audio(16), 8);
        assert_eq!(audio(32), 16);
        assert_eq!(audio(64), 16);
    }

    #[test]
    fn pdf_batch_concurrency_by_core_count() {
        use crate::commands::pdf::{PDF_BATCH_MAX, PDF_BATCH_MIN};
        let pdf = |cores| batch_concurrency_for_cores(cores, PDF_BATCH_MIN, PDF_BATCH_MAX);
        assert_eq!(pdf(2), 1);
        assert_eq!(pdf(4), 3);
        // 8 cores: cap = 8/2 = 4 (the old fixed cap was 3).
        assert_eq!(pdf(8), 4);
        assert_eq!(pdf(16), 8);
        assert_eq!(pdf(32), 12);
    }
}
