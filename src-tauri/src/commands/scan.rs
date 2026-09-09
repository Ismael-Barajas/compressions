use std::collections::HashSet;
use std::path::Path;

use walkdir::WalkDir;

use crate::media::is_supported_media_path;

/// Bounds for a dropped folder: deep enough for any real media library, small
/// enough that a dropped drive root cannot enumerate the whole disk onto the IPC
/// boundary.
const MAX_SCAN_DEPTH: usize = 32;
pub const MAX_SCAN_FILES: usize = 10_000;

fn is_hidden(entry: &walkdir::DirEntry) -> bool {
    entry.depth() > 0
        && entry
            .file_name()
            .to_str()
            .map(|n| n.starts_with('.'))
            .unwrap_or(false)
}

fn scan_paths_sync(paths: &[String]) -> Vec<String> {
    let mut results = Vec::new();

    'outer: for p in paths {
        let path = Path::new(p);
        if path.is_dir() {
            // No symlink following (guards against cycles), skip hidden entries such
            // as `.git`/`.Trash`, and keep going past unreadable subdirectories.
            let walker = WalkDir::new(path)
                .follow_links(false)
                .max_depth(MAX_SCAN_DEPTH)
                .into_iter()
                .filter_entry(|e| !is_hidden(e));
            for entry in walker.filter_map(|e| e.ok()) {
                if results.len() >= MAX_SCAN_FILES {
                    tracing::warn!(
                        root = %p,
                        cap = MAX_SCAN_FILES,
                        "Folder scan hit the file cap; remaining files were skipped"
                    );
                    break 'outer;
                }
                if entry.file_type().is_file() && is_supported_media_path(entry.path()) {
                    if let Some(s) = entry.path().to_str() {
                        results.push(s.to_string());
                    }
                }
            }
        } else if path.is_file() && is_supported_media_path(path) {
            if let Some(s) = path.to_str() {
                results.push(s.to_string());
            }
        }
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    results.retain(|p| seen.insert(p.clone()));
    results
}

/// Expand dropped/selected paths into a flat list of supported media files.
/// Runs on a blocking thread so a large folder never freezes the webview.
#[tauri::command]
pub async fn scan_paths(paths: Vec<String>) -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(move || scan_paths_sync(&paths))
        .await
        .map_err(|e| format!("Scan task failed: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_recursively_and_skips_hidden_and_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::create_dir_all(root.join("sub/.hidden")).unwrap();
        std::fs::write(root.join("a.mp4"), b"x").unwrap();
        std::fs::write(root.join("notes.txt"), b"x").unwrap();
        std::fs::write(root.join("sub/b.PNG"), b"x").unwrap();
        std::fs::write(root.join("sub/.hidden/c.jpg"), b"x").unwrap();
        std::fs::write(root.join("sub/.dotfile.jpg"), b"x").unwrap();

        let mut found = scan_paths_sync(&[root.to_string_lossy().to_string()]);
        found.sort();
        let names: Vec<String> = found
            .iter()
            .map(|p| {
                Path::new(p)
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert_eq!(names, vec!["a.mp4", "b.PNG"]);
    }

    #[test]
    fn dedupes_and_accepts_direct_files() {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("song.mp3");
        std::fs::write(&f, b"x").unwrap();
        let s = f.to_string_lossy().to_string();
        let found = scan_paths_sync(&[s.clone(), s.clone()]);
        assert_eq!(found, vec![s]);
    }

    #[test]
    fn stops_below_max_depth() {
        let dir = tempfile::tempdir().unwrap();
        let mut deep = dir.path().to_path_buf();
        for i in 0..(MAX_SCAN_DEPTH + 2) {
            deep = deep.join(format!("d{}", i));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("far.jpg"), b"x").unwrap();
        std::fs::write(dir.path().join("near.jpg"), b"x").unwrap();

        let found = scan_paths_sync(&[dir.path().to_string_lossy().to_string()]);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("near.jpg"));
    }
}
