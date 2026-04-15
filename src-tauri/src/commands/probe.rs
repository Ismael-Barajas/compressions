use std::path::Path;
use std::sync::Arc;

use tauri::{ipc::Channel, AppHandle};

use crate::ffmpeg::probe::probe_video_info;
use crate::types::{FileInfo, MediaType, ProbeEvent};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif", "heic", "heif",
];

#[tauri::command]
pub fn detect_media_type(path: String) -> Result<MediaType, String> {
    let ext = Path::new(&path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Ok(MediaType::Video)
    } else if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        Ok(MediaType::Image)
    } else if ext == "pdf" {
        Ok(MediaType::Pdf)
    } else {
        Err(format!("Unsupported file type: .{}", ext))
    }
}

#[tauri::command]
pub async fn probe_file(app: AppHandle, path: String) -> Result<FileInfo, String> {
    let file_name = Path::new(&path)
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    let media_type = detect_media_type(path.clone())?;

    match media_type {
        MediaType::Video => {
            let info = probe_video_info(&app, &path).await?;
            Ok(FileInfo {
                path,
                file_name,
                size,
                media_type: MediaType::Video,
                duration: info.duration,
                resolution: info.resolution,
                codec_name: info.codec_name,
            })
        }
        MediaType::Pdf => Ok(FileInfo {
            path,
            file_name,
            size,
            media_type: MediaType::Pdf,
            duration: None,
            resolution: None,
            codec_name: None,
        }),
        MediaType::Image => {
            let ext = Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_lowercase())
                .unwrap_or_default();

            // AVIF/HEIC: image crate can't decode these, use FFprobe for dimensions
            let resolution = if ext == "avif" || ext == "heic" || ext == "heif" {
                let info = probe_video_info(&app, &path).await.ok();
                info.and_then(|i| i.resolution)
            } else {
                let path_clone = path.clone();
                let dims =
                    tokio::task::spawn_blocking(move || image::image_dimensions(&path_clone).ok())
                        .await
                        .map_err(|e| e.to_string())?;
                dims.map(|(w, h)| crate::types::Resolution {
                    width: w,
                    height: h,
                })
            };

            Ok(FileInfo {
                path,
                file_name,
                size,
                media_type: MediaType::Image,
                duration: None,
                resolution,
                codec_name: None,
            })
        }
    }
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

            let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let media_type = detect_media_type(path.clone()).ok();

            let (resolution, duration) = match media_type {
                Some(MediaType::Video) => {
                    let info = probe_video_info(&app, &path).await.ok();
                    (
                        info.as_ref().and_then(|i| i.resolution.clone()),
                        info.as_ref().and_then(|i| i.duration),
                    )
                }
                Some(MediaType::Image) => {
                    let ext = Path::new(&path)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_lowercase())
                        .unwrap_or_default();

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
            let result = detect_media_type(format!("file.{}", ext));
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
            let result = detect_media_type(format!("file.{}", ext));
            assert!(
                matches!(result, Ok(MediaType::Image)),
                "failed for .{}",
                ext
            );
        }
    }

    #[test]
    fn detect_pdf() {
        assert!(matches!(
            detect_media_type("doc.pdf".into()),
            Ok(MediaType::Pdf)
        ));
    }

    #[test]
    fn case_insensitive() {
        assert!(matches!(
            detect_media_type("file.MP4".into()),
            Ok(MediaType::Video)
        ));
        assert!(matches!(
            detect_media_type("file.Png".into()),
            Ok(MediaType::Image)
        ));
        assert!(matches!(
            detect_media_type("file.PDF".into()),
            Ok(MediaType::Pdf)
        ));
    }

    #[test]
    fn unknown_extension() {
        assert!(detect_media_type("file.xyz".into()).is_err());
    }

    #[test]
    fn no_extension() {
        assert!(detect_media_type("noext".into()).is_err());
    }
}
