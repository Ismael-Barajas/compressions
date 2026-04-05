use std::path::{Path, PathBuf};

/// If `output` already exists, append `_2`, `_3`, etc. before the extension
/// until a free path is found. Returns the original path if it doesn't exist.
pub fn resolve_output_conflict(output: &str) -> String {
    let path = Path::new(output);
    if !path.exists() {
        return output.to_string();
    }

    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let parent = path.parent().unwrap_or(Path::new("."));

    for i in 2..=999 {
        let candidate: PathBuf = if ext.is_empty() {
            parent.join(format!("{}_{}", stem, i))
        } else {
            parent.join(format!("{}_{}.{}", stem, i, ext))
        };
        if !candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    // Fallback: just return the original (will overwrite)
    output.to_string()
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
}
