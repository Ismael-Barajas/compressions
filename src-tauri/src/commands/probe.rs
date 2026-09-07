use std::path::Path;
use std::sync::Arc;

use tauri::{ipc::Channel, AppHandle};

use crate::ffmpeg::probe::probe_video_info;
use crate::media::{extension_of, media_type_for_path};
use crate::types::{MediaType, ProbeEvent};

pub fn detect_media_type(path: &str) -> Result<MediaType, String> {
    let path = Path::new(path);
    media_type_for_path(path)
        .ok_or_else(|| format!("Unsupported file type: .{}", extension_of(path)))
}

#[tauri::command]
pub async fn probe_files_batch(
    app: AppHandle,
    paths: Vec<String>,
    on_result: Channel<ProbeEvent>,
) -> Result<(), String> {
    let sem = Arc::new(tokio::sync::Semaphore::new(6));
    let mut set = tokio::task::JoinSet::new();

    for path in paths {
        let app = app.clone();
        let sem = Arc::clone(&sem);
        let on_result = on_result.clone();
        set.spawn(async move {
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return,
            };

            let size = tokio::fs::metadata(&path)
                .await
                .map(|m| m.len())
                .unwrap_or(0);
            let media_type = detect_media_type(&path).ok();

            let (resolution, duration) = match media_type {
                Some(MediaType::Video) | Some(MediaType::Audio) => {
                    let info = probe_video_info(&app, &path).await.ok();
                    (
                        info.as_ref().and_then(|i| i.resolution.clone()),
                        info.as_ref().and_then(|i| i.duration),
                    )
                }
                Some(MediaType::Image) => {
                    let ext = extension_of(Path::new(&path));

                    let res = if ext == "avif" || ext == "heic" || ext == "heif" {
                        probe_video_info(&app, &path)
                            .await
                            .ok()
                            .and_then(|i| i.resolution)
                    } else {
                        let path_clone = path.clone();
                        tokio::task::spawn_blocking(move || {
                            image::image_dimensions(&path_clone).ok()
                        })
                        .await
                        .ok()
                        .flatten()
                        .map(|(w, h)| crate::types::Resolution {
                            width: w,
                            height: h,
                        })
                    };
                    (res, None)
                }
                _ => (None, None),
            };

            let _ = on_result.send(ProbeEvent {
                path,
                size,
                resolution,
                duration,
            });
        });
    }

    while set.join_next().await.is_some() {}

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_video_types() {
        for ext in &[
            "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts",
        ] {
            let result = detect_media_type(&format!("file.{}", ext));
            assert!(
                matches!(result, Ok(MediaType::Video)),
                "failed for .{}",
                ext
            );
        }
    }

    #[test]
    fn detect_image_types() {
        for ext in &[
            "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif", "heic", "heif",
        ] {
            let result = detect_media_type(&format!("file.{}", ext));
            assert!(
                matches!(result, Ok(MediaType::Image)),
                "failed for .{}",
                ext
            );
        }
    }

    #[test]
    fn detect_pdf() {
        assert!(matches!(detect_media_type("doc.pdf"), Ok(MediaType::Pdf)));
    }

    #[test]
    fn case_insensitive() {
        assert!(matches!(
            detect_media_type("file.MP4"),
            Ok(MediaType::Video)
        ));
        assert!(matches!(
            detect_media_type("file.Png"),
            Ok(MediaType::Image)
        ));
        assert!(matches!(detect_media_type("file.PDF"), Ok(MediaType::Pdf)));
    }

    #[test]
    fn detect_audio_types() {
        for ext in &[
            "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma", "aiff", "ape", "alac", "ac3",
            "dts", "pcm", "amr",
        ] {
            let result = detect_media_type(&format!("file.{}", ext));
            assert!(
                matches!(result, Ok(MediaType::Audio)),
                "failed for .{}",
                ext
            );
        }
    }

    #[test]
    fn case_insensitive_audio() {
        assert!(matches!(
            detect_media_type("file.MP3"),
            Ok(MediaType::Audio)
        ));
        assert!(matches!(
            detect_media_type("file.Flac"),
            Ok(MediaType::Audio)
        ));
    }

    #[test]
    fn unknown_extension() {
        assert!(detect_media_type("file.xyz").is_err());
    }

    #[test]
    fn no_extension() {
        assert!(detect_media_type("noext").is_err());
    }
}
