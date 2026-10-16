use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use tauri_plugin_shell::process::CommandChild;

#[derive(Default)]
pub struct AppState {
    pub active_jobs: HashMap<String, (CommandChild, String)>,
    /// Job ids the user stopped individually (per-file Stop, not Cancel All).
    /// Lets the job runner tell "killed on purpose" apart from "encoder failed", so a
    /// stopped HW encode is not retried in software and no error is reported.
    pub cancelled_jobs: HashSet<String>,
}

/// Global cancel flag for the queue drain. Set by `cancel_all`, checked by long-running
/// batch loops (notably image compression) so they can stop spawning new tasks. Lives
/// outside `AppState` so workers can check it without taking the AppState mutex.
#[derive(Default)]
pub struct CancelFlag(pub Arc<AtomicBool>);

/// Queue pause flag. Set by `pause_queue`, cleared by `resume_queue` and at the start
/// of every fresh drain. Batch runners check it before starting each file, so Pause
/// lets running files finish but starts nothing new.
#[derive(Default)]
pub struct PauseFlag(pub Arc<AtomicBool>);

/// Hardware encoders detected at startup. Read-heavy, written once.
#[derive(Default)]
pub struct HwEncoders(pub RwLock<HashSet<String>>);

/// Limits concurrent thumbnail generation (especially FFmpeg spawns).
/// Wrapped in Arc so batch tasks can take owned permits via acquire_owned().
pub struct ThumbnailSemaphore(pub Arc<tokio::sync::Semaphore>);

/// Limits concurrent ffprobe/header reads across every `probe_files_batch` call, so
/// several quick drops share one budget instead of each getting its own.
pub struct ProbeSemaphore(pub Arc<tokio::sync::Semaphore>);

/// Number of native (in-process, `spawn_blocking`) image encodes in flight. These
/// have no child process, so `active_jobs` never sees them; the close-window prompt
/// counts them from here.
#[derive(Default)]
pub struct NativeJobs(pub Arc<AtomicUsize>);

/// RAII increment of [`NativeJobs`]; decrements on drop (including on panic unwind).
pub struct NativeJobGuard(Arc<AtomicUsize>);

impl NativeJobGuard {
    pub fn new(counter: &NativeJobs) -> Self {
        counter.0.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(&counter.0))
    }
}

impl Drop for NativeJobGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
