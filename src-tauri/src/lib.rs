mod commands;
pub mod compression;
mod ffmpeg;
mod history;
mod logging;
mod presets;
mod state;
pub mod types;
mod utils;
mod validate;

use state::AppState;
use std::sync::Mutex;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize tracing early — resolve log dir via platform dirs
    let log_dir = dirs_next::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("com.compressions.app")
        .join("logs");
    std::fs::create_dir_all(&log_dir).ok();
    let _guard = logging::setup::init_tracing(log_dir);

    tracing::info!("Compressions app starting");

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(Mutex::new(AppState::default()))
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let encoders = ffmpeg::probe::detect_hw_encoders(&handle).await;
                if let Some(state) = handle.try_state::<Mutex<AppState>>() {
                    if let Ok(mut s) = state.lock() {
                        s.hw_encoders = encoders;
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::video::compress_video,
            commands::video::compress_videos_batch,
            commands::video::cancel_compression,
            commands::image::compress_image,
            commands::image::compress_images_batch,
            commands::probe::probe_file,
            commands::probe::detect_media_type,
            commands::presets::get_presets,
            commands::presets::save_preset,
            commands::presets::delete_preset,
            commands::presets::get_default_output_dir,
            commands::scan::scan_paths,
            commands::audio::extract_audio,
            commands::audio::extract_audio_batch,
            commands::gif::convert_video_to_gif,
            commands::gif::convert_videos_to_gif_batch,
            commands::pdf::compress_pdf,
            commands::pdf::compress_pdfs_batch,
            commands::clipboard::read_clipboard_files,
            commands::clipboard::save_clipboard_image,
            commands::history::get_history,
            commands::history::clear_history,
            commands::logs::get_log_path,
            commands::logs::read_logs,
            commands::logs::clear_logs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
