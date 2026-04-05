use tauri::AppHandle;

use crate::logging::setup;
use crate::types::LogEntry;

#[tauri::command]
pub fn get_log_path(app: AppHandle) -> Result<String, String> {
    let path = setup::log_file_path(&app)?;
    Ok(path.to_string_lossy().to_string())
}

#[tauri::command]
pub fn read_logs(app: AppHandle, max_lines: Option<usize>) -> Result<Vec<LogEntry>, String> {
    setup::read_log_entries(&app, max_lines)
}

#[tauri::command]
pub fn clear_logs(app: AppHandle) -> Result<(), String> {
    setup::clear_logs(&app)
}
