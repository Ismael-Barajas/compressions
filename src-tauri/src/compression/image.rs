use image::imageops::FilterType;
use image::DynamicImage;

use crate::types::{ImageFormat, ImageOptions};

pub fn compress(input: &str, output: &str, options: &ImageOptions) -> Result<(), String> {
    let img = image::open(input).map_err(|e| format!("Failed to open image: {}", e))?;

    // Resize if requested
    let img = if let Some(ref resize) = options.resize {
        img.resize(resize.width, resize.height, FilterType::Lanczos3)
    } else {
        img
    };

    match options.format {
        ImageFormat::Jpeg => encode_jpeg(&img, output, options.quality),
        ImageFormat::Png => encode_png(&img, output),
        ImageFormat::WebP => encode_webp(&img, output, options.quality),
        ImageFormat::Avif => encode_avif(&img, output, options.quality),
    }
}

fn encode_jpeg(img: &DynamicImage, output: &str, quality: u8) -> Result<(), String> {
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

    std::fs::write(output, data).map_err(|e| format!("Failed to write JPEG: {}", e))
}

fn encode_png(img: &DynamicImage, output: &str) -> Result<(), String> {
    // First save as unoptimized PNG, then run oxipng
    let mut buf = Vec::new();
    let mut cursor = std::io::Cursor::new(&mut buf);
    img.write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    // Optimize with oxipng
    let optimized = oxipng::optimize_from_memory(&buf, &oxipng::Options::from_preset(3))
        .map_err(|e| format!("Failed to optimize PNG: {}", e))?;

    std::fs::write(output, optimized).map_err(|e| format!("Failed to write PNG: {}", e))
}

fn encode_webp(img: &DynamicImage, output: &str, quality: u8) -> Result<(), String> {
    let encoder = webp::Encoder::from_image(img)
        .map_err(|e| format!("Failed to create WebP encoder: {}", e))?;
    let encoded = encoder.encode(quality as f32);
    std::fs::write(output, &*encoded).map_err(|e| format!("Failed to write WebP: {}", e))
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
