//! Single source of truth for which file extensions the app accepts, and how they
//! map to a media type. The frontend fetches this over IPC (`get_supported_media`)
//! so dialog filters and drop handling can never drift from the backend.

use std::path::Path;

use serde::Serialize;

use crate::types::MediaType;

pub const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts",
];

pub const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif", "heic", "heif",
];

pub const PDF_EXTENSIONS: &[&str] = &["pdf"];

pub const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma", "aiff", "ape", "alac", "ac3", "dts",
    "pcm", "amr",
];

/// Lower-cased extension of `path` without the dot, or empty.
pub fn extension_of(path: &Path) -> String {
    path.extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

pub fn media_type_for_extension(ext: &str) -> Option<MediaType> {
    if VIDEO_EXTENSIONS.contains(&ext) {
        Some(MediaType::Video)
    } else if IMAGE_EXTENSIONS.contains(&ext) {
        Some(MediaType::Image)
    } else if PDF_EXTENSIONS.contains(&ext) {
        Some(MediaType::Pdf)
    } else if AUDIO_EXTENSIONS.contains(&ext) {
        Some(MediaType::Audio)
    } else {
        None
    }
}

pub fn media_type_for_path(path: &Path) -> Option<MediaType> {
    media_type_for_extension(&extension_of(path))
}

pub fn is_supported_media_path(path: &Path) -> bool {
    media_type_for_path(path).is_some()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedMedia {
    pub video: Vec<&'static str>,
    pub image: Vec<&'static str>,
    pub audio: Vec<&'static str>,
    pub pdf: Vec<&'static str>,
}

#[tauri::command]
pub fn get_supported_media() -> SupportedMedia {
    SupportedMedia {
        video: VIDEO_EXTENSIONS.to_vec(),
        image: IMAGE_EXTENSIONS.to_vec(),
        audio: AUDIO_EXTENSIONS.to_vec(),
        pdf: PDF_EXTENSIONS.to_vec(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_case_insensitively() {
        assert!(matches!(
            media_type_for_path(Path::new("a.MP4")),
            Some(MediaType::Video)
        ));
        assert!(matches!(
            media_type_for_path(Path::new("a.Png")),
            Some(MediaType::Image)
        ));
        assert!(matches!(
            media_type_for_path(Path::new("a.PDF")),
            Some(MediaType::Pdf)
        ));
        assert!(matches!(
            media_type_for_path(Path::new("a.FLAC")),
            Some(MediaType::Audio)
        ));
        assert!(media_type_for_path(Path::new("a.xyz")).is_none());
        assert!(media_type_for_path(Path::new("noext")).is_none());
    }

    #[test]
    fn lists_are_disjoint() {
        let all: Vec<&str> = VIDEO_EXTENSIONS
            .iter()
            .chain(IMAGE_EXTENSIONS)
            .chain(PDF_EXTENSIONS)
            .chain(AUDIO_EXTENSIONS)
            .copied()
            .collect();
        let unique: std::collections::HashSet<&str> = all.iter().copied().collect();
        assert_eq!(all.len(), unique.len());
    }
}
