use std::sync::atomic::Ordering;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, State};

use crate::state::{AppState, CancelFlag};

/// True if a queue cancel is currently active. Used by sidecar handlers to
/// suppress the error event + history append when a child died because the
/// user pressed Cancel All (the frontend has already reset file state).
pub fn is_cancelled(app: &AppHandle) -> bool {
    app.try_state::<CancelFlag>()
        .map(|s| s.0.load(Ordering::SeqCst))
        .unwrap_or(false)
}

/// Drain `AppState.active_jobs` and kill every tracked sidecar child.
/// Also raises `CancelFlag` so worker loops (image batch) stop spawning new work.
///
/// Idempotent: safe to call when nothing is running, and safe to call from
/// shutdown paths that may already have drained the map.
pub(crate) fn shutdown_all_jobs(app: &AppHandle) {
    if let Some(flag) = app.try_state::<CancelFlag>() {
        flag.0.store(true, Ordering::SeqCst);
    }
    let Some(state) = app.try_state::<Mutex<AppState>>() else {
        return;
    };
    let jobs: Vec<_> = match state.lock() {
        Ok(mut s) => s.active_jobs.drain().collect(),
        Err(e) => {
            tracing::warn!(error = %e, "Failed to lock AppState during shutdown_all_jobs");
            return;
        }
    };
    for (job_id, (child, _)) in jobs {
        if let Err(e) = child.kill() {
            tracing::warn!(job_id = %job_id, error = %e, "Failed to kill child during shutdown");
        }
    }
}

/// Cancel every active job — kills FFmpeg / Ghostscript children and raises the
/// cancel flag so worker loops (image batch) stop spawning new work.
///
/// Idempotent: safe to call when nothing is running.
#[tauri::command]
pub fn cancel_all(app: AppHandle) -> Result<(), String> {
    shutdown_all_jobs(&app);
    Ok(())
}

/// Clear the cancel flag — call at the start of each fresh queue drain.
#[tauri::command]
pub fn reset_cancel(cancel_flag: State<'_, CancelFlag>) -> Result<(), String> {
    cancel_flag.0.store(false, Ordering::SeqCst);
    Ok(())
}
