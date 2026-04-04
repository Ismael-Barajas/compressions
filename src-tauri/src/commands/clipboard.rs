use arboard::Clipboard;
use std::path::PathBuf;
use tauri::Manager;

/// Read file paths from the system clipboard (e.g. files copied in Finder/Explorer).
/// Returns an empty vec if clipboard doesn't contain file paths.
#[tauri::command]
pub fn read_clipboard_files() -> Result<Vec<String>, String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    // arboard doesn't have a direct "get files" API on all platforms,
    // but we can try reading text and parsing file paths/URIs.
    let text = clipboard.get_text().unwrap_or_default();
    if text.is_empty() {
        return Ok(vec![]);
    }

    let mut paths = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        // Handle file:// URIs (common on macOS/Linux)
        let path_str = if let Some(stripped) = line.strip_prefix("file://") {
            // URL-decode %20 etc.
            urldecode(stripped)
        } else {
            line.to_string()
        };

        let path = PathBuf::from(&path_str);
        if path.exists() {
            paths.push(path.to_string_lossy().to_string());
        }
    }

    Ok(paths)
}

/// Save clipboard image data to a temporary PNG file.
/// Returns the path to the saved file, or an error if no image is available.
#[tauri::command]
pub fn save_clipboard_image(app: tauri::AppHandle) -> Result<String, String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    let image = clipboard.get_image().map_err(|_| "No image in clipboard".to_string())?;

    // Build temp dir path inside app's temp directory
    let temp_dir = app
        .path()
        .temp_dir()
        .map_err(|e: tauri::Error| e.to_string())?;

    std::fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;

    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let file_path = temp_dir.join(format!("clipboard_{}.png", timestamp));

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

fn urldecode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.bytes();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let hi = chars.next().unwrap_or(b'0');
            let lo = chars.next().unwrap_or(b'0');
            let val = hex_val(hi) * 16 + hex_val(lo);
            result.push(val as char);
        } else {
            result.push(b as char);
        }
    }
    result
}

fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        b'A'..=b'F' => b - b'A' + 10,
        _ => 0,
    }
}
