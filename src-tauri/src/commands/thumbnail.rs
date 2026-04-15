use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::time::timeout;

use crate::state::ThumbnailSemaphore;

const THUMB_SIZE: u32 = 160;
const FFMPEG_THUMB_TIMEOUT: Duration = Duration::from_secs(10);

const VIDEO_EXTENSIONS: &[&str] = &[
    "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "m4v", "ts",
];

const IMAGE_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "tiff", "tif", "gif"];

fn get_extension(path: &str) -> String {
    Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

/// Stable hash of a file path to use as the thumbnail filename.
fn path_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// Get (and create if needed) the thumbnail cache directory.
/// Uses the system temp directory so the OS can clean up on its own too.
pub fn cache_dir() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join("compressions-thumbnails");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create thumbnail cache dir: {e}"))?;
    Ok(dir)
}

/// Generate a thumbnail for an image using the `image` crate, write to disk.
async fn thumbnail_image(path: String, out_path: PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let img = image::open(&path).map_err(|e| format!("Failed to open image: {e}"))?;
        let thumb = img.thumbnail(THUMB_SIZE, THUMB_SIZE);
        let mut buf = Vec::new();
        let mut cursor = Cursor::new(&mut buf);
        thumb
            .write_to(&mut cursor, image::ImageFormat::Jpeg)
            .map_err(|e| format!("Failed to encode thumbnail: {e}"))?;
        std::fs::write(&out_path, &buf).map_err(|e| format!("Failed to write thumbnail: {e}"))?;
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())??;
    Ok(())
}

/// Decode an image to a temp PNG via FFmpeg, then thumbnail with the `image` crate.
/// Used for formats the `image` crate can't decode directly (HEIC/HEIF) where FFmpeg's
/// tile grid assembly uses an internal complex filtergraph that conflicts with `-vf`.
async fn thumbnail_via_decode(
    app: &AppHandle,
    path: &str,
    out_path: PathBuf,
) -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    let temp_name = format!("compressions_thumb_{}.png", path_hash(path));
    let temp_path = temp_dir.join(&temp_name);
    let temp_str = temp_path
        .to_str()
        .ok_or_else(|| "Invalid temp path".to_string())?
        .to_string();

    // Decode to full-size PNG (no filters — avoids complex filtergraph conflict)
    let args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        path.into(),
        "-frames:v".into(),
        "1".into(),
        "-pix_fmt".into(),
        "rgba".into(),
        temp_str.clone(),
    ];

    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {e}"))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {e}"))?;

    let mut stderr_lines = Vec::new();
    let recv_result = timeout(FFMPEG_THUMB_TIMEOUT, async {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(line) => {
                    stderr_lines.push(String::from_utf8_lossy(&line).to_string());
                }
                CommandEvent::Terminated(_) => break,
                _ => {}
            }
        }
    })
    .await;

    if recv_result.is_err() {
        let _ = child.kill();
        let _ = std::fs::remove_file(&temp_path);
        return Err("FFmpeg decode timed out".to_string());
    }

    if !temp_path.exists() {
        let stderr = stderr_lines.join("\n");
        tracing::warn!(path = %path, stderr = %stderr, "FFmpeg decode for thumbnail failed");
        return Err("FFmpeg decode did not produce output".to_string());
    }

    // Now thumbnail the decoded PNG with the image crate
    let result = thumbnail_image(temp_str.clone(), out_path).await;

    // Clean up temp file
    let _ = std::fs::remove_file(&temp_path);

    result
}

/// Generate a thumbnail using FFmpeg (for videos and AVIF images), write to disk.
/// `is_video` controls whether we seek ahead (videos) or not (still images like AVIF).
async fn thumbnail_ffmpeg(
    app: &AppHandle,
    path: &str,
    out_path: &Path,
    is_video: bool,
) -> Result<(), String> {
    let out_str = out_path
        .to_str()
        .ok_or_else(|| "Invalid output path".to_string())?;

    let mut args = Vec::new();
    args.push("-y".to_string());
    if is_video {
        args.push("-ss".to_string());
        args.push("1".to_string());
    }
    args.push("-i".to_string());
    args.push(path.to_string());
    args.push("-frames:v".to_string());
    args.push("1".to_string());
    args.push("-vf".to_string());
    args.push(format!("scale={THUMB_SIZE}:-2"));
    args.push("-pix_fmt".to_string());
    args.push("yuvj420p".to_string());
    args.push("-q:v".to_string());
    args.push("5".to_string());
    args.push(out_str.to_string());

    let (mut rx, child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {e}"))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {e}"))?;

    let mut stderr_lines = Vec::new();
    let recv_result = timeout(FFMPEG_THUMB_TIMEOUT, async {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stderr(line) => {
                    stderr_lines.push(String::from_utf8_lossy(&line).to_string());
                }
                CommandEvent::Terminated(_) => break,
                _ => {}
            }
        }
    })
    .await;

    if recv_result.is_err() {
        let _ = child.kill();
        tracing::warn!(path = %path, "FFmpeg thumbnail timed out");
        return Err("FFmpeg thumbnail timed out".to_string());
    }

    // child dropped here on success path — process already terminated

    if !out_path.exists() {
        let stderr = stderr_lines.join("\n");
        tracing::warn!(path = %path, stderr = %stderr, "FFmpeg thumbnail did not produce output");
        return Err("FFmpeg did not produce output".to_string());
    }

    Ok(())
}

/// Generate a single thumbnail for any supported media file.
/// Returns the path to the thumbnail on disk, or None for unsupported types.
async fn generate_one(app: &AppHandle, path: &str) -> Result<Option<String>, String> {
    let ext = get_extension(path);

    // PDF or unsupported — no thumbnail
    if !IMAGE_EXTENSIONS.contains(&ext.as_str())
        && ext != "avif"
        && ext != "heic"
        && ext != "heif"
        && !VIDEO_EXTENSIONS.contains(&ext.as_str())
    {
        return Ok(None);
    }

    let hash = path_hash(path);
    let dir = cache_dir()?;
    let out_path = dir.join(format!("{hash}.jpg"));

    // Already cached on disk
    if out_path.exists() {
        return Ok(Some(
            out_path
                .to_str()
                .ok_or_else(|| "Invalid path".to_string())?
                .to_string(),
        ));
    }

    let result = if IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        thumbnail_image(path.to_string(), out_path.clone()).await
    } else if ext == "heic" || ext == "heif" {
        // HEIC uses tile grids internally — FFmpeg assembles them via a complex
        // filtergraph, so we can't add -vf filters. Decode to temp PNG first,
        // then thumbnail with the image crate.
        thumbnail_via_decode(app, path, out_path.clone()).await
    } else if ext == "avif" {
        thumbnail_ffmpeg(app, path, &out_path, false).await
    } else {
        // Video
        thumbnail_ffmpeg(app, path, &out_path, true).await
    };

    match result {
        Ok(()) => Ok(Some(
            out_path
                .to_str()
                .ok_or_else(|| "Invalid path".to_string())?
                .to_string(),
        )),
        Err(e) => {
            // Clean up partial file
            let _ = std::fs::remove_file(&out_path);
            tracing::warn!(path = %path, error = %e, "Thumbnail generation failed");
            Ok(None)
        }
    }
}

#[tauri::command]
pub async fn generate_thumbnail(
    app: AppHandle,
    state: tauri::State<'_, ThumbnailSemaphore>,
    path: String,
) -> Result<Option<String>, String> {
    let _permit = state
        .0
        .acquire()
        .await
        .map_err(|e| format!("Semaphore error: {e}"))?;

    generate_one(&app, &path).await
}

#[tauri::command]
pub async fn generate_thumbnails_batch(
    app: AppHandle,
    state: tauri::State<'_, ThumbnailSemaphore>,
    paths: Vec<String>,
) -> Result<Vec<(String, Option<String>)>, String> {
    let sem = Arc::clone(&state.0);
    let mut set = tokio::task::JoinSet::new();

    for path in paths {
        let app = app.clone();
        let sem = Arc::clone(&sem);
        set.spawn(async move {
            let _permit = match sem.acquire_owned().await {
                Ok(p) => p,
                Err(_) => return (path, None),
            };
            let thumb = generate_one(&app, &path).await.unwrap_or(None);
            (path, thumb)
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        if let Ok(entry) = res {
            results.push(entry);
        }
    }
    Ok(results)
}

#[tauri::command]
pub async fn clear_thumbnail_cache() -> Result<(), String> {
    cleanup_thumbnail_cache();
    Ok(())
}

/// Delete the thumbnail cache directory. Called on queue clear and app exit.
pub fn cleanup_thumbnail_cache() {
    if let Ok(dir) = cache_dir() {
        if dir.exists() {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                tracing::warn!(error = %e, "Failed to clear thumbnail cache");
            }
        }
    }
}
