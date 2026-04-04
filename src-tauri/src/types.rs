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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageOptions {
    pub format: ImageFormat,
    pub quality: u8,
    pub resize: Option<Resolution>,
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
#[serde(rename_all = "camelCase", tag = "event", content = "data")]
pub enum ProgressEvent {
    #[serde(rename_all = "camelCase")]
    Started {
        job_id: String,
        file_name: String,
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
pub struct Preset {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_builtin: bool,
    pub media_type: MediaType,
    pub video_options: Option<VideoOptions>,
    pub image_options: Option<ImageOptions>,
}
