use std::collections::{HashMap, HashSet};
use std::sync::{Arc, RwLock};
use tauri_plugin_shell::process::CommandChild;

#[derive(Default)]
pub struct AppState {
    pub active_jobs: HashMap<String, (CommandChild, String)>,
}

/// Hardware encoders detected at startup. Read-heavy, written once.
#[derive(Default)]
pub struct HwEncoders(pub RwLock<HashSet<String>>);

/// Limits concurrent thumbnail generation (especially FFmpeg spawns).
/// Wrapped in Arc so batch tasks can take owned permits via acquire_owned().
pub struct ThumbnailSemaphore(pub Arc<tokio::sync::Semaphore>);
