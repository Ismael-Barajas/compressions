use tauri::AppHandle;

use crate::history::storage;
use crate::types::HistoryEntry;

#[tauri::command]
pub fn get_history(app: AppHandle) -> Result<Vec<HistoryEntry>, String> {
    storage::load_history(&app)
}

#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    storage::clear_history(&app)
}
