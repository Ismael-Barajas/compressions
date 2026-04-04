use std::path::Path;

use image::imageops::FilterType;
use image::DynamicImage;

use crate::types::{ImageFormat, ImageOptions};

pub fn compress(input: &str, output: &str, options: &ImageOptions) -> Result<(), String> {
    // GIF inputs always re-encode as animated GIF regardless of the selected format
    let input_ext = Path::new(input)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if input_ext == "gif" {
        return encode_gif(input, output);
    }

    let img = image::open(input).map_err(|e| format!("Failed to open image: {}", e))?;

    // Resize if requested
    let img = if let Some(ref resize) = options.resize {
        img.resize(resize.width, resize.height, FilterType::Lanczos3)
    } else {
        img
    };

    let preserve = !options.strip_metadata;

    match options.format {
        ImageFormat::Jpeg => encode_jpeg(&img, input, output, options.quality, preserve),
        ImageFormat::Png => encode_png(&img, output, preserve),
        ImageFormat::WebP => encode_webp(&img, input, output, options.quality, preserve),
        ImageFormat::Avif => encode_avif(&img, output, options.quality),
        ImageFormat::Gif => encode_gif(input, output),
    }
}

fn encode_jpeg(
    img: &DynamicImage,
    input: &str,
    output: &str,
    quality: u8,
    preserve_metadata: bool,
) -> Result<(), String> {
    let rgb = img.to_rgb8();
    let (width, height) = rgb.dimensions();

    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width as usize, height as usize);
    comp.set_quality(quality as f32);

    let mut comp = comp
        .start_compress(Vec::new())
        .map_err(|e| format!("Failed to start JPEG compress: {}", e))?;

    let pixels = rgb.as_raw();
    let row_stride = width as usize * 3;
    for row in pixels.chunks(row_stride) {
        comp.write_scanlines(row)
            .map_err(|e| format!("Failed to write scanlines: {}", e))?;
    }

    let data = comp
        .finish()
        .map_err(|_| "Failed to get JPEG data".to_string())?;

    if preserve_metadata {
        let input_bytes =
            std::fs::read(input).map_err(|e| format!("Failed to read input for metadata: {}", e))?;

        // Try to copy EXIF (APP1) from input into the encoded output
        if let (Ok(src), Ok(mut dst)) = (
            img_parts::jpeg::Jpeg::from_bytes(input_bytes.into()),
            img_parts::jpeg::Jpeg::from_bytes(data.clone().into()),
        ) {
            use img_parts::ImageEXIF;
            if let Some(exif) = src.exif() {
                dst.set_exif(Some(exif));
            }
            let out_bytes = dst.encoder().bytes();
            return std::fs::write(output, out_bytes)
                .map_err(|e| format!("Failed to write JPEG: {}", e));
        }
    }

    std::fs::write(output, data).map_err(|e| format!("Failed to write JPEG: {}", e))
}

fn encode_png(img: &DynamicImage, output: &str, preserve_metadata: bool) -> Result<(), String> {
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    let mut opts = oxipng::Options::from_preset(3);
    opts.strip = if preserve_metadata {
        oxipng::StripChunks::None
    } else {
        oxipng::StripChunks::All
    };

    let optimized = oxipng::optimize_from_memory(&buf, &opts)
        .map_err(|e| format!("Failed to optimize PNG: {}", e))?;

    std::fs::write(output, optimized).map_err(|e| format!("Failed to write PNG: {}", e))
}

fn encode_webp(
    img: &DynamicImage,
    input: &str,
    output: &str,
    quality: u8,
    preserve_metadata: bool,
) -> Result<(), String> {
    let encoder = webp::Encoder::from_image(img)
        .map_err(|e| format!("Failed to create WebP encoder: {}", e))?;
    let encoded = encoder.encode(quality as f32);
    let data: Vec<u8> = encoded.to_vec();

    if preserve_metadata {
        let input_bytes =
            std::fs::read(input).map_err(|e| format!("Failed to read input for metadata: {}", e))?;

        if let (Ok(src), Ok(mut dst)) = (
            img_parts::webp::WebP::from_bytes(input_bytes.into()),
            img_parts::webp::WebP::from_bytes(data.clone().into()),
        ) {
            use img_parts::ImageEXIF;
            if let Some(exif) = src.exif() {
                dst.set_exif(Some(exif));
            }
            let out_bytes = dst.encoder().bytes();
            return std::fs::write(output, out_bytes)
                .map_err(|e| format!("Failed to write WebP: {}", e));
        }
    }

    std::fs::write(output, data).map_err(|e| format!("Failed to write WebP: {}", e))
}

fn encode_avif(img: &DynamicImage, output: &str, quality: u8) -> Result<(), String> {
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();

    let pixels: Vec<rgb::RGBA8> = rgba
        .pixels()
        .map(|p| rgb::RGBA8 {
            r: p[0],
            g: p[1],
            b: p[2],
            a: p[3],
        })
        .collect();

    let img_ref = ravif::Img::new(&pixels[..], width as usize, height as usize);

    let res = ravif::Encoder::new()
        .with_quality(quality as f32)
        .with_speed(6)
        .encode_rgba(img_ref)
        .map_err(|e| format!("Failed to encode AVIF: {}", e))?;

    std::fs::write(output, res.avif_file).map_err(|e| format!("Failed to write AVIF: {}", e))
}

fn encode_gif(input: &str, output: &str) -> Result<(), String> {
    use gif::{DecodeOptions, Encoder, Repeat};
    use std::fs::File;

    let in_file =
        File::open(input).map_err(|e| format!("Failed to open GIF input: {}", e))?;

    let mut decode_opts = DecodeOptions::new();
    decode_opts.set_color_output(gif::ColorOutput::Indexed);

    let mut decoder = decode_opts
        .read_info(in_file)
        .map_err(|e| format!("Failed to read GIF info: {}", e))?;

    let width = decoder.width();
    let height = decoder.height();
    let global_palette = decoder.global_palette().map(|p| p.to_vec());

    // Collect all frames first
    let mut frames: Vec<gif::Frame<'static>> = Vec::new();
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|e| format!("Failed to read GIF frame: {}", e))?
    {
        frames.push(frame.clone().to_owned());
    }

    if frames.is_empty() {
        return Err("GIF has no frames".to_string());
    }

    let out_file =
        File::create(output).map_err(|e| format!("Failed to create GIF output: {}", e))?;

    let palette = global_palette.as_deref().unwrap_or(&[]);
    let mut encoder = Encoder::new(out_file, width, height, palette)
        .map_err(|e| format!("Failed to create GIF encoder: {}", e))?;

    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| format!("Failed to set GIF repeat: {}", e))?;

    for frame in &frames {
        encoder
            .write_frame(frame)
            .map_err(|e| format!("Failed to write GIF frame: {}", e))?;
    }

    Ok(())
}
