use regex::Regex;
use std::sync::LazyLock;

use crate::types::{
    AudioCompressionOptions, AudioExtractionOptions, GifConversionOptions, PdfOptions, VideoOptions,
};

static BITRATE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+[kKmMgG]?$").unwrap());

fn validate_bitrate(value: &str, field: &str) -> Result<(), String> {
    if !BITRATE_RE.is_match(value) {
        return Err(format!(
            "Invalid {}: '{}' (expected e.g. 128k, 2M)",
            field, value
        ));
    }
    Ok(())
}

fn validate_range(value: u32, min: u32, max: u32, field: &str) -> Result<(), String> {
    if value < min || value > max {
        return Err(format!(
            "{} must be between {} and {} (got {})",
            field, min, max, value
        ));
    }
    Ok(())
}

pub fn validate_video_options(opts: &VideoOptions) -> Result<(), String> {
    validate_range(opts.crf as u32, 0, 51, "CRF")?;
    if let Some(ref bitrate) = opts.bitrate {
        validate_bitrate(bitrate, "video bitrate")?;
    }
    if let Some(ref ab) = opts.audio_bitrate {
        validate_bitrate(ab, "audio bitrate")?;
    }
    if let Some(ref res) = opts.resolution {
        validate_range(res.width, 16, 7680, "resolution width")?;
        validate_range(res.height, 16, 7680, "resolution height")?;
    }
    if let Some(fps) = opts.framerate {
        if !(1.0..=240.0).contains(&fps) {
            return Err(format!("framerate must be between 1 and 240 (got {})", fps));
        }
    }
    Ok(())
}

pub fn validate_audio_options(opts: &AudioExtractionOptions) -> Result<(), String> {
    if let Some(ref bitrate) = opts.bitrate {
        validate_bitrate(bitrate, "audio bitrate")?;
    }
    if let Some(sr) = opts.sample_rate {
        validate_range(sr, 8000, 192000, "sample rate")?;
    }
    Ok(())
}

pub fn validate_audio_compression_options(opts: &AudioCompressionOptions) -> Result<(), String> {
    if let Some(ref bitrate) = opts.bitrate {
        validate_bitrate(bitrate, "audio bitrate")?;
    }
    if let Some(sr) = opts.sample_rate {
        validate_range(sr, 8000, 192000, "sample rate")?;
    }
    Ok(())
}

pub fn validate_gif_options(opts: &GifConversionOptions) -> Result<(), String> {
    validate_range(opts.fps, 1, 120, "fps")?;
    validate_range(opts.max_colors, 2, 256, "max_colors")?;
    if let Some(w) = opts.width {
        validate_range(w, 16, 7680, "width")?;
    }
    Ok(())
}

pub fn validate_pdf_options(opts: &PdfOptions) -> Result<(), String> {
    if let Some(dpi) = opts.dpi {
        validate_range(dpi, 36, 1200, "DPI")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::*;

    #[test]
    fn valid_bitrates() {
        assert!(validate_bitrate("128k", "test").is_ok());
        assert!(validate_bitrate("2M", "test").is_ok());
        assert!(validate_bitrate("1500", "test").is_ok());
        assert!(validate_bitrate("256K", "test").is_ok());
    }

    #[test]
    fn invalid_bitrates() {
        assert!(validate_bitrate("", "test").is_err());
        assert!(validate_bitrate("abc", "test").is_err());
        assert!(validate_bitrate("128k; rm -rf /", "test").is_err());
        assert!(validate_bitrate("2M extra", "test").is_err());
    }

    #[test]
    fn valid_video_options() {
        let opts = VideoOptions {
            codec: VideoCodec::H264,
            crf: 23,
            resolution: Some(Resolution {
                width: 1920,
                height: 1080,
            }),
            bitrate: Some("2M".into()),
            framerate: Some(30.0),
            audio_codec: AudioCodec::AAC,
            audio_bitrate: Some("128k".into()),
            hw_encoder: None,
        };
        assert!(validate_video_options(&opts).is_ok());
    }

    #[test]
    fn rejects_extreme_crf() {
        let opts = VideoOptions {
            codec: VideoCodec::H264,
            crf: 52,
            resolution: None,
            bitrate: None,
            framerate: None,
            audio_codec: AudioCodec::None,
            audio_bitrate: None,
            hw_encoder: None,
        };
        assert!(validate_video_options(&opts).is_err());
    }

    #[test]
    fn rejects_extreme_resolution() {
        let opts = VideoOptions {
            codec: VideoCodec::H264,
            crf: 23,
            resolution: Some(Resolution {
                width: 99999,
                height: 1080,
            }),
            bitrate: None,
            framerate: None,
            audio_codec: AudioCodec::None,
            audio_bitrate: None,
            hw_encoder: None,
        };
        assert!(validate_video_options(&opts).is_err());
    }

    #[test]
    fn valid_gif_options() {
        let opts = GifConversionOptions {
            fps: 15,
            width: Some(320),
            max_colors: 256,
            dither: DitherMode::Bayer,
        };
        assert!(validate_gif_options(&opts).is_ok());
    }

    #[test]
    fn rejects_extreme_gif_fps() {
        let opts = GifConversionOptions {
            fps: 0,
            width: None,
            max_colors: 256,
            dither: DitherMode::Bayer,
        };
        assert!(validate_gif_options(&opts).is_err());
    }

    #[test]
    fn valid_pdf_options() {
        let opts = PdfOptions {
            quality: PdfQuality::Ebook,
            dpi: Some(150),
        };
        assert!(validate_pdf_options(&opts).is_ok());
    }

    #[test]
    fn rejects_extreme_dpi() {
        let opts = PdfOptions {
            quality: PdfQuality::Ebook,
            dpi: Some(0),
        };
        assert!(validate_pdf_options(&opts).is_err());
    }

    #[test]
    fn valid_audio_compression_options() {
        let opts = AudioCompressionOptions {
            format: AudioCompressionFormat::Mp3,
            bitrate: Some("192k".into()),
            sample_rate: Some(44100),
        };
        assert!(validate_audio_compression_options(&opts).is_ok());
    }

    #[test]
    fn valid_audio_compression_no_bitrate() {
        let opts = AudioCompressionOptions {
            format: AudioCompressionFormat::Flac,
            bitrate: None,
            sample_rate: None,
        };
        assert!(validate_audio_compression_options(&opts).is_ok());
    }

    #[test]
    fn rejects_invalid_audio_compression_bitrate() {
        let opts = AudioCompressionOptions {
            format: AudioCompressionFormat::Original,
            bitrate: Some("invalid; rm -rf /".into()),
            sample_rate: None,
        };
        assert!(validate_audio_compression_options(&opts).is_err());
    }

    #[test]
    fn rejects_extreme_audio_compression_sample_rate() {
        let opts = AudioCompressionOptions {
            format: AudioCompressionFormat::Aac,
            bitrate: Some("128k".into()),
            sample_rate: Some(500000),
        };
        assert!(validate_audio_compression_options(&opts).is_err());
    }
}
