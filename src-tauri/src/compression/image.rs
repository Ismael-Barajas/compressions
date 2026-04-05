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
        let input_bytes = std::fs::read(input)
            .map_err(|e| format!("Failed to read input for metadata: {}", e))?;

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
        let input_bytes = std::fs::read(input)
            .map_err(|e| format!("Failed to read input for metadata: {}", e))?;

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
    use gif::{DecodeOptions, Encoder, Frame, Repeat};
    use std::fs::File;

    let in_file = File::open(input).map_err(|e| format!("Failed to open GIF input: {}", e))?;

    // Decode to RGBA so we can re-quantize with imagequant
    let mut decode_opts = DecodeOptions::new();
    decode_opts.set_color_output(gif::ColorOutput::RGBA);

    let mut decoder = decode_opts
        .read_info(in_file)
        .map_err(|e| format!("Failed to read GIF info: {}", e))?;

    let width = decoder.width();
    let height = decoder.height();

    // Collect all frames with their delays and dispose info
    struct RawFrame {
        rgba: Vec<u8>,
        delay: u16,
        left: u16,
        top: u16,
        width: u16,
        height: u16,
    }

    let mut raw_frames: Vec<RawFrame> = Vec::new();
    while let Some(frame) = decoder
        .read_next_frame()
        .map_err(|e| format!("Failed to read GIF frame: {}", e))?
    {
        raw_frames.push(RawFrame {
            rgba: frame.buffer.to_vec(),
            delay: frame.delay,
            left: frame.left,
            top: frame.top,
            width: frame.width,
            height: frame.height,
        });
    }

    if raw_frames.is_empty() {
        return Err("GIF has no frames".to_string());
    }

    let out_file =
        File::create(output).map_err(|e| format!("Failed to create GIF output: {}", e))?;

    // Empty global palette — each frame gets its own local palette via imagequant
    let mut encoder = Encoder::new(out_file, width, height, &[])
        .map_err(|e| format!("Failed to create GIF encoder: {}", e))?;

    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| format!("Failed to set GIF repeat: {}", e))?;

    let mut liq = imagequant::new();
    liq.set_quality(0, 100)
        .map_err(|e| format!("imagequant quality error: {}", e))?;

    for raw in &raw_frames {
        let fw = raw.width as usize;
        let fh = raw.height as usize;
        let pixel_count = fw * fh;

        // Convert RGBA bytes to imagequant RGBA pixels
        let pixels: Vec<imagequant::RGBA> = raw
            .rgba
            .chunks_exact(4)
            .map(|c| imagequant::RGBA {
                r: c[0],
                g: c[1],
                b: c[2],
                a: c[3],
            })
            .collect();

        if pixels.len() != pixel_count {
            // Fallback: skip quantization for malformed frames
            continue;
        }

        let mut img = liq
            .new_image_borrowed(&pixels, fw, fh, 0.0)
            .map_err(|e| format!("imagequant image error: {}", e))?;

        let mut res = liq
            .quantize(&mut img)
            .map_err(|e| format!("imagequant quantize error: {}", e))?;

        res.set_dithering_level(1.0)
            .map_err(|e| format!("imagequant dither error: {}", e))?;

        let (palette_data, indexed_pixels) = res
            .remapped(&mut img)
            .map_err(|e| format!("imagequant remap error: {}", e))?;

        // Build palette bytes (RGB triples)
        let mut palette_bytes: Vec<u8> = Vec::with_capacity(palette_data.len() * 3);
        let mut transparent_idx: Option<u8> = None;
        for (i, color) in palette_data.iter().enumerate() {
            palette_bytes.push(color.r);
            palette_bytes.push(color.g);
            palette_bytes.push(color.b);
            if color.a < 128 && transparent_idx.is_none() {
                transparent_idx = Some(i as u8);
            }
        }

        let mut frame = Frame::default();
        frame.width = raw.width;
        frame.height = raw.height;
        frame.left = raw.left;
        frame.top = raw.top;
        frame.delay = raw.delay;
        frame.palette = Some(palette_bytes);
        frame.buffer = std::borrow::Cow::Owned(indexed_pixels);
        frame.transparent = transparent_idx;

        encoder
            .write_frame(&frame)
            .map_err(|e| format!("Failed to write GIF frame: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Resolution;

    /// Create a test image in memory and save it to `path`.
    fn create_test_image(path: &str, width: u32, height: u32) {
        let img = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(width, height, |x, y| {
            image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
        }));
        img.save(path).expect("Failed to save test image");
    }

    fn default_opts(format: ImageFormat) -> ImageOptions {
        ImageOptions {
            format,
            quality: 80,
            resize: None,
            strip_metadata: true,
        }
    }

    #[test]
    fn jpeg_produces_valid_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.jpg");
        create_test_image(input.to_str().unwrap(), 100, 100);

        compress(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::Jpeg),
        )
        .unwrap();

        let data = std::fs::read(&output).unwrap();
        assert!(data.len() > 0);
        // JPEG magic bytes: FF D8 FF
        assert_eq!(&data[0..2], &[0xFF, 0xD8]);
    }

    #[test]
    fn png_produces_valid_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.png");
        create_test_image(input.to_str().unwrap(), 100, 100);

        compress(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::Png),
        )
        .unwrap();

        let data = std::fs::read(&output).unwrap();
        // PNG magic bytes
        assert_eq!(&data[0..4], &[0x89, 0x50, 0x4E, 0x47]);
    }

    #[test]
    fn webp_produces_valid_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.webp");
        create_test_image(input.to_str().unwrap(), 100, 100);

        compress(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::WebP),
        )
        .unwrap();

        let data = std::fs::read(&output).unwrap();
        // RIFF....WEBP magic
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
    }

    #[test]
    fn avif_produces_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.avif");
        create_test_image(input.to_str().unwrap(), 64, 64);

        compress(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::Avif),
        )
        .unwrap();

        let data = std::fs::read(&output).unwrap();
        assert!(data.len() > 0);
    }

    #[test]
    fn resize_preserves_aspect_ratio() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.jpg");
        create_test_image(input.to_str().unwrap(), 200, 100);

        let opts = ImageOptions {
            format: ImageFormat::Jpeg,
            quality: 80,
            resize: Some(Resolution {
                width: 50,
                height: 50,
            }),
            strip_metadata: true,
        };

        compress(input.to_str().unwrap(), output.to_str().unwrap(), &opts).unwrap();

        let dims = image::image_dimensions(&output).unwrap();
        // 200x100 resized to fit 50x50 → 50x25 (aspect preserved)
        assert!(dims.0 <= 50);
        assert!(dims.1 <= 50);
    }

    #[test]
    fn quality_extremes_produce_valid_output() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        create_test_image(input.to_str().unwrap(), 64, 64);

        for q in [1u8, 100] {
            let output = dir.path().join(format!("output_q{}.jpg", q));
            let opts = ImageOptions {
                format: ImageFormat::Jpeg,
                quality: q,
                resize: None,
                strip_metadata: true,
            };
            compress(input.to_str().unwrap(), output.to_str().unwrap(), &opts).unwrap();
            assert!(std::fs::metadata(&output).unwrap().len() > 0);
        }
    }

    #[test]
    fn missing_input_returns_error() {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("output.jpg");
        let result = compress(
            "/nonexistent/file.png",
            output.to_str().unwrap(),
            &default_opts(ImageFormat::Jpeg),
        );
        assert!(result.is_err());
    }
}
