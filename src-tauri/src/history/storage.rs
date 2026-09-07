//! Compression history, kept in memory and flushed to disk in the background.
//!
//! Every completed job used to read, parse, re-serialize and rewrite the whole
//! `history.json` (up to 1000 entries) under a blocking lock, on the async runtime.
//! Now `append_entry` is an in-memory push; a single flusher task coalesces writes
//! so a 500-file batch costs a handful of disk writes instead of 500.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager};
use tokio::sync::Notify;

use crate::types::HistoryEntry;

const MAX_ENTRIES: usize = 1000;
/// Wait this long after the first append before writing, so bursts coalesce.
const FLUSH_DEBOUNCE: Duration = Duration::from_millis(750);

pub struct HistoryState {
    entries: Mutex<Vec<HistoryEntry>>,
    path: PathBuf,
    dirty: AtomicBool,
    notify: Notify,
}

impl HistoryState {
    fn new(path: PathBuf, entries: Vec<HistoryEntry>) -> Self {
        Self {
            entries: Mutex::new(entries),
            path,
            dirty: AtomicBool::new(false),
            notify: Notify::new(),
        }
    }

    pub fn snapshot(&self) -> Vec<HistoryEntry> {
        self.entries.lock().map(|e| e.clone()).unwrap_or_default()
    }

    pub fn push(&self, entry: HistoryEntry) -> Result<(), String> {
        let mut entries = self.entries.lock().map_err(|e| e.to_string())?;
        entries.push(entry);
        cap_entries(&mut entries);
        drop(entries);
        self.mark_dirty();
        Ok(())
    }

    pub fn clear(&self) -> Result<(), String> {
        self.entries.lock().map_err(|e| e.to_string())?.clear();
        self.mark_dirty();
        Ok(())
    }

    fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::SeqCst);
        self.notify.notify_one();
    }

    /// Write the current entries to disk if anything changed. Safe to call from any
    /// thread; does the file I/O synchronously (callers on the runtime use
    /// `spawn_blocking`).
    pub fn flush_if_dirty(&self) -> Result<(), String> {
        if !self.dirty.swap(false, Ordering::SeqCst) {
            return Ok(());
        }
        let snapshot = self.snapshot();
        if let Err(e) = write_history_file(&self.path, &snapshot) {
            // Try again on the next change.
            self.dirty.store(true, Ordering::SeqCst);
            return Err(e);
        }
        Ok(())
    }

    /// Background loop: sleep until notified, debounce, flush. Runs for the app's life.
    async fn run_flusher(app: AppHandle) {
        loop {
            let Some(state) = app.try_state::<HistoryState>() else {
                return;
            };
            state.notify.notified().await;
            tokio::time::sleep(FLUSH_DEBOUNCE).await;
            let app2 = app.clone();
            let res = tokio::task::spawn_blocking(move || {
                app2.try_state::<HistoryState>()
                    .map(|s| s.flush_if_dirty())
                    .unwrap_or(Ok(()))
            })
            .await;
            match res {
                Ok(Ok(())) => {}
                Ok(Err(e)) => tracing::warn!(error = %e, "Failed to flush history"),
                Err(e) => tracing::warn!(error = %e, "History flusher join error"),
            }
        }
    }
}

fn cap_entries(entries: &mut Vec<HistoryEntry>) {
    if entries.len() > MAX_ENTRIES {
        let drain_count = entries.len() - MAX_ENTRIES;
        entries.drain(..drain_count);
    }
}

fn history_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(dir.join("history.json"))
}

fn read_history_file(path: &PathBuf) -> Result<Vec<HistoryEntry>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(path).map_err(|e| format!("Failed to read history file: {}", e))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse history file: {}", e))
}

/// Write via a temp file + rename so a crash mid-write never truncates the history.
fn write_history_file(path: &PathBuf, entries: &[HistoryEntry]) -> Result<(), String> {
    let data = serialize_history(entries)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, data).map_err(|e| format!("Failed to write history file: {}", e))?;
    fs::rename(&tmp, path).map_err(|e| format!("Failed to replace history file: {}", e))
}

fn serialize_history(entries: &[HistoryEntry]) -> Result<String, String> {
    serde_json::to_string(entries).map_err(|e| format!("Failed to serialize history: {}", e))
}

/// Load history from disk into managed state and start the background flusher.
/// Call once from `setup`.
pub fn init(app: &AppHandle) -> Result<(), String> {
    let path = history_path(app)?;
    let entries = match read_history_file(&path) {
        Ok(mut e) => {
            cap_entries(&mut e);
            e
        }
        Err(err) => {
            tracing::warn!(error = %err, "Could not load history; starting empty");
            Vec::new()
        }
    };
    app.manage(HistoryState::new(path, entries));
    tauri::async_runtime::spawn(HistoryState::run_flusher(app.clone()));
    Ok(())
}

pub fn load_history(app: &AppHandle) -> Result<Vec<HistoryEntry>, String> {
    Ok(app
        .try_state::<HistoryState>()
        .map(|s| s.snapshot())
        .unwrap_or_default())
}

pub fn append_entry(app: &AppHandle, entry: HistoryEntry) -> Result<(), String> {
    app.try_state::<HistoryState>()
        .ok_or_else(|| "History not initialized".to_string())?
        .push(entry)
}

pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    app.try_state::<HistoryState>()
        .ok_or_else(|| "History not initialized".to_string())?
        .clear()
}

/// Synchronous flush for shutdown paths.
pub fn flush_now(app: &AppHandle) {
    if let Some(state) = app.try_state::<HistoryState>() {
        if let Err(e) = state.flush_if_dirty() {
            tracing::warn!(error = %e, "Failed to flush history on exit");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(id: &str) -> HistoryEntry {
        HistoryEntry {
            id: id.to_string(),
            timestamp: "2026-07-02T00:00:00Z".to_string(),
            input_path: "in.png".to_string(),
            output_path: "out.png".to_string(),
            input_size: 100,
            output_size: 50,
            duration_ms: 10,
            media_type: "image".to_string(),
            success: true,
            error: None,
        }
    }

    #[test]
    fn serialize_history_uses_compact_json() {
        let data = serialize_history(&[entry("1")]).unwrap();

        assert!(!data.contains('\n'));
        assert!(data.starts_with("[{\"id\":\"1\""));
    }

    #[test]
    fn push_caps_and_marks_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let state = HistoryState::new(dir.path().join("history.json"), Vec::new());
        for i in 0..(MAX_ENTRIES + 5) {
            state.push(entry(&i.to_string())).unwrap();
        }
        let snap = state.snapshot();
        assert_eq!(snap.len(), MAX_ENTRIES);
        assert_eq!(snap.first().unwrap().id, "5");
        assert!(state.dirty.load(Ordering::SeqCst));
    }

    #[test]
    fn flush_writes_once_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let state = HistoryState::new(path.clone(), Vec::new());
        state.push(entry("a")).unwrap();
        state.push(entry("b")).unwrap();

        state.flush_if_dirty().unwrap();
        assert!(!state.dirty.load(Ordering::SeqCst));
        let on_disk = read_history_file(&path).unwrap();
        assert_eq!(on_disk.len(), 2);
        assert!(!path.with_extension("json.tmp").exists());

        // Nothing changed: flushing again is a no-op (file untouched).
        let mtime = fs::metadata(&path).unwrap().modified().unwrap();
        state.flush_if_dirty().unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), mtime);
    }

    #[test]
    fn clear_empties_and_persists() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("history.json");
        let state = HistoryState::new(path.clone(), vec![entry("x")]);
        state.clear().unwrap();
        state.flush_if_dirty().unwrap();
        assert!(read_history_file(&path).unwrap().is_empty());
    }
}
