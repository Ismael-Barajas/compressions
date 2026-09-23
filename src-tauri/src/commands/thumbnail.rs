use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use image::DynamicImage;
use tauri::{ipc::Channel, AppHandle};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::time::timeout;

use crate::media::{extension_of, VIDEO_EXTENSIONS};
use crate::state::ThumbnailSemaphore;
use crate::types::ThumbnailEvent;

const THUMB_SIZE: u32 = 160;
const FFMPEG_THUMB_TIMEOUT: Duration = Duration::from_secs(10);

/// Formats the `image` crate decodes directly. AVIF/HEIC go through FFmpeg.
const NATIVE_IMAGE_EXTENSIONS: &[&str] =
    &["jpg", "jpeg", "png", "webp", "bmp", "tiff", "tif", "gif"];

fn get_extension(path: &str) -> String {
    extension_of(Path::new(path))
}

/// Cache key for a file: its path plus size and mtime, so an edited file gets a
/// fresh thumbnail instead of the stale cached one.
fn path_hash(path: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    if let Ok(meta) = std::fs::metadata(path) {
        meta.len().hash(&mut hasher);
        if let Ok(modified) = meta.modified() {
            if let Ok(d) = modified.duration_since(std::time::UNIX_EPOCH) {
                d.as_secs().hash(&mut hasher);
            }
        }
    }
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

fn is_jpeg_path(path: &str) -> bool {
    matches!(get_extension(path).as_str(), "jpg" | "jpeg")
}

/// Decode a JPEG at the smallest DCT scale (1/2, 1/4 or 1/8) that still covers the
/// thumbnail box, skipping most of the IDCT and color conversion: a 12 MP photo
/// decodes to ~0.6 MB of pixels instead of ~36 MB. Returns `None` for anything
/// unusual (CMYK, 16-bit, decode errors) so the caller falls back to a full decode.
fn decode_jpeg_scaled(path: &str, min_side: u16) -> Option<DynamicImage> {
    use jpeg_decoder::{Decoder, PixelFormat};

    let file = std::fs::File::open(path).ok()?;
    let mut decoder = Decoder::new(std::io::BufReader::new(file));
    decoder.read_info().ok()?;
    let (w, h) = decoder.scale(min_side, min_side).ok()?;
    let pixels = decoder.decode().ok()?;
    let (w, h) = (u32::from(w), u32::from(h));
    match decoder.info()?.pixel_format {
        PixelFormat::RGB24 => image::RgbImage::from_raw(w, h, pixels).map(DynamicImage::ImageRgb8),
        PixelFormat::L8 => image::GrayImage::from_raw(w, h, pixels).map(DynamicImage::ImageLuma8),
        _ => None,
    }
}

/// Generate a thumbnail for an image using the `image` crate, write to disk.
async fn thumbnail_image(path: String, out_path: PathBuf) -> Result<(), String> {
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let scaled = if is_jpeg_path(&path) {
            decode_jpeg_scaled(&path, THUMB_SIZE as u16)
        } else {
            None
        };
        let img = match scaled {
            Some(img) => img,
            None => image::open(&path).map_err(|e| format!("Failed to open image: {e}"))?,
        };
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

/// Decode an image to a temp QOI via FFmpeg, then thumbnail with the `image` crate.
/// Used for formats the `image` crate can't decode directly (HEIC/HEIF) where FFmpeg's
/// tile grid assembly uses an internal complex filtergraph that conflicts with `-vf`.
/// QOI rather than PNG: for a 12 MP frame FFmpeg writes it in 0.68 s instead of 3.1 s
/// and it reads back in about half the time (same reason as the compress path).
async fn thumbnail_via_decode(
    app: &AppHandle,
    path: &str,
    out_path: PathBuf,
) -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    let temp_name = format!("compressions_thumb_{}.qoi", path_hash(path));
    let temp_path = temp_dir.join(&temp_name);
    let temp_str = temp_path
        .to_str()
        .ok_or_else(|| "Invalid temp path".to_string())?
        .to_string();

    // Decode to full-size QOI (no filters — avoids complex filtergraph conflict)
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

    // Now thumbnail the decoded frame with the image crate
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
    if !NATIVE_IMAGE_EXTENSIONS.contains(&ext.as_str())
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

    let result = if NATIVE_IMAGE_EXTENSIONS.contains(&ext.as_str()) {
        thumbnail_image(path.to_string(), out_path.clone()).await
    } else if ext == "heic" || ext == "heif" {
        // HEIC uses tile grids internally — FFmpeg assembles them via a complex
        // filtergraph, so we can't add -vf filters. Decode to a temp QOI first,
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

/// Generate thumbnails for `paths`, streaming each result on `on_result` as soon
/// as it is ready so one slow item (a video seek, a HEIC decode) never holds back
/// the rest of the visible rows.
#[tauri::command]
pub async fn generate_thumbnails_batch(
    app: AppHandle,
    state: tauri::State<'_, ThumbnailSemaphore>,
    paths: Vec<String>,
    on_result: Channel<ThumbnailEvent>,
) -> Result<(), String> {
    let sem = Arc::clone(&state.0);
    let mut set = tokio::task::JoinSet::new();

    for path in paths {
        let app = app.clone();
        let sem = Arc::clone(&sem);
        let on_result = on_result.clone();
        set.spawn(async move {
            let thumbnail_path = match sem.acquire_owned().await {
                Ok(_permit) => generate_one(&app, &path).await.unwrap_or(None),
                Err(_) => None,
            };
            let _ = on_result.send(ThumbnailEvent {
                path,
                thumbnail_path,
            });
        });
    }

    while set.join_next().await.is_some() {}
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_jpeg(path: &std::path::Path, img: DynamicImage) {
        img.save_with_format(path, image::ImageFormat::Jpeg)
            .unwrap();
    }

    #[test]
    fn scaled_decode_picks_smallest_scale_covering_the_box() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.jpg");
        write_jpeg(
            &path,
            DynamicImage::ImageRgb8(image::RgbImage::from_fn(1600, 1200, |x, y| {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 90])
            })),
        );
        let img = decode_jpeg_scaled(path.to_str().unwrap(), THUMB_SIZE as u16).unwrap();
        // A fit-inside thumbnail only needs the long side to reach 160 px, so 1/8
        // (200x150) is enough and still thumbnails to the same 160x120.
        assert_eq!((img.width(), img.height()), (200, 150));
        assert_eq!(img.thumbnail(THUMB_SIZE, THUMB_SIZE).width(), 160);
    }

    #[test]
    fn scaled_decode_keeps_grayscale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("gray.jpg");
        write_jpeg(
            &path,
            DynamicImage::ImageLuma8(image::GrayImage::from_fn(800, 600, |x, _| {
                image::Luma([(x % 256) as u8])
            })),
        );
        let img = decode_jpeg_scaled(path.to_str().unwrap(), THUMB_SIZE as u16).unwrap();
        assert!(matches!(img, DynamicImage::ImageLuma8(_)));
    }

    #[test]
    fn scaled_decode_rejects_corrupt_input_without_panicking() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.jpg");
        std::fs::write(&path, b"\xFF\xD8\xFF\xE0not really a jpeg").unwrap();
        assert!(decode_jpeg_scaled(path.to_str().unwrap(), THUMB_SIZE as u16).is_none());
    }

    #[tokio::test]
    async fn jpeg_thumbnail_fits_the_box() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("photo.jpg");
        write_jpeg(
            &path,
            DynamicImage::ImageRgb8(image::RgbImage::from_fn(2000, 1000, |x, y| {
                image::Rgb([(x % 256) as u8, (y % 256) as u8, 30])
            })),
        );
        let out = dir.path().join("thumb.jpg");
        thumbnail_image(path.to_string_lossy().to_string(), out.clone())
            .await
            .unwrap();
        assert_eq!(image::image_dimensions(&out).unwrap(), (160, 80));
    }
}
