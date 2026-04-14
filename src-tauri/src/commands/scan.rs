use std::collections::HashSet;
use std::fs;
use std::path::Path;

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts",
];

const IMAGE_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "webp", "avif", "bmp", "tiff", "tif", "gif",
];

const PDF_EXTENSIONS: &[&str] = &["pdf"];

const AUDIO_EXTENSIONS: &[&str] = &[
    "mp3", "aac", "m4a", "flac", "wav", "ogg", "opus", "wma", "aiff", "ape", "alac", "ac3", "dts",
    "pcm", "amr",
];

fn is_media_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| {
            let ext = ext.to_lowercase();
            VIDEO_EXTENSIONS.contains(&ext.as_str())
                || IMAGE_EXTENSIONS.contains(&ext.as_str())
                || PDF_EXTENSIONS.contains(&ext.as_str())
                || AUDIO_EXTENSIONS.contains(&ext.as_str())
        })
        .unwrap_or(false)
}

fn walk_dir(dir: &Path, results: &mut Vec<String>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(_) => return, // skip unreadable directories
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk_dir(&path, results);
        } else if path.is_file() && is_media_file(&path) {
            if let Some(s) = path.to_str() {
                results.push(s.to_string());
            }
        }
    }
}

#[tauri::command]
pub fn scan_paths(paths: Vec<String>) -> Result<Vec<String>, String> {
    let mut results = Vec::new();

    for p in &paths {
        let path = Path::new(p);
        if path.is_dir() {
            walk_dir(path, &mut results);
        } else if path.is_file() && is_media_file(path) {
            if let Some(s) = path.to_str() {
                results.push(s.to_string());
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    results.retain(|p| seen.insert(p.clone()));

    Ok(results)
}
