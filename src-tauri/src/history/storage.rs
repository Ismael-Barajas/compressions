use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::types::HistoryEntry;

const MAX_ENTRIES: usize = 1000;
static HISTORY_WRITE_LOCK: Mutex<()> = Mutex::new(());

fn history_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(dir.join("history.json"))
}

pub fn load_history(app: &AppHandle) -> Result<Vec<HistoryEntry>, String> {
    let path = history_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data =
        fs::read_to_string(&path).map_err(|e| format!("Failed to read history file: {}", e))?;
    serde_json::from_str(&data).map_err(|e| format!("Failed to parse history file: {}", e))
}

pub fn append_entry(app: &AppHandle, entry: HistoryEntry) -> Result<(), String> {
    let _guard = HISTORY_WRITE_LOCK
        .lock()
        .map_err(|e| format!("Failed to lock history file: {}", e))?;
    let mut entries = load_history(app).unwrap_or_default();
    entries.push(entry);

    // Cap at MAX_ENTRIES, keeping the most recent
    if entries.len() > MAX_ENTRIES {
        let drain_count = entries.len() - MAX_ENTRIES;
        entries.drain(..drain_count);
    }

    write_history(app, &entries)
}

pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    let _guard = HISTORY_WRITE_LOCK
        .lock()
        .map_err(|e| format!("Failed to lock history file: {}", e))?;
    write_history(app, &[])
}

fn write_history(app: &AppHandle, entries: &[HistoryEntry]) -> Result<(), String> {
    let path = history_path(app)?;
    let data = serialize_history(entries)?;
    fs::write(path, data).map_err(|e| format!("Failed to write history file: {}", e))
}

fn serialize_history(entries: &[HistoryEntry]) -> Result<String, String> {
    serde_json::to_string(entries).map_err(|e| format!("Failed to serialize history: {}", e))
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
}
