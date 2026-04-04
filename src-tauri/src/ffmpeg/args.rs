use crate::types::{AudioCodec, VideoCodec, VideoOptions};

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
