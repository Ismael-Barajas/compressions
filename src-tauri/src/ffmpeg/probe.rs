use std::collections::HashSet;

use tauri::AppHandle;
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;

use crate::types::Resolution;

/// Known HW encoder names to look for in `ffmpeg -encoders` output.
const HW_ENCODERS: &[&str] = &[
    "h264_videotoolbox",
    "hevc_videotoolbox",
    "h264_nvenc",
    "hevc_nvenc",
];

/// Run `ffmpeg -encoders` and return the set of available HW encoders.
pub async fn detect_hw_encoders(app: &AppHandle) -> HashSet<String> {
    let result = detect_hw_encoders_inner(app).await;
    match result {
        Ok(encoders) => {
            tracing::info!(?encoders, "Detected HW encoders");
            encoders
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to detect HW encoders, using software only");
            HashSet::new()
        }
    }
}

async fn detect_hw_encoders_inner(app: &AppHandle) -> Result<HashSet<String>, String> {
    let (mut rx, _child) = app
        .shell()
        .sidecar("ffmpeg")
        .map_err(|e| format!("Failed to create FFmpeg sidecar: {}", e))?
        .args(["-encoders", "-hide_banner"])
        .spawn()
        .map_err(|e| format!("Failed to spawn FFmpeg: {}", e))?;

    let mut output = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                output.push_str(&String::from_utf8_lossy(&bytes));
            }
            CommandEvent::Terminated(_) => break,
            _ => {}
        }
    }

    let mut found = HashSet::new();
    for name in HW_ENCODERS {
        if output.contains(name) {
            found.insert(name.to_string());
        }
    }
    Ok(found)
}

pub struct VideoInfo {
    pub duration: Option<f64>,
    pub resolution: Option<Resolution>,
    pub codec_name: Option<String>,
}

pub async fn probe_video_duration(app: &AppHandle, path: &str) -> Result<f64, String> {
    let info = probe_video_info(app, path).await?;
    info.duration
        .ok_or_else(|| "Could not determine video duration".to_string())
}

pub async fn probe_video_info(app: &AppHandle, path: &str) -> Result<VideoInfo, String> {
    let args = vec![
        "-v",
        "quiet",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        path,
    ];

    let (mut rx, _child) = app
        .shell()
        .sidecar("ffprobe")
        .map_err(|e| format!("Failed to create ffprobe sidecar: {}", e))?
        .args(&args)
        .spawn()
        .map_err(|e| format!("Failed to spawn ffprobe: {}", e))?;

    let mut output = String::new();
    while let Some(event) = rx.recv().await {
        match event {
            CommandEvent::Stdout(bytes) => {
                output.push_str(&String::from_utf8_lossy(&bytes));
            }
            CommandEvent::Terminated(_) => break,
            _ => {}
        }
    }

    let json: serde_json::Value = serde_json::from_str(&output)
        .map_err(|e| format!("Failed to parse ffprobe output: {}", e))?;

    let duration = json["format"]["duration"]
        .as_str()
        .and_then(|d| d.parse::<f64>().ok());

    // Find video stream
    let video_stream = json["streams"].as_array().and_then(|streams| {
        streams
            .iter()
            .find(|s| s["codec_type"].as_str() == Some("video"))
    });

    let resolution = video_stream.and_then(|s| {
        let w = s["width"].as_u64()?;
        let h = s["height"].as_u64()?;
        Some(Resolution {
            width: w as u32,
            height: h as u32,
        })
    });

    let codec_name = video_stream
        .and_then(|s| s["codec_name"].as_str())
        .map(|s| s.to_string());

    Ok(VideoInfo {
        duration,
        resolution,
        codec_name,
    })
}
