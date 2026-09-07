#[tauri::command]
pub fn get_default_output_dir() -> Result<String, String> {
    dirs_next::video_dir()
        .or_else(dirs_next::download_dir)
        .or_else(dirs_next::home_dir)
        .map(|p| p.to_string_lossy().to_string())
        .ok_or_else(|| "Could not determine default output directory".to_string())
}
