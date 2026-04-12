use std::fs::OpenOptions;
use std::path::{Path, PathBuf};

/// If `output` already exists, append `_2`, `_3`, etc. before the extension
/// until a free path is found. Atomically claims the path by creating a
/// zero-byte marker file, preventing TOCTOU races during parallel compression.
pub fn resolve_output_conflict(output: &str) -> String {
    let path = Path::new(output);

    // Try to atomically claim the original path first
    if try_claim_path(path) {
        return output.to_string();
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 2..=999 {
        let candidate: PathBuf = if ext.is_empty() {
            parent.join(format!("{}_{}", stem, i))
        } else {
            parent.join(format!("{}_{}.{}", stem, i, ext))
        };
        if try_claim_path(&candidate) {
            return candidate.to_string_lossy().to_string();
        }
    }

    // Fallback: just return the original (will overwrite)
    output.to_string()
}

/// Attempt to atomically create a zero-byte file at `path`.
/// Returns true if the file was created (path claimed), false if it already exists.
fn try_claim_path(path: &Path) -> bool {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_conflict_returns_original() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jpg");
        let result = resolve_output_conflict(path.to_str().unwrap());
        assert_eq!(result, path.to_str().unwrap());
    }

    #[test]
    fn conflict_appends_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jpg");
        std::fs::write(&path, b"data").unwrap();

        let result = resolve_output_conflict(path.to_str().unwrap());
        assert!(result.ends_with("test_2.jpg"));
    }

    #[test]
    fn multiple_conflicts_increment() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("test.jpg");
        std::fs::write(&path, b"data").unwrap();
        std::fs::write(dir.path().join("test_2.jpg"), b"data").unwrap();

        let result = resolve_output_conflict(path.to_str().unwrap());
        assert!(result.ends_with("test_3.jpg"));
    }

    #[test]
    fn parallel_calls_get_distinct_paths() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        let path_str = path.to_str().unwrap();

        // Simulate two parallel calls for the same output path
        let result1 = resolve_output_conflict(path_str);
        let result2 = resolve_output_conflict(path_str);

        // Both should succeed with different paths (atomic claim prevents collision)
        assert_ne!(result1, result2);
        assert_eq!(result1, path_str);
        assert!(result2.ends_with("photo_2.jpg"));
    }
}
