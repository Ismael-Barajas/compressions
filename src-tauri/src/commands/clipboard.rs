use arboard::Clipboard;
use std::path::{Path, PathBuf};
use tauri::Manager;

/// Read file paths from the system clipboard (e.g. files copied in Finder/Explorer).
/// Returns an empty vec if clipboard doesn't contain file paths.
#[tauri::command]
pub async fn read_clipboard_files() -> Result<Vec<String>, String> {
    tokio::task::spawn_blocking(read_clipboard_files_sync)
        .await
        .map_err(|e| format!("Clipboard task failed: {}", e))?
}

fn read_clipboard_files_sync() -> Result<Vec<String>, String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    // Native file list first (CF_HDROP on Windows, NSFilenamesPboardType on macOS,
    // text/uri-list on Linux): this is what "Copy" in a file manager produces.
    if let Ok(files) = clipboard.get().file_list() {
        let paths: Vec<String> = files
            .into_iter()
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string())
            .collect();
        if !paths.is_empty() {
            return Ok(paths);
        }
    }

    // Fall back to text: some apps put plain paths or file:// URIs on the clipboard.
    let text = clipboard.get_text().unwrap_or_default();
    Ok(paths_from_clipboard_text(&text)
        .into_iter()
        .filter(|p| Path::new(p).exists())
        .collect())
}

/// Parse one path per line, accepting plain paths and `file://` URIs.
fn paths_from_clipboard_text(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|line| match line.strip_prefix("file://") {
            Some(uri) => file_uri_to_path(uri),
            None => line.to_string(),
        })
        .collect()
}

/// Turn the part after `file://` into a local path. `file:///C:/x.png` must become
/// `C:/x.png` on Windows (the leading slash is the URI's, not the path's), and a
/// `localhost` authority is dropped.
fn file_uri_to_path(after_scheme: &str) -> String {
    let rest = after_scheme
        .strip_prefix("localhost/")
        .map(|r| format!("/{}", r))
        .unwrap_or_else(|| after_scheme.to_string());
    let decoded = urldecode(&rest);
    let bytes = decoded.as_bytes();
    let is_drive_path =
        bytes.len() >= 3 && bytes[0] == b'/' && bytes[1].is_ascii_alphabetic() && bytes[2] == b':';
    if is_drive_path {
        decoded[1..].to_string()
    } else {
        decoded
    }
}

const CLIPBOARD_IMAGE_PREFIX: &str = "clipboard_";

/// Save clipboard image data to a temporary PNG file.
/// Returns the path to the saved file, or an error if no image is available.
#[tauri::command]
pub async fn save_clipboard_image(app: tauri::AppHandle) -> Result<String, String> {
    tokio::task::spawn_blocking(move || save_clipboard_image_sync(&app))
        .await
        .map_err(|e| format!("Clipboard task failed: {}", e))?
}

fn clipboard_temp_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .temp_dir()
        .map_err(|e: tauri::Error| e.to_string())
}

fn save_clipboard_image_sync(app: &tauri::AppHandle) -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    let image = clipboard
        .get_image()
        .map_err(|_| "No image in clipboard".to_string())?;

    let temp_dir = clipboard_temp_dir(app)?;
    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let file_path = temp_dir.join(format!(
        "{}{}.png",
        CLIPBOARD_IMAGE_PREFIX,
        uuid::Uuid::new_v4()
    ));

    // Convert RGBA bytes to PNG
    let width = image.width as u32;
    let height = image.height as u32;
    let rgba_data: Vec<u8> = image.bytes.into_owned();

    let img_buf = image::RgbaImage::from_raw(width, height, rgba_data)
        .ok_or_else(|| "Failed to create image from clipboard data".to_string())?;

    img_buf
        .save(&file_path)
        .map_err(|e| format!("Failed to save clipboard image: {}", e))?;

    Ok(file_path.to_string_lossy().to_string())
}

/// Delete every PNG this app saved from the clipboard. Called at startup (leftovers
/// from a crash) and on exit; while the app runs the files may still be queued.
pub fn cleanup_clipboard_images(app: &tauri::AppHandle) {
    let Ok(dir) = clipboard_temp_dir(app) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let is_ours = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.starts_with(CLIPBOARD_IMAGE_PREFIX) && n.ends_with(".png"))
            .unwrap_or(false);
        if is_ours {
            let _ = std::fs::remove_file(&path);
        }
    }
}

fn urldecode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut raw = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = bytes[i + 1];
            let lo = bytes[i + 2];
            if hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit() {
                raw.push(hex_val(hi) * 16 + hex_val(lo));
                i += 3;
                continue;
            }
        }
        raw.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&raw).into_owned()
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_uri_windows_drive_letter() {
        assert_eq!(
            file_uri_to_path("/C:/Users/me/a%20b.png"),
            "C:/Users/me/a b.png"
        );
        assert_eq!(file_uri_to_path("localhost/C:/x.png"), "C:/x.png");
    }

    #[test]
    fn file_uri_posix() {
        assert_eq!(file_uri_to_path("/home/me/photo.jpg"), "/home/me/photo.jpg");
        assert_eq!(file_uri_to_path("localhost/tmp/x.png"), "/tmp/x.png");
    }

    #[test]
    fn clipboard_text_mixes_uris_and_plain_paths() {
        let text = "file:///C:/a.png\r\n\r\n  D:\\b.mp4  \nfile:///tmp/c%20d.pdf";
        assert_eq!(
            paths_from_clipboard_text(text),
            vec!["C:/a.png", "D:\\b.mp4", "/tmp/c d.pdf"]
        );
    }

    #[test]
    fn urldecode_percent20() {
        assert_eq!(urldecode("/path/to/my%20file.txt"), "/path/to/my file.txt");
    }

    #[test]
    fn urldecode_passthrough() {
        assert_eq!(urldecode("/normal/path.txt"), "/normal/path.txt");
    }

    #[test]
    fn urldecode_empty() {
        assert_eq!(urldecode(""), "");
    }

    #[test]
    fn urldecode_truncated_percent() {
        // Truncated %2 at end — should leave literal %2
        assert_eq!(urldecode("/path%2"), "/path%2");
        // Lone % at end
        assert_eq!(urldecode("/path%"), "/path%");
    }

    #[test]
    fn urldecode_invalid_hex_passthrough() {
        // Non-hex chars after % — should leave literal
        assert_eq!(urldecode("/path%ZZ"), "/path%ZZ");
    }

    #[test]
    fn urldecode_multiple_encoded() {
        assert_eq!(urldecode("%2F%2F"), "//");
    }

    #[test]
    fn hex_val_digits() {
        for (b, expected) in [(b'0', 0), (b'5', 5), (b'9', 9)] {
            assert_eq!(hex_val(b), expected);
        }
    }

    #[test]
    fn hex_val_lowercase() {
        for (b, expected) in [(b'a', 10), (b'c', 12), (b'f', 15)] {
            assert_eq!(hex_val(b), expected);
        }
    }

    #[test]
    fn hex_val_uppercase() {
        for (b, expected) in [(b'A', 10), (b'C', 12), (b'F', 15)] {
            assert_eq!(hex_val(b), expected);
        }
    }

    #[test]
    fn hex_val_invalid() {
        assert_eq!(hex_val(b'g'), 0);
        assert_eq!(hex_val(b'Z'), 0);
    }

    #[test]
    fn urldecode_multibyte_utf8() {
        // "é" is U+00E9, encoded as %C3%A9 in UTF-8
        assert_eq!(urldecode("/path/caf%C3%A9"), "/path/café");
        // "日本語" encoded as percent-encoded UTF-8
        assert_eq!(urldecode("%E6%97%A5%E6%9C%AC%E8%AA%9E"), "日本語");
    }
}
