use crate::types::*;

pub fn builtin_presets() -> Vec<Preset> {
    vec![
        // Video presets
        Preset {
            id: "video-web".into(),
            name: "Web Optimized".into(),
            description: "H.264, CRF 28, 720p max, AAC 128k".into(),
            is_builtin: true,
            media_type: MediaType::Video,
            video_options: Some(VideoOptions {
                codec: VideoCodec::H264,
                crf: 28,
                resolution: Some(Resolution {
                    width: 1280,
                    height: 720,
                }),
                bitrate: None,
                framerate: None,
                audio_codec: AudioCodec::AAC,
                audio_bitrate: Some("128k".into()),
            }),
            image_options: None,
        },
        Preset {
            id: "video-high".into(),
            name: "High Quality".into(),
            description: "H.265, CRF 20, original resolution, AAC 192k".into(),
            is_builtin: true,
            media_type: MediaType::Video,
            video_options: Some(VideoOptions {
                codec: VideoCodec::H265,
                crf: 20,
                resolution: None,
                bitrate: None,
                framerate: None,
                audio_codec: AudioCodec::AAC,
                audio_bitrate: Some("192k".into()),
            }),
            image_options: None,
        },
        Preset {
            id: "video-small".into(),
            name: "Small File Size".into(),
            description: "H.265, CRF 32, 480p max, AAC 96k".into(),
            is_builtin: true,
            media_type: MediaType::Video,
            video_options: Some(VideoOptions {
                codec: VideoCodec::H265,
                crf: 32,
                resolution: Some(Resolution {
                    width: 854,
                    height: 480,
                }),
                bitrate: None,
                framerate: None,
                audio_codec: AudioCodec::AAC,
                audio_bitrate: Some("96k".into()),
            }),
            image_options: None,
        },
        Preset {
            id: "video-social".into(),
            name: "Social Media".into(),
            description: "H.264, CRF 23, 1080p max, AAC 128k".into(),
            is_builtin: true,
            media_type: MediaType::Video,
            video_options: Some(VideoOptions {
                codec: VideoCodec::H264,
                crf: 23,
                resolution: Some(Resolution {
                    width: 1920,
                    height: 1080,
                }),
                bitrate: None,
                framerate: None,
                audio_codec: AudioCodec::AAC,
                audio_bitrate: Some("128k".into()),
            }),
            image_options: None,
        },
        // Image presets
        Preset {
            id: "image-web".into(),
            name: "Web Optimized".into(),
            description: "WebP, quality 80".into(),
            is_builtin: true,
            media_type: MediaType::Image,
            video_options: None,
            image_options: Some(ImageOptions {
                format: ImageFormat::WebP,
                quality: 80,
                resize: None,
                strip_metadata: true,
            }),
        },
        Preset {
            id: "image-high".into(),
            name: "High Quality".into(),
            description: "PNG lossless optimization".into(),
            is_builtin: true,
            media_type: MediaType::Image,
            video_options: None,
            image_options: Some(ImageOptions {
                format: ImageFormat::Png,
                quality: 100,
                resize: None,
                strip_metadata: false,
            }),
        },
        Preset {
            id: "image-small".into(),
            name: "Small File Size".into(),
            description: "AVIF, quality 60".into(),
            is_builtin: true,
            media_type: MediaType::Image,
            video_options: None,
            image_options: Some(ImageOptions {
                format: ImageFormat::Avif,
                quality: 60,
                resize: None,
                strip_metadata: true,
            }),
        },
        Preset {
            id: "image-thumb".into(),
            name: "Thumbnail".into(),
            description: "JPEG, quality 70, 300px max".into(),
            is_builtin: true,
            media_type: MediaType::Image,
            video_options: None,
            image_options: Some(ImageOptions {
                format: ImageFormat::Jpeg,
                quality: 70,
                resize: Some(Resolution {
                    width: 300,
                    height: 300,
                }),
                strip_metadata: true,
            }),
        },
    ]
}
