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
    args.push("-c:v".into());
    args.push(
        match opts.codec {
            VideoCodec::H264 => "libx264",
            VideoCodec::H265 => "libx265",
            VideoCodec::AV1 => "libsvtav1",
        }
        .into(),
    );

    // Quality (CRF)
    args.push("-crf".into());
    args.push(opts.crf.to_string());

    // Resolution
    if let Some(ref res) = opts.resolution {
        args.push("-vf".into());
        // Use -2 to ensure even dimensions (required by most codecs)
        args.push(format!(
            "scale='min({},iw)':min'({},ih)':force_original_aspect_ratio=decrease",
            res.width, res.height
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
