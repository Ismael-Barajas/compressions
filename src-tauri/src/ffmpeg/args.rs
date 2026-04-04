use crate::types::{AudioCodec, AudioExtractionOptions, AudioOutputFormat, DitherMode, GifConversionOptions, VideoCodec, VideoOptions};

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

    // Video codec
    let codec_str = match opts.codec {
        VideoCodec::H264 => "libx264",
        VideoCodec::H265 => "libx265",
        VideoCodec::AV1 => "libsvtav1",
    };
    args.push("-c:v".into());
    args.push(codec_str.into());

    // AV1 requires yuv420p pixel format
    if matches!(opts.codec, VideoCodec::AV1) {
        args.push("-pix_fmt".into());
        args.push("yuv420p".into());
    }

    // Quality (CRF)
    args.push("-crf".into());
    args.push(opts.crf.to_string());

    // Resolution — downscale only, maintain aspect ratio, ensure even dimensions
    if let Some(ref res) = opts.resolution {
        args.push("-vf".into());
        args.push(format!(
            "scale='min({w},iw)':'min({h},ih)':force_original_aspect_ratio=decrease,scale=trunc(iw/2)*2:trunc(ih/2)*2",
            w = res.width, h = res.height
        ));
    }

    // Bitrate (overrides CRF if set)
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
    if !matches!(opts.format, AudioOutputFormat::Flac | AudioOutputFormat::Wav) {
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
