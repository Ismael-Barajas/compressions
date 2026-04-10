use crate::types::{
    AudioCodec, AudioExtractionOptions, AudioOutputFormat, DitherMode, GifConversionOptions,
    VideoCodec, VideoOptions,
};

pub fn build_video_args(input: &str, output: &str, opts: &VideoOptions) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        // Progress output to stderr
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        "0.1".into(),
    ];

    // Video codec — use HW encoder if backend resolved one, else software
    let is_hw = opts.hw_encoder.is_some();
    let codec_str = match &opts.hw_encoder {
        Some(hw) => hw.clone(),
        None => match opts.codec {
            VideoCodec::H264 => "libx264".into(),
            VideoCodec::H265 => "libx265".into(),
            VideoCodec::AV1 => "libsvtav1".into(),
        },
    };
    args.push("-c:v".into());
    args.push(codec_str);

    if !is_hw {
        // Encoder preset — "fast" for x264/x265, "7" for SVT-AV1
        args.push("-preset".into());
        args.push(
            match opts.codec {
                VideoCodec::H264 | VideoCodec::H265 => "fast",
                VideoCodec::AV1 => "7",
            }
            .into(),
        );
    }

    // AV1 requires yuv420p pixel format
    if matches!(opts.codec, VideoCodec::AV1) {
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    }

    // Quality — HW encoders use codec-specific rate control, software uses CRF
    let is_nvenc = opts
        .hw_encoder
        .as_deref()
        .is_some_and(|hw| hw.contains("nvenc"));

    if is_hw {
        if is_nvenc {
            // NVENC: constant quality VBR. -cq takes the same 0-51 scale as CRF.
            args.push("-rc".into());
            args.push("vbr".into());
            args.push("-cq".into());
            args.push(opts.crf.to_string());
            if opts.bitrate.is_none() {
                args.push("-b:v".into());
                args.push("0".into());
            }
            // p5 = balanced quality/speed; hardcoded since NVENC presets aren't user-facing
            args.push("-preset".into());
            args.push("p5".into());
        } else {
            // Fallback HW path (videotoolbox-style): -q:v on a 0-100 scale (higher = better)
            let q = ((51u16.saturating_sub(opts.crf.min(51) as u16)) * 100 / 51) as u8;
            args.push("-q:v".into());
            args.push(q.to_string());
        }
    } else {
        args.push("-crf".into());
        args.push(opts.crf.to_string());
    }

    // Resolution — downscale only, maintain aspect ratio, ensure even dimensions
    if let Some(ref res) = opts.resolution {
        args.push("-vf".into());
        args.push(format!(
            "scale='min({w},iw)':'min({h},ih)':force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2",
            w = res.width, h = res.height
        ));
    }

    // Bitrate (overrides CRF/CQ if set)
    if let Some(ref bitrate) = opts.bitrate {
        args.push("-b:v".into());
        args.push(bitrate.clone());
    }

    // Frame rate
    if let Some(fps) = opts.framerate {
        args.push("-r".into());
        args.push(fps.to_string());
    }

    // Audio
    match opts.audio_codec {
        AudioCodec::None => {
            args.push("-an".into());
        }
        AudioCodec::Copy => {
            args.push("-c:a".into());
            args.push("copy".into());
        }
        AudioCodec::AAC => {
            args.push("-c:a".into());
            args.push("aac".into());
            if let Some(ref ab) = opts.audio_bitrate {
                args.push("-b:a".into());
                args.push(ab.clone());
            }
        }
        AudioCodec::Opus => {
            args.push("-c:a".into());
            args.push("libopus".into());
            if let Some(ref ab) = opts.audio_bitrate {
                args.push("-b:a".into());
                args.push(ab.clone());
            }
        }
    }

    // Faststart for MP4 (moves moov atom to front for streaming)
    if output.ends_with(".mp4") {
        args.push("-movflags".into());
        args.push("+faststart".into());
    }

    args.push(output.into());
    args
}

pub fn build_audio_extraction_args(
    input: &str,
    output: &str,
    opts: &AudioExtractionOptions,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        "0.1".into(),
        // Strip video stream
        "-vn".into(),
    ];

    // Audio codec
    let codec_str = match opts.format {
        AudioOutputFormat::Mp3 => "libmp3lame",
        AudioOutputFormat::Aac => "aac",
        AudioOutputFormat::Flac => "flac",
        AudioOutputFormat::Opus => "libopus",
        AudioOutputFormat::Wav => "pcm_s16le",
    };
    args.push("-c:a".into());
    args.push(codec_str.into());

    // Bitrate (not applicable for lossless formats)
    if !matches!(
        opts.format,
        AudioOutputFormat::Flac | AudioOutputFormat::Wav
    ) {
        if let Some(ref bitrate) = opts.bitrate {
            args.push("-b:a".into());
            args.push(bitrate.clone());
        }
    }

    // Sample rate
    if let Some(sr) = opts.sample_rate {
        args.push("-ar".into());
        args.push(sr.to_string());
    }

    args.push(output.into());
    args
}

/// Build args for video-to-GIF palette generation pass.
pub fn build_gif_palette_args(
    input: &str,
    palette_path: &str,
    opts: &GifConversionOptions,
) -> Vec<String> {
    let mut filter_parts: Vec<String> = Vec::new();
    filter_parts.push(format!("fps={}", opts.fps));
    if let Some(w) = opts.width {
        filter_parts.push(format!("scale={}:-1:flags=lanczos", w));
    }
    filter_parts.push(format!(
        "palettegen=max_colors={}:stats_mode=diff",
        opts.max_colors
    ));

    let filter = filter_parts.join(",");

    vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-vf".into(),
        filter,
        palette_path.into(),
    ]
}

/// Build args for video-to-GIF encoding pass using a pre-generated palette.
pub fn build_gif_encode_args(
    input: &str,
    palette_path: &str,
    output: &str,
    opts: &GifConversionOptions,
) -> Vec<String> {
    let dither_str = match opts.dither {
        DitherMode::Bayer => "dither=bayer:bayer_scale=3",
        DitherMode::FloydSteinberg => "dither=floyd_steinberg",
        DitherMode::None => "dither=none",
    };

    let mut filter_parts: Vec<String> = Vec::new();
    filter_parts.push(format!("fps={}", opts.fps));
    if let Some(w) = opts.width {
        filter_parts.push(format!("scale={}:-1:flags=lanczos", w));
    }

    let filter_prefix = filter_parts.join(",");

    let filtergraph = format!(
        "[0:v]{filter}[x];[x][1:v]paletteuse={dither}",
        filter = filter_prefix,
        dither = dither_str,
    );

    vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-i".into(),
        palette_path.into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        "0.1".into(),
        "-lavfi".into(),
        filtergraph,
        output.into(),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_video_opts() -> VideoOptions {
        VideoOptions {
            codec: VideoCodec::H264,
            crf: 23,
            resolution: None,
            bitrate: None,
            framerate: None,
            audio_codec: AudioCodec::AAC,
            audio_bitrate: Some("128k".into()),
            hw_encoder: None,
        }
    }

    #[test]
    fn h264_basic_args() {
        let args = build_video_args("in.mp4", "out.mp4", &default_video_opts());
        assert!(args.contains(&"libx264".to_string()));
        assert!(args.contains(&"-preset".to_string()));
        assert!(args.contains(&"fast".to_string()));
        assert!(args.contains(&"-crf".to_string()));
        assert!(args.contains(&"23".to_string()));
        assert!(args.contains(&"+faststart".to_string()));
        assert!(!args.contains(&"-pix_fmt".to_string()));
    }

    #[test]
    fn h265_codec() {
        let mut opts = default_video_opts();
        opts.codec = VideoCodec::H265;
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"libx265".to_string()));
        assert!(args.contains(&"fast".to_string()));
    }

    #[test]
    fn av1_has_pix_fmt() {
        let mut opts = default_video_opts();
        opts.codec = VideoCodec::AV1;
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"libsvtav1".to_string()));
        assert!(args.contains(&"7".to_string()));
        assert!(args.contains(&"-pix_fmt".to_string()));
        assert!(args.contains(&"yuv420p".to_string()));
    }

    #[test]
    fn resolution_filter() {
        let mut opts = default_video_opts();
        opts.resolution = Some(crate::types::Resolution {
            width: 1280,
            height: 720,
        });
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"-vf".to_string()));
        let vf = args.iter().find(|a| a.contains("scale=")).unwrap();
        assert!(vf.contains("1280"));
        assert!(vf.contains("720"));
    }

    #[test]
    fn audio_none_strips_audio() {
        let mut opts = default_video_opts();
        opts.audio_codec = AudioCodec::None;
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"-an".to_string()));
    }

    #[test]
    fn audio_copy() {
        let mut opts = default_video_opts();
        opts.audio_codec = AudioCodec::Copy;
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"copy".to_string()));
    }

    #[test]
    fn bitrate_and_fps() {
        let mut opts = default_video_opts();
        opts.bitrate = Some("2M".into());
        opts.framerate = Some(30.0);
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"-b:v".to_string()));
        assert!(args.contains(&"2M".to_string()));
        assert!(args.contains(&"-r".to_string()));
        assert!(args.contains(&"30".to_string()));
    }

    #[test]
    fn no_faststart_for_mkv() {
        let args = build_video_args("in.mkv", "out.mkv", &default_video_opts());
        assert!(!args.contains(&"+faststart".to_string()));
    }

    #[test]
    fn audio_extraction_mp3() {
        let opts = AudioExtractionOptions {
            format: AudioOutputFormat::Mp3,
            bitrate: Some("192k".into()),
            sample_rate: None,
        };
        let args = build_audio_extraction_args("in.mp4", "out.mp3", &opts);
        assert!(args.contains(&"libmp3lame".to_string()));
        assert!(args.contains(&"-vn".to_string()));
        assert!(args.contains(&"-b:a".to_string()));
        assert!(args.contains(&"192k".to_string()));
    }

    #[test]
    fn audio_extraction_flac_no_bitrate() {
        let opts = AudioExtractionOptions {
            format: AudioOutputFormat::Flac,
            bitrate: Some("192k".into()),
            sample_rate: Some(44100),
        };
        let args = build_audio_extraction_args("in.mp4", "out.flac", &opts);
        assert!(args.contains(&"flac".to_string()));
        assert!(!args.contains(&"-b:a".to_string()));
        assert!(args.contains(&"-ar".to_string()));
        assert!(args.contains(&"44100".to_string()));
    }

    #[test]
    fn gif_palette_args() {
        let opts = GifConversionOptions {
            fps: 15,
            width: Some(320),
            max_colors: 256,
            dither: DitherMode::Bayer,
        };
        let args = build_gif_palette_args("in.mp4", "palette.png", &opts);
        let vf = args.iter().find(|a| a.contains("fps=")).unwrap();
        assert!(vf.contains("fps=15"));
        assert!(vf.contains("scale=320"));
        assert!(vf.contains("palettegen"));
    }

    #[test]
    fn gif_encode_dither_modes() {
        let mut opts = GifConversionOptions {
            fps: 10,
            width: None,
            max_colors: 128,
            dither: DitherMode::FloydSteinberg,
        };
        let args = build_gif_encode_args("in.mp4", "palette.png", "out.gif", &opts);
        let lavfi = args.iter().find(|a| a.contains("paletteuse")).unwrap();
        assert!(lavfi.contains("dither=floyd_steinberg"));

        opts.dither = DitherMode::None;
        let args = build_gif_encode_args("in.mp4", "palette.png", "out.gif", &opts);
        let lavfi = args.iter().find(|a| a.contains("paletteuse")).unwrap();
        assert!(lavfi.contains("dither=none"));
    }

    #[test]
    fn hw_encoder_uses_qv_instead_of_crf() {
        let mut opts = default_video_opts();
        opts.hw_encoder = Some("h264_videotoolbox".into());
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"h264_videotoolbox".to_string()));
        assert!(args.contains(&"-q:v".to_string()));
        assert!(!args.contains(&"-crf".to_string()));
        assert!(!args.contains(&"-preset".to_string()));
    }

    #[test]
    fn hw_encoder_quality_mapping() {
        let mut opts = default_video_opts();
        opts.crf = 23;
        opts.hw_encoder = Some("hevc_videotoolbox".into());
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        // CRF 23 → q = (51-23)*100/51 = 54
        let qv_idx = args.iter().position(|a| a == "-q:v").unwrap();
        assert_eq!(args[qv_idx + 1], "54");
    }

    #[test]
    fn nvenc_basic_args() {
        let mut opts = default_video_opts();
        opts.crf = 28;
        opts.hw_encoder = Some("h264_nvenc".into());
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"h264_nvenc".to_string()));
        assert!(args.contains(&"-rc".to_string()));
        assert!(args.contains(&"vbr".to_string()));
        assert!(args.contains(&"-cq".to_string()));
        assert!(args.contains(&"28".to_string()));
        assert!(args.contains(&"-b:v".to_string()));
        assert!(args.contains(&"0".to_string()));
        assert!(args.contains(&"-preset".to_string()));
        assert!(args.contains(&"p5".to_string()));
        // Must not use software-style flags
        assert!(!args.contains(&"-crf".to_string()));
        assert!(!args.contains(&"-q:v".to_string()));
    }

    #[test]
    fn nvenc_hevc_basic_args() {
        let mut opts = default_video_opts();
        opts.hw_encoder = Some("hevc_nvenc".into());
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        assert!(args.contains(&"hevc_nvenc".to_string()));
        assert!(args.contains(&"-rc".to_string()));
        assert!(args.contains(&"-cq".to_string()));
    }

    #[test]
    fn nvenc_with_bitrate_skips_bv_zero() {
        let mut opts = default_video_opts();
        opts.hw_encoder = Some("h264_nvenc".into());
        opts.bitrate = Some("5M".into());
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        // Should have -cq (quality) and -b:v 5M, but NOT -b:v 0
        assert!(args.contains(&"-cq".to_string()));
        assert!(args.contains(&"5M".to_string()));
        assert!(!args.contains(&"0".to_string()));
        // Only one -b:v flag
        let bv_count = args.iter().filter(|a| *a == "-b:v").count();
        assert_eq!(bv_count, 1);
    }
}
