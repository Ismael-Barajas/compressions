use crate::types::{
    AudioCodec, AudioExtractionOptions, AudioOutputFormat, DitherMode, GifConversionOptions,
    Resolution, VideoCodec, VideoOptions,
};

/// How often FFmpeg emits a progress block. The backend additionally throttles
/// what reaches the UI (see `ProgressThrottle`), so this only bounds parse work.
const PROGRESS_PERIOD: &str = "0.25";

fn audio_codec_name(format: &AudioOutputFormat) -> &'static str {
    match format {
        AudioOutputFormat::Mp3 => "libmp3lame",
        AudioOutputFormat::Aac => "aac",
        AudioOutputFormat::Flac => "flac",
        AudioOutputFormat::Opus => "libopus",
        AudioOutputFormat::Wav => "pcm_s16le",
    }
}

fn is_lossless(format: &AudioOutputFormat) -> bool {
    matches!(format, AudioOutputFormat::Flac | AudioOutputFormat::Wav)
}

fn push_audio_encoding_args(args: &mut Vec<String>, opts: &AudioExtractionOptions) {
    args.push("-c:a".into());
    args.push(audio_codec_name(&opts.format).into());

    // Bitrate (not applicable for lossless formats)
    if !is_lossless(&opts.format) {
        if let Some(ref bitrate) = opts.bitrate {
            args.push("-b:a".into());
            args.push(bitrate.clone());
        }
    }

    if let Some(sr) = opts.sample_rate {
        args.push("-ar".into());
        args.push(sr.to_string());
    }
}

pub fn build_video_args(input: &str, output: &str, opts: &VideoOptions) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        // Machine-readable progress blocks on stderr; `-nostats` drops the human
        // stats line so every block is parsed exactly once.
        "-nostats".into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        PROGRESS_PERIOD.into(),
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

    // Force yuv420p for maximum player compatibility (WMP, QuickTime, etc.)
    if !is_hw {
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

    // Resolution — downscale only, maintain aspect ratio, ensure even dimensions.
    // `force_divisible_by` rounds in the same scaler instead of a second `scale` pass.
    if let Some(ref res) = opts.resolution {
        args.push("-vf".into());
        args.push(format!(
            "scale='min({w},iw)':'min({h},ih)':force_original_aspect_ratio=decrease:force_divisible_by=2",
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
    if output.to_lowercase().ends_with(".mp4") {
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
        "-nostats".into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        PROGRESS_PERIOD.into(),
        // Strip video stream
        "-vn".into(),
    ];

    push_audio_encoding_args(&mut args, opts);

    args.push(output.into());
    args
}

/// Embedded cover art arrives as an attached-picture video stream. Left alone,
/// FFmpeg transcodes it to the muxer's default video codec: a JPEG cover becomes a
/// multi-MB PNG (MP3/FLAC, often making the output larger than the input), libx264
/// output the ipod muxer then rejects (M4A fails outright), or a Theora track (Ogg).
/// Keep the cover's bytes where the container supports it; drop it where it doesn't.
fn push_cover_art_args(args: &mut Vec<String>, format: &AudioOutputFormat) {
    match format {
        AudioOutputFormat::Mp3 | AudioOutputFormat::Flac | AudioOutputFormat::Aac => {
            args.push("-c:v".into());
            args.push("copy".into());
        }
        // The Ogg muxer can't stream-copy an image and WAV has no place for one.
        AudioOutputFormat::Opus | AudioOutputFormat::Wav => args.push("-vn".into()),
    }
}

/// Build FFmpeg args for audio compression. Unlike extraction, the video stream (if
/// any) is the file's cover art, which is copied or dropped rather than re-encoded.
/// The caller must resolve `Original` to a concrete `AudioOutputFormat` before calling this.
pub fn build_audio_compression_args(
    input: &str,
    output: &str,
    opts: &AudioExtractionOptions,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        PROGRESS_PERIOD.into(),
    ];

    push_audio_encoding_args(&mut args, opts);
    push_cover_art_args(&mut args, &opts.format);

    args.push(output.into());
    args
}

fn gif_dither_arg(dither: &DitherMode) -> &'static str {
    match dither {
        DitherMode::Bayer => "dither=bayer:bayer_scale=3",
        DitherMode::FloydSteinberg => "dither=floyd_steinberg",
        DitherMode::None => "dither=none",
    }
}

fn gif_prefilter(opts: &GifConversionOptions) -> String {
    let mut parts = vec![format!("fps={}", opts.fps)];
    if let Some(w) = opts.width {
        parts.push(format!("scale={}:-1:flags=lanczos", w));
    }
    parts.join(",")
}

/// Memory the single-pass GIF graph may buffer before falling back to two passes.
pub const GIF_SINGLE_PASS_BUDGET_BYTES: u64 = 256 * 1024 * 1024;

/// Bytes the single-pass graph holds at its peak. `palettegen` only emits the
/// palette at end of stream, so every filtered frame queues on the `paletteuse`
/// branch until then: frames × output width × output height × 4 (RGB32). Measured
/// on a 2-minute 1080p clip: 916 MB at 480 px wide, 11.6 GB at original width.
/// `None` when the duration or source resolution is unknown.
pub fn gif_single_pass_buffer_bytes(
    opts: &GifConversionOptions,
    duration: Option<f64>,
    source: Option<&Resolution>,
) -> Option<u64> {
    let duration = duration.filter(|d| d.is_finite() && *d > 0.0)?;
    let src = source.filter(|r| r.width > 0 && r.height > 0)?;
    let (w, h) = match opts.width {
        Some(w) if w > 0 => {
            let h = (src.height as f64 * w as f64 / src.width as f64).round();
            (w as f64, h.max(1.0))
        }
        _ => (src.width as f64, src.height as f64),
    };
    let frames = (duration * opts.fps as f64).ceil();
    Some((frames * w * h * 4.0) as u64)
}

/// Single pass reads the source once (about 13% faster at the default 480 px), but
/// its memory grows with the clip. Use it only when the buffer is known to be small.
pub fn gif_prefers_single_pass(
    opts: &GifConversionOptions,
    duration: Option<f64>,
    source: Option<&Resolution>,
) -> bool {
    gif_single_pass_buffer_bytes(opts, duration, source)
        .is_some_and(|bytes| bytes <= GIF_SINGLE_PASS_BUDGET_BYTES)
}

fn progress_args() -> [String; 5] {
    [
        "-nostats".into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        PROGRESS_PERIOD.into(),
    ]
}

/// Two-pass GIF, pass 1: analyze the whole clip into a palette image. Memory stays
/// flat regardless of clip length.
pub fn build_gif_palette_args(
    input: &str,
    palette: &str,
    opts: &GifConversionOptions,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-y".into(), "-i".into(), input.into()];
    args.extend(progress_args());
    args.push("-vf".into());
    args.push(format!(
        "{pre},palettegen=max_colors={colors}:stats_mode=diff",
        pre = gif_prefilter(opts),
        colors = opts.max_colors,
    ));
    args.push(palette.into());
    args
}

/// Two-pass GIF, pass 2: re-read the source and map it onto the palette.
pub fn build_gif_paletteuse_args(
    input: &str,
    palette: &str,
    output: &str,
    opts: &GifConversionOptions,
) -> Vec<String> {
    let mut args: Vec<String> = vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-i".into(),
        palette.into(),
    ];
    args.extend(progress_args());
    args.push("-filter_complex".into());
    args.push(format!(
        "[0:v]{pre}[x];[x][1:v]paletteuse={dither}",
        pre = gif_prefilter(opts),
        dither = gif_dither_arg(&opts.dither),
    ));
    args.push(output.into());
    args
}

/// Build args for a single-pass video-to-GIF conversion. The decoded, scaled
/// stream is split so `palettegen` and `paletteuse` share one decode: the source
/// is read once instead of twice and no temporary palette file is written. Only
/// for short clips; see [`gif_prefers_single_pass`].
pub fn build_gif_single_pass_args(
    input: &str,
    output: &str,
    opts: &GifConversionOptions,
) -> Vec<String> {
    let filtergraph = format!(
        "[0:v]{pre},split[a][b];[a]palettegen=max_colors={colors}:stats_mode=diff[p];[b][p]paletteuse={dither}",
        pre = gif_prefilter(opts),
        colors = opts.max_colors,
        dither = gif_dither_arg(&opts.dither),
    );

    vec![
        "-y".into(),
        "-i".into(),
        input.into(),
        "-nostats".into(),
        "-progress".into(),
        "pipe:2".into(),
        "-stats_period".into(),
        PROGRESS_PERIOD.into(),
        "-filter_complex".into(),
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
        assert!(args.contains(&"-pix_fmt".to_string()));
        assert!(args.contains(&"yuv420p".to_string()));
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
    fn gif_single_pass_reads_input_once() {
        let opts = GifConversionOptions {
            fps: 12,
            width: Some(480),
            max_colors: 128,
            dither: DitherMode::Bayer,
        };
        let args = build_gif_single_pass_args("in.mp4", "out.gif", &opts);
        assert_eq!(args.iter().filter(|a| *a == "-i").count(), 1);
        let fc_idx = args.iter().position(|a| a == "-filter_complex").unwrap();
        let graph = &args[fc_idx + 1];
        assert!(graph.contains("fps=12,scale=480:-1:flags=lanczos,split[a][b]"));
        assert!(graph.contains("[a]palettegen=max_colors=128:stats_mode=diff[p]"));
        assert!(graph.contains("[b][p]paletteuse=dither=bayer:bayer_scale=3"));
        assert!(args.contains(&"-progress".to_string()));
        assert_eq!(args.last().map(String::as_str), Some("out.gif"));
    }

    fn audio_opts(format: AudioOutputFormat) -> AudioExtractionOptions {
        AudioExtractionOptions {
            format,
            bitrate: Some("192k".into()),
            sample_rate: None,
        }
    }

    #[test]
    fn audio_compression_copies_cover_art_where_supported() {
        for format in [
            AudioOutputFormat::Mp3,
            AudioOutputFormat::Flac,
            AudioOutputFormat::Aac,
        ] {
            let args = build_audio_compression_args("in", "out", &audio_opts(format));
            assert!(args.windows(2).any(|p| p == ["-c:v", "copy"]));
            assert!(!args.contains(&"-vn".to_string()));
        }
    }

    #[test]
    fn audio_compression_drops_cover_art_where_unsupported() {
        for format in [AudioOutputFormat::Opus, AudioOutputFormat::Wav] {
            let args = build_audio_compression_args("in", "out", &audio_opts(format));
            assert!(args.contains(&"-vn".to_string()));
            assert!(!args.contains(&"-c:v".to_string()));
        }
    }

    fn gif_opts(width: Option<u32>) -> GifConversionOptions {
        GifConversionOptions {
            fps: 12,
            width,
            max_colors: 128,
            dither: DitherMode::Bayer,
        }
    }

    #[test]
    fn gif_buffer_estimate_matches_frame_math() {
        let src = Resolution {
            width: 1920,
            height: 1080,
        };
        // 120 s × 12 fps × 480×270 × 4 bytes
        assert_eq!(
            gif_single_pass_buffer_bytes(&gif_opts(Some(480)), Some(120.0), Some(&src)),
            Some(1440 * 480 * 270 * 4)
        );
        // Original width keeps the source size.
        assert_eq!(
            gif_single_pass_buffer_bytes(&gif_opts(None), Some(1.0), Some(&src)),
            Some(12 * 1920 * 1080 * 4)
        );
        assert_eq!(
            gif_single_pass_buffer_bytes(&gif_opts(Some(480)), None, Some(&src)),
            None
        );
        assert_eq!(
            gif_single_pass_buffer_bytes(&gif_opts(Some(480)), Some(10.0), None),
            None
        );
    }

    #[test]
    fn gif_single_pass_only_for_small_buffers() {
        let src = Resolution {
            width: 1920,
            height: 1080,
        };
        // 10 s at 480 px ≈ 62 MB: single pass.
        assert!(gif_prefers_single_pass(
            &gif_opts(Some(480)),
            Some(10.0),
            Some(&src)
        ));
        // 2 minutes at 480 px ≈ 746 MB: two passes.
        assert!(!gif_prefers_single_pass(
            &gif_opts(Some(480)),
            Some(120.0),
            Some(&src)
        ));
        // 10 s at original 1080p ≈ 1 GB: two passes.
        assert!(!gif_prefers_single_pass(
            &gif_opts(None),
            Some(10.0),
            Some(&src)
        ));
        // Unknown size: take the path with bounded memory.
        assert!(!gif_prefers_single_pass(&gif_opts(Some(480)), None, None));
    }

    #[test]
    fn gif_two_pass_args_share_the_prefilter() {
        let opts = gif_opts(Some(480));
        let pass1 = build_gif_palette_args("in.mp4", "pal.png", &opts);
        let vf = pass1.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(
            pass1[vf + 1],
            "fps=12,scale=480:-1:flags=lanczos,palettegen=max_colors=128:stats_mode=diff"
        );
        assert_eq!(pass1.last().map(String::as_str), Some("pal.png"));
        assert!(pass1.contains(&"-progress".to_string()));

        let pass2 = build_gif_paletteuse_args("in.mp4", "pal.png", "out.gif", &opts);
        assert_eq!(pass2.iter().filter(|a| *a == "-i").count(), 2);
        let fc = pass2.iter().position(|a| a == "-filter_complex").unwrap();
        assert_eq!(
            pass2[fc + 1],
            "[0:v]fps=12,scale=480:-1:flags=lanczos[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=3"
        );
        assert_eq!(pass2.last().map(String::as_str), Some("out.gif"));
    }

    #[test]
    fn resolution_filter_rounds_to_even_in_one_scaler() {
        let mut opts = default_video_opts();
        opts.resolution = Some(Resolution {
            width: 1280,
            height: 720,
        });
        let args = build_video_args("in.mp4", "out.mp4", &opts);
        let vf = args.iter().position(|a| a == "-vf").unwrap();
        assert_eq!(args[vf + 1].matches("scale=").count(), 1);
        assert!(args[vf + 1].ends_with("force_divisible_by=2"));
    }

    #[test]
    fn progress_args_disable_stats_line() {
        let args = build_video_args("in.mp4", "out.mp4", &default_video_opts());
        assert!(args.contains(&"-nostats".to_string()));
        assert!(args.contains(&"-progress".to_string()));
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
    fn audio_compression_mp3_no_vn() {
        let opts = AudioExtractionOptions {
            format: AudioOutputFormat::Mp3,
            bitrate: Some("192k".into()),
            sample_rate: None,
        };
        let args = build_audio_compression_args("in.mp3", "out.mp3", &opts);
        assert!(args.contains(&"libmp3lame".to_string()));
        assert!(
            !args.contains(&"-vn".to_string()),
            "-vn should NOT be present for audio compression"
        );
        assert!(args.contains(&"-b:a".to_string()));
        assert!(args.contains(&"192k".to_string()));
    }

    #[test]
    fn audio_compression_flac_no_bitrate() {
        let opts = AudioExtractionOptions {
            format: AudioOutputFormat::Flac,
            bitrate: Some("192k".into()),
            sample_rate: None,
        };
        let args = build_audio_compression_args("in.flac", "out.flac", &opts);
        assert!(args.contains(&"flac".to_string()));
        assert!(
            !args.contains(&"-b:a".to_string()),
            "bitrate should be ignored for FLAC"
        );
    }

    #[test]
    fn audio_compression_sample_rate() {
        let opts = AudioExtractionOptions {
            format: AudioOutputFormat::Opus,
            bitrate: Some("128k".into()),
            sample_rate: Some(48000),
        };
        let args = build_audio_compression_args("in.ogg", "out.ogg", &opts);
        assert!(args.contains(&"-ar".to_string()));
        assert!(args.contains(&"48000".to_string()));
        assert!(args.contains(&"libopus".to_string()));
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
