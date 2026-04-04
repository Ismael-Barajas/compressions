mod commands;
mod compression;
mod ffmpeg;
mod presets;
mod state;
mod types;

use std::sync::Mutex;
use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(Mutex::new(AppState::default()))
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
