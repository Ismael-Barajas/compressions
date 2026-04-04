use std::sync::Mutex;

use tauri::{AppHandle, State};

use crate::presets::{definitions::builtin_presets, storage};
use crate::state::AppState;
use crate::types::Preset;

#[tauri::command]
pub fn get_presets(
    app: AppHandle,
    _state: State<'_, Mutex<AppState>>,
) -> Result<Vec<Preset>, String> {
    let mut presets = builtin_presets();
    let user_presets = storage::load_user_presets(&app).unwrap_or_default();
    presets.extend(user_presets);
    Ok(presets)
}

#[tauri::command]
pub fn save_preset(app: AppHandle, preset: Preset) -> Result<(), String> {
    storage::save_user_preset(&app, &preset)
}

#[tauri::command]
pub fn delete_preset(app: AppHandle, id: String) -> Result<(), String> {
    let builtins = builtin_presets();
    if builtins.iter().any(|p| p.id == id) {
        return Err("Cannot delete a built-in preset".to_string());
    }
    storage::delete_user_preset(&app, &id)
}

#[tauri::command]
pub fn get_default_output_dir() -> Result<String, String> {
    dirs_next::video_dir()
        .or_else(dirs_next::download_dir)
        .or_else(dirs_next::home_dir)
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not determine default output directory".to_string())
}
