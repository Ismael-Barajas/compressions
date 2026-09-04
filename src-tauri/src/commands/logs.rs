use tauri::AppHandle;

use crate::logging::setup;
use crate::types::LogEntry;

#[tauri::command]
pub fn get_log_path(app: AppHandle) -> Result<String, String> {
    let path = setup::log_file_path(&app)?;
    Ok(path.to_string_lossy().to_string())
}

/// Reads log files on a blocking thread so the main thread stays responsive.
#[tauri::command]
pub async fn read_logs(app: AppHandle, max_lines: Option<usize>) -> Result<Vec<LogEntry>, String> {
    tokio::task::spawn_blocking(move || setup::read_log_entries(&app, max_lines))
        .await
        .map_err(|e| format!("Log read task failed: {}", e))?
}

#[tauri::command]
pub async fn clear_logs(app: AppHandle) -> Result<(), String> {
    tokio::task::spawn_blocking(move || setup::clear_logs(&app))
        .await
        .map_err(|e| format!("Log clear task failed: {}", e))?
}
