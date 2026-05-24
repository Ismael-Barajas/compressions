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

use state::{AppState, CancelFlag, HwEncoders, ThumbnailSemaphore};
use std::sync::{Arc, Mutex};
use tauri::{Manager, WindowEvent};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .manage(Mutex::new(AppState::default()))
        .manage(HwEncoders::default())
        .manage(CancelFlag::default())
        .manage(ThumbnailSemaphore(Arc::new(tokio::sync::Semaphore::new(4))))
        .setup(|app| {
            // Set window icon explicitly so it shows in dev mode too
            if let Some(window) = app.get_webview_window("main") {
                window
                    .set_icon(tauri::include_image!("icons/icon.png"))
                    .ok();
            }

            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                let encoders = ffmpeg::probe::detect_hw_encoders(&handle).await;
                if let Some(hw) = handle.try_state::<HwEncoders>() {
                    if let Ok(mut w) = hw.0.write() {
                        *w = encoders;
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::video::compress_video,
            commands::video::compress_videos_batch,
            commands::video::cancel_compression,
            commands::queue::cancel_all,
            commands::queue::reset_cancel,
            commands::image::compress_image,
            commands::image::compress_images_batch,
            commands::probe::probe_file,
            commands::probe::detect_media_type,
            commands::probe::probe_files_batch,
            commands::presets::get_presets,
            commands::presets::save_preset,
            commands::presets::delete_preset,
            commands::presets::get_default_output_dir,
            commands::scan::scan_paths,
            commands::audio::extract_audio,
            commands::audio::extract_audio_batch,
            commands::audio::compress_audio,
            commands::audio::compress_audio_batch,
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
            commands::thumbnail::generate_thumbnail,
            commands::thumbnail::generate_thumbnails_batch,
            commands::thumbnail::clear_thumbnail_cache,
        ])
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                let app = window.app_handle().clone();
                let job_count = app
                    .try_state::<Mutex<AppState>>()
                    .and_then(|s| s.lock().ok().map(|g| g.active_jobs.len()))
                    .unwrap_or(0);
                if job_count == 0 {
                    return;
                }
                api.prevent_close();
                let win = window.clone();
                let msg = format!(
                    "{} compression{} in progress. Cancel and exit?",
                    job_count,
                    if job_count == 1 { "" } else { "s" }
                );
                app.clone()
                    .dialog()
                    .message(msg)
                    .title("Compressions running")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::YesNo)
                    .show(move |confirmed| {
                        if confirmed {
                            commands::queue::shutdown_all_jobs(&app);
                            let _ = win.destroy();
                        }
                    });
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            if let tauri::RunEvent::Exit = event {
                commands::queue::shutdown_all_jobs(app);
                commands::thumbnail::cleanup_thumbnail_cache();
            }
        });
}
