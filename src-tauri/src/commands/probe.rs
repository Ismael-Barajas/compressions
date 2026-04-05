use std::path::Path;

use tauri::AppHandle;

use crate::ffmpeg::probe::probe_video_info;
use crate::types::{FileInfo, MediaType};

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif",
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

            // AVIF: image crate can't decode AVIF, use FFprobe for dimensions
            let resolution = if ext == "avif" {
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
            "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif",
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
