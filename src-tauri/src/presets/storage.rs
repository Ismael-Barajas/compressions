use std::fs;
use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use crate::types::Preset;

fn presets_path(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {}", e))?;
    Ok(dir.join("presets.json"))
}

pub fn load_user_presets(app: &AppHandle) -> Result<Vec<Preset>, String> {
    let path = presets_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let data = fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read presets file: {}", e))?;
    serde_json::from_str(&data)
        .map_err(|e| format!("Failed to parse presets file: {}", e))
}

pub fn save_user_preset(app: &AppHandle, preset: &Preset) -> Result<(), String> {
    let mut presets = load_user_presets(app).unwrap_or_default();

    // Update or add
    if let Some(existing) = presets.iter_mut().find(|p| p.id == preset.id) {
        *existing = preset.clone();
    } else {
        presets.push(preset.clone());
    }

    write_presets(app, &presets)
}

pub fn delete_user_preset(app: &AppHandle, id: &str) -> Result<(), String> {
    let mut presets = load_user_presets(app).unwrap_or_default();
    presets.retain(|p| p.id != id);
    write_presets(app, &presets)
}

fn write_presets(app: &AppHandle, presets: &[Preset]) -> Result<(), String> {
    let path = presets_path(app)?;
    let data = serde_json::to_string_pretty(presets)
        .map_err(|e| format!("Failed to serialize presets: {}", e))?;
    fs::write(path, data).map_err(|e| format!("Failed to write presets file: {}", e))
}
