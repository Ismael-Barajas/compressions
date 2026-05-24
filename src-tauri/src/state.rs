use std::collections::{HashMap, HashSet};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};
use tauri_plugin_shell::process::CommandChild;

#[derive(Default)]
pub struct AppState {
    pub active_jobs: HashMap<String, (CommandChild, String)>,
}

/// Global cancel flag for the queue drain. Set by `cancel_all`, checked by long-running
/// batch loops (notably image compression) so they can stop spawning new tasks. Lives
/// outside `AppState` so workers can check it without taking the AppState mutex.
#[derive(Default)]
pub struct CancelFlag(pub Arc<AtomicBool>);

/// Hardware encoders detected at startup. Read-heavy, written once.
#[derive(Default)]
pub struct HwEncoders(pub RwLock<HashSet<String>>);

/// Limits concurrent thumbnail generation (especially FFmpeg spawns).
/// Wrapped in Arc so batch tasks can take owned permits via acquire_owned().
pub struct ThumbnailSemaphore(pub Arc<tokio::sync::Semaphore>);
