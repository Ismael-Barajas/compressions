use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VideoCodec {
    H264,
    H265,
    AV1,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioCodec {
    AAC,
    Opus,
    Copy,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ImageFormat {
    Jpeg,
    Png,
    WebP,
    Avif,
    Gif,
    Heic,
    Original,
}

impl ImageFormat {
    /// Resolve `Original` to a concrete format based on the input file extension.
    /// BMP/TIFF have no output encoder — fall back to PNG (lossless).
    /// Returns self unchanged for concrete formats.
    pub fn resolve_for_input(&self, input_path: &str) -> ImageFormat {
        match self {
            ImageFormat::Original => {
                let ext = std::path::Path::new(input_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                match ext.as_str() {
                    "jpg" | "jpeg" => ImageFormat::Jpeg,
                    "png" => ImageFormat::Png,
                    "webp" => ImageFormat::WebP,
                    "avif" => ImageFormat::Avif,
                    "gif" => ImageFormat::Gif,
                    // HEIC can't be re-encoded (no HEIF muxer) — fall back to JPEG
                    "heic" | "heif" => ImageFormat::Jpeg,
                    // BMP, TIFF, unknown → lossless PNG fallback
                    _ => ImageFormat::Png,
                }
            }
            other => other.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioOutputFormat {
    Mp3,
    Aac,
    Flac,
    Opus,
    Wav,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioExtractionOptions {
    pub format: AudioOutputFormat,
    pub bitrate: Option<String>,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AudioCompressionFormat {
    Original,
    Mp3,
    Aac,
    Flac,
    Opus,
    Wav,
}

impl AudioCompressionFormat {
    /// Resolve `Original` to a concrete `AudioOutputFormat` based on the input file extension.
    /// Formats without a matching encoder fall back to MP3.
    pub fn resolve_for_input(&self, input_path: &str) -> AudioOutputFormat {
        match self {
            AudioCompressionFormat::Original => {
                let ext = std::path::Path::new(input_path)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_lowercase())
                    .unwrap_or_default();
                match ext.as_str() {
                    "mp3" => AudioOutputFormat::Mp3,
                    "aac" | "m4a" => AudioOutputFormat::Aac,
                    "flac" => AudioOutputFormat::Flac,
                    "ogg" | "opus" => AudioOutputFormat::Opus,
                    "wav" | "pcm" => AudioOutputFormat::Wav,
                    // WMA, AIFF, APE, ALAC, AC3, DTS, AMR, unknown → lossy MP3 fallback
                    _ => AudioOutputFormat::Mp3,
                }
            }
            AudioCompressionFormat::Mp3 => AudioOutputFormat::Mp3,
            AudioCompressionFormat::Aac => AudioOutputFormat::Aac,
            AudioCompressionFormat::Flac => AudioOutputFormat::Flac,
            AudioCompressionFormat::Opus => AudioOutputFormat::Opus,
            AudioCompressionFormat::Wav => AudioOutputFormat::Wav,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioCompressionOptions {
    pub format: AudioCompressionFormat,
    pub bitrate: Option<String>,
    pub sample_rate: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DitherMode {
    #[serde(rename = "bayer")]
    Bayer,
    #[serde(rename = "floyd_steinberg")]
    FloydSteinberg,
    #[serde(rename = "none")]
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GifConversionOptions {
    pub fps: u32,
    pub width: Option<u32>,
    pub max_colors: u32,
    pub dither: DitherMode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PdfQuality {
    #[serde(rename = "screen")]
    Screen,
    #[serde(rename = "ebook")]
    Ebook,
    #[serde(rename = "printer")]
    Printer,
    #[serde(rename = "prepress")]
    Prepress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfOptions {
    pub quality: PdfQuality,
    pub dpi: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MediaType {
    #[serde(rename = "video")]
    Video,
    #[serde(rename = "image")]
    Image,
    #[serde(rename = "pdf")]
    Pdf,
    #[serde(rename = "audio")]
    Audio,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resolution {
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoOptions {
    pub codec: VideoCodec,
    pub crf: u8,
    pub resolution: Option<Resolution>,
    pub bitrate: Option<String>,
    pub framerate: Option<f32>,
    pub audio_codec: AudioCodec,
    pub audio_bitrate: Option<String>,
    /// Set by backend when a HW encoder is available. Frontend never sends this.
    #[serde(default)]
    pub hw_encoder: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResizeMode {
    #[default]
    Fit,
    Exact,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageOptions {
    pub format: ImageFormat,
    pub quality: u8,
    pub resize: Option<Resolution>,
    #[serde(default)]
    pub resize_mode: ResizeMode,
    pub strip_metadata: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProgressPayload {
    pub job_id: String,
    pub file_name: String,
    pub percent: f32,
    pub current_frame: Option<u64>,
    pub total_frames: Option<u64>,
    pub speed: Option<String>,
    pub eta_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompressionResult {
    pub job_id: String,
    pub input_path: String,
    pub output_path: String,
    pub input_size: u64,
    pub output_size: u64,
    pub duration_ms: u64,
    pub success: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileInfo {
    pub path: String,
    pub file_name: String,
    pub size: u64,
    pub media_type: MediaType,
    pub duration: Option<f64>,
    pub resolution: Option<Resolution>,
    pub codec_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeEvent {
    pub path: String,
    pub size: u64,
    pub resolution: Option<Resolution>,
    pub duration: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum ProgressEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        job_id: String,
        file_name: String,
        input_path: String,
    },
    Progress(ProgressPayload),
    Completed(CompressionResult),
    #[serde(rename_all = "camelCase")]
    Error {
        job_id: String,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEntry {
    pub input: String,
    pub output: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub timestamp: String,
    pub input_path: String,
    pub output_path: String,
    pub input_size: u64,
    pub output_size: u64,
    pub duration_ms: u64,
    pub media_type: String,
    pub success: bool,
    pub error: Option<String>,
}

impl HistoryEntry {
    pub fn from_result(result: &CompressionResult, media_type: &str) -> Self {
        Self {
            id: result.job_id.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            input_path: result.input_path.clone(),
            output_path: result.output_path.clone(),
            input_size: result.input_size,
            output_size: result.output_size,
            duration_ms: result.duration_ms,
            media_type: media_type.to_string(),
            success: result.success,
            error: result.error.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub media_type: MediaType,
    pub video_options: Option<VideoOptions>,
    pub image_options: Option<ImageOptions>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_compression_format_resolve_known() {
        let original = AudioCompressionFormat::Original;
        assert!(matches!(
            original.resolve_for_input("song.mp3"),
            AudioOutputFormat::Mp3
        ));
        assert!(matches!(
            original.resolve_for_input("track.aac"),
            AudioOutputFormat::Aac
        ));
        assert!(matches!(
            original.resolve_for_input("track.m4a"),
            AudioOutputFormat::Aac
        ));
        assert!(matches!(
            original.resolve_for_input("album.flac"),
            AudioOutputFormat::Flac
        ));
        assert!(matches!(
            original.resolve_for_input("podcast.ogg"),
            AudioOutputFormat::Opus
        ));
        assert!(matches!(
            original.resolve_for_input("voice.opus"),
            AudioOutputFormat::Opus
        ));
        assert!(matches!(
            original.resolve_for_input("raw.wav"),
            AudioOutputFormat::Wav
        ));
        assert!(matches!(
            original.resolve_for_input("raw.pcm"),
            AudioOutputFormat::Wav
        ));
    }

    #[test]
    fn audio_compression_format_resolve_fallback() {
        let original = AudioCompressionFormat::Original;
        // Niche formats fall back to MP3
        for ext in &["wma", "aiff", "ape", "alac", "ac3", "dts", "amr"] {
            assert!(
                matches!(
                    original.resolve_for_input(&format!("file.{}", ext)),
                    AudioOutputFormat::Mp3
                ),
                "expected MP3 fallback for .{}",
                ext
            );
        }
    }

    #[test]
    fn audio_compression_format_concrete_passthrough() {
        assert!(matches!(
            AudioCompressionFormat::Aac.resolve_for_input("anything.xyz"),
            AudioOutputFormat::Aac
        ));
        assert!(matches!(
            AudioCompressionFormat::Flac.resolve_for_input("anything.xyz"),
            AudioOutputFormat::Flac
        ));
        assert!(matches!(
            AudioCompressionFormat::Wav.resolve_for_input("anything.xyz"),
            AudioOutputFormat::Wav
        ));
    }
}
