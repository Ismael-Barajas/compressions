use std::borrow::Cow;
use std::path::Path;

use fast_image_resize::{FilterType as FrFilter, ResizeAlg, ResizeOptions, Resizer};
use image::DynamicImage;
use rgb::FromSlice;

use crate::types::{ImageFormat, ImageOptions, ResizeMode};

pub fn compress(input: &str, output: &str, options: &ImageOptions) -> Result<(), String> {
    let threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    compress_with_threads(input, output, options, threads)
}

/// Like [`compress`], with an explicit thread budget for encoders that parallelize
/// internally (AVIF).
pub fn compress_with_threads(
    input: &str,
    output: &str,
    options: &ImageOptions,
    threads: usize,
) -> Result<(), String> {
    // GIF inputs always re-encode as animated GIF regardless of the selected format
    let input_ext = Path::new(input)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase())
        .unwrap_or_default();

    if input_ext == "gif" {
        return encode_gif(input, output, options.quality);
    }

    let img = image::open(input).map_err(|e| format!("Failed to open image: {}", e))?;

    // Resize if requested
    let img = match options.resize {
        Some(ref resize) => resize_image(img, resize.width, resize.height, &options.resize_mode)?,
        None => img,
    };

    let preserve = !options.strip_metadata;

    // Resolve Original → concrete format (defensive; commands layer resolves first)
    let effective_format = options.format.resolve_for_input(input);

    match effective_format {
        ImageFormat::Jpeg => encode_jpeg(&img, input, output, options.quality, preserve),
        ImageFormat::Png => encode_png(&img, output, preserve),
        ImageFormat::WebP => encode_webp(&img, input, output, options.quality, preserve),
        ImageFormat::Avif => encode_avif(&img, output, options.quality, threads),
        ImageFormat::Gif => encode_gif(input, output, options.quality),
        ImageFormat::Heic => Err(
            "HEIC output encoding is not supported by the native encoder; use FFmpeg pipeline"
                .to_string(),
        ),
        ImageFormat::Original => unreachable!("resolve_for_input always returns concrete format"),
    }
}

/// Target dimensions for a resize request. `Fit` treats a zero dimension as
/// unconstrained and preserves aspect ratio; `Exact` stretches.
pub fn target_dimensions(
    src_w: u32,
    src_h: u32,
    req_w: u32,
    req_h: u32,
    mode: &ResizeMode,
) -> (u32, u32) {
    match mode {
        ResizeMode::Exact => (req_w.max(1), req_h.max(1)),
        ResizeMode::Fit => {
            if req_w == 0 && req_h == 0 {
                return (src_w, src_h);
            }
            let max_w = if req_w > 0 { req_w } else { u32::MAX };
            let max_h = if req_h > 0 { req_h } else { u32::MAX };
            // Same rule as image::imageops::resize_dimensions: scale to fit inside.
            let wratio = max_w as f64 / src_w as f64;
            let hratio = max_h as f64 / src_h as f64;
            let ratio = wratio.min(hratio);
            let w = (src_w as f64 * ratio).round().max(1.0) as u32;
            let h = (src_h as f64 * ratio).round().max(1.0) as u32;
            (w, h)
        }
    }
}

/// Resize with `fast_image_resize` (SIMD convolution), which is several times
/// faster than `image::imageops::resize` for the same Lanczos3 kernel. Falls back
/// to the `image` crate for exotic pixel layouts.
fn resize_image(
    img: DynamicImage,
    req_w: u32,
    req_h: u32,
    mode: &ResizeMode,
) -> Result<DynamicImage, String> {
    let (w, h) = target_dimensions(img.width(), img.height(), req_w, req_h, mode);
    if w == img.width() && h == img.height() {
        return Ok(img);
    }

    // Normalize to an 8-bit layout fast_image_resize handles directly.
    let src = match img {
        DynamicImage::ImageLuma8(_)
        | DynamicImage::ImageLumaA8(_)
        | DynamicImage::ImageRgb8(_)
        | DynamicImage::ImageRgba8(_) => img,
        other if other.color().has_alpha() => DynamicImage::ImageRgba8(other.to_rgba8()),
        other => DynamicImage::ImageRgb8(other.to_rgb8()),
    };
    let mut dst = DynamicImage::new(w, h, src.color());

    let opts = ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FrFilter::Lanczos3));
    Resizer::new()
        .resize(&src, &mut dst, &opts)
        .map_err(|e| format!("Failed to resize image: {}", e))?;
    Ok(dst)
}

fn encode_jpeg(
    img: &DynamicImage,
    input: &str,
    output: &str,
    quality: u8,
    preserve_metadata: bool,
) -> Result<(), String> {
    // Borrow the buffer when the decoded image is already RGB8 (the common JPEG case).
    let rgb: Cow<image::RgbImage> = match img {
        DynamicImage::ImageRgb8(buf) => Cow::Borrowed(buf),
        other => Cow::Owned(other.to_rgb8()),
    };
    let (width, height) = rgb.dimensions();

    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(width as usize, height as usize);
    comp.set_quality(quality as f32);

    let mut comp = comp
        .start_compress(Vec::new())
        .map_err(|e| format!("Failed to start JPEG compress: {}", e))?;

    comp.write_scanlines(rgb.as_raw())
        .map_err(|e| format!("Failed to write scanlines: {}", e))?;

    let data = comp
        .finish()
        .map_err(|_| "Failed to get JPEG data".to_string())?;

    if preserve_metadata {
        if let Some(out_bytes) = copy_exif_jpeg(input, &data) {
            return std::fs::write(output, out_bytes)
                .map_err(|e| format!("Failed to write JPEG: {}", e));
        }
    }

    std::fs::write(output, data).map_err(|e| format!("Failed to write JPEG: {}", e))
}

/// Copy the EXIF segment from `input` into the freshly encoded JPEG `data`.
/// Returns `None` when either side cannot be parsed or the input has no EXIF.
fn copy_exif_jpeg(input: &str, data: &[u8]) -> Option<Vec<u8>> {
    use img_parts::ImageEXIF;
    let input_bytes = std::fs::read(input).ok()?;
    let src = img_parts::jpeg::Jpeg::from_bytes(input_bytes.into()).ok()?;
    let exif = src.exif()?;
    let mut dst = img_parts::jpeg::Jpeg::from_bytes(data.to_vec().into()).ok()?;
    dst.set_exif(Some(exif));
    Some(dst.encoder().bytes().to_vec())
}

/// Encode PNG by handing raw pixels straight to oxipng. The previous pipeline
/// wrote a PNG with the `image` crate, then had oxipng decode and re-encode it;
/// this skips that round trip and keeps grayscale/RGB inputs in their native
/// layout instead of inflating everything to RGBA first.
fn encode_png(img: &DynamicImage, output: &str, preserve_metadata: bool) -> Result<(), String> {
    use oxipng::{BitDepth, ColorType, RawImage};

    let (width, height) = (img.width(), img.height());
    let (color_type, data): (ColorType, Vec<u8>) = match img {
        DynamicImage::ImageLuma8(b) => (
            ColorType::Grayscale {
                transparent_shade: None,
            },
            b.as_raw().clone(),
        ),
        DynamicImage::ImageLumaA8(b) => (ColorType::GrayscaleAlpha, b.as_raw().clone()),
        DynamicImage::ImageRgb8(b) => (
            ColorType::RGB {
                transparent_color: None,
            },
            b.as_raw().clone(),
        ),
        DynamicImage::ImageRgba8(b) => (ColorType::RGBA, b.as_raw().clone()),
        other if other.color().has_alpha() => (ColorType::RGBA, other.to_rgba8().into_raw()),
        other => (
            ColorType::RGB {
                transparent_color: None,
            },
            other.to_rgb8().into_raw(),
        ),
    };

    let raw = RawImage::new(width, height, color_type, BitDepth::Eight, data)
        .map_err(|e| format!("Failed to build PNG image: {}", e))?;

    let mut opts = oxipng::Options::from_preset(2);
    opts.strip = if preserve_metadata {
        oxipng::StripChunks::None
    } else {
        oxipng::StripChunks::All
    };

    let optimized = raw
        .create_optimized_png(&opts)
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
    // The webp crate only accepts RGB8/RGBA8; grayscale inputs used to fail here.
    let rgb_img: Cow<DynamicImage> = match img {
        DynamicImage::ImageRgb8(_) | DynamicImage::ImageRgba8(_) => Cow::Borrowed(img),
        other if other.color().has_alpha() => {
            Cow::Owned(DynamicImage::ImageRgba8(other.to_rgba8()))
        }
        other => Cow::Owned(DynamicImage::ImageRgb8(other.to_rgb8())),
    };
    let encoder = webp::Encoder::from_image(&rgb_img)
        .map_err(|e| format!("Failed to create WebP encoder: {}", e))?;
    let encoded = encoder.encode(quality as f32);

    if preserve_metadata {
        if let Some(out_bytes) = copy_exif_webp(input, &encoded) {
            return std::fs::write(output, out_bytes)
                .map_err(|e| format!("Failed to write WebP: {}", e));
        }
    }

    std::fs::write(output, &*encoded).map_err(|e| format!("Failed to write WebP: {}", e))
}

fn copy_exif_webp(input: &str, data: &[u8]) -> Option<Vec<u8>> {
    use img_parts::ImageEXIF;
    let input_bytes = std::fs::read(input).ok()?;
    let src = img_parts::webp::WebP::from_bytes(input_bytes.into()).ok()?;
    let exif = src.exif()?;
    let mut dst = img_parts::webp::WebP::from_bytes(data.to_vec().into()).ok()?;
    dst.set_exif(Some(exif));
    Some(dst.encoder().bytes().to_vec())
}

fn encode_avif(
    img: &DynamicImage,
    output: &str,
    quality: u8,
    threads: usize,
) -> Result<(), String> {
    let encoder = ravif::Encoder::new()
        .with_quality(quality as f32)
        .with_speed(6)
        .with_num_threads(Some(threads.max(1)));

    let (width, height) = (img.width() as usize, img.height() as usize);

    // Zero-copy casts from the image buffer; opaque images take the RGB path,
    // which skips the alpha plane entirely.
    let res = if img.color().has_alpha() {
        let rgba: Cow<image::RgbaImage> = match img {
            DynamicImage::ImageRgba8(b) => Cow::Borrowed(b),
            other => Cow::Owned(other.to_rgba8()),
        };
        encoder
            .encode_rgba(ravif::Img::new(rgba.as_raw().as_rgba(), width, height))
            .map_err(|e| format!("Failed to encode AVIF: {}", e))?
    } else {
        let rgb: Cow<image::RgbImage> = match img {
            DynamicImage::ImageRgb8(b) => Cow::Borrowed(b),
            other => Cow::Owned(other.to_rgb8()),
        };
        encoder
            .encode_rgb(ravif::Img::new(rgb.as_raw().as_rgb(), width, height))
            .map_err(|e| format!("Failed to encode AVIF: {}", e))?
    };

    std::fs::write(output, res.avif_file).map_err(|e| format!("Failed to write AVIF: {}", e))
}

/// Dithering strength for re-quantized GIF frames. 1.0 (the maximum) is slowest
/// and noisiest; 0.5 is the usual sweet spot for animation.
const GIF_DITHERING_LEVEL: f32 = 0.5;

/// Re-quantize an animated GIF frame by frame. Frames are streamed: each one is
/// decoded, quantized, and written before the next is read, so memory stays at
/// one frame instead of the whole animation.
fn encode_gif(input: &str, output: &str, quality: u8) -> Result<(), String> {
    use gif::{DecodeOptions, Encoder, Frame, Repeat};
    use std::fs::File;
    use std::io::BufWriter;

    let in_file = File::open(input).map_err(|e| format!("Failed to open GIF input: {}", e))?;

    // Decode to RGBA so we can re-quantize with imagequant
    let mut decode_opts = DecodeOptions::new();
    decode_opts.set_color_output(gif::ColorOutput::RGBA);

    let mut decoder = decode_opts
        .read_info(in_file)
        .map_err(|e| format!("Failed to read GIF info: {}", e))?;

    let width = decoder.width();
    let height = decoder.height();

    let out_file =
        File::create(output).map_err(|e| format!("Failed to create GIF output: {}", e))?;

    // Empty global palette — each frame gets its own local palette via imagequant
    let mut encoder = Encoder::new(BufWriter::new(out_file), width, height, &[])
        .map_err(|e| format!("Failed to create GIF encoder: {}", e))?;

    encoder
        .set_repeat(Repeat::Infinite)
        .map_err(|e| format!("Failed to set GIF repeat: {}", e))?;

    let mut liq = imagequant::new();
    liq.set_quality(0, quality)
        .map_err(|e| format!("imagequant quality error: {}", e))?;

    let mut frame_count = 0usize;
    while let Some(raw) = decoder
        .read_next_frame()
        .map_err(|e| format!("Failed to read GIF frame: {}", e))?
    {
        frame_count += 1;
        let fw = raw.width as usize;
        let fh = raw.height as usize;
        let pixel_count = fw * fh;

        // Zero-copy view of the RGBA bytes as imagequant pixels
        let pixels: &[imagequant::RGBA] = raw.buffer.as_rgba();
        if pixels.len() != pixel_count {
            // Skip malformed frames rather than aborting the whole file
            continue;
        }

        let mut img = liq
            .new_image_borrowed(pixels, fw, fh, 0.0)
            .map_err(|e| format!("imagequant image error: {}", e))?;

        let mut res = liq
            .quantize(&mut img)
            .map_err(|e| format!("imagequant quantize error: {}", e))?;

        res.set_dithering_level(GIF_DITHERING_LEVEL)
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

        let frame = Frame {
            width: raw.width,
            height: raw.height,
            left: raw.left,
            top: raw.top,
            delay: raw.delay,
            dispose: raw.dispose,
            palette: Some(palette_bytes),
            buffer: Cow::Owned(indexed_pixels),
            transparent: transparent_idx,
            ..Default::default()
        };

        encoder
            .write_frame(&frame)
            .map_err(|e| format!("Failed to write GIF frame: {}", e))?;
    }

    if frame_count == 0 {
        return Err("GIF has no frames".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ResizeMode, Resolution};
    use image::GenericImageView;

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
            resize_mode: ResizeMode::Fit,
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
        assert!(!data.is_empty());
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
        assert!(!data.is_empty());
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
            resize_mode: ResizeMode::Fit,
            strip_metadata: true,
        };

        compress(input.to_str().unwrap(), output.to_str().unwrap(), &opts).unwrap();

        let dims = image::image_dimensions(&output).unwrap();
        // 200x100 resized to fit 50x50 → 50x25 (aspect preserved)
        assert!(dims.0 <= 50);
        assert!(dims.1 <= 50);
    }

    #[test]
    fn resize_fit_single_dimension() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("input.png");
        let output = dir.path().join("output.jpg");
        create_test_image(input.to_str().unwrap(), 200, 100);

        let opts = ImageOptions {
            format: ImageFormat::Jpeg,
            quality: 80,
            resize: Some(Resolution {
                width: 80,
                height: 0,
            }),
            resize_mode: ResizeMode::Fit,
            strip_metadata: true,
        };

        compress(input.to_str().unwrap(), output.to_str().unwrap(), &opts).unwrap();

        let dims = image::image_dimensions(&output).unwrap();
        // 200x100 scaled to width 80 → 80x40
        assert_eq!(dims.0, 80);
        assert_eq!(dims.1, 40);
    }

    #[test]
    fn resize_exact_forces_dimensions() {
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
            resize_mode: ResizeMode::Exact,
            strip_metadata: true,
        };

        compress(input.to_str().unwrap(), output.to_str().unwrap(), &opts).unwrap();

        let dims = image::image_dimensions(&output).unwrap();
        // Exact mode: should be exactly 50x50 regardless of aspect ratio
        assert_eq!(dims.0, 50);
        assert_eq!(dims.1, 50);
    }

    #[test]
    fn grayscale_png_and_webp_encode() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("gray.png");
        let gray = DynamicImage::ImageLuma8(image::GrayImage::from_fn(40, 30, |x, y| {
            image::Luma([((x + y) % 256) as u8])
        }));
        gray.save(&input).unwrap();

        let out_png = dir.path().join("out.png");
        compress(
            input.to_str().unwrap(),
            out_png.to_str().unwrap(),
            &default_opts(ImageFormat::Png),
        )
        .unwrap();
        let decoded = image::open(&out_png).unwrap();
        assert_eq!(decoded.dimensions(), (40, 30));
        // Native grayscale stays grayscale on disk (no RGBA inflation)
        assert!(matches!(decoded.color(), image::ColorType::L8));

        let out_webp = dir.path().join("out.webp");
        compress(
            input.to_str().unwrap(),
            out_webp.to_str().unwrap(),
            &default_opts(ImageFormat::WebP),
        )
        .unwrap();
        assert_eq!(&std::fs::read(&out_webp).unwrap()[0..4], b"RIFF");

        let out_avif = dir.path().join("out.avif");
        compress(
            input.to_str().unwrap(),
            out_avif.to_str().unwrap(),
            &default_opts(ImageFormat::Avif),
        )
        .unwrap();
        assert!(!std::fs::read(&out_avif).unwrap().is_empty());
    }

    #[test]
    fn png_round_trips_pixels() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("in.png");
        let output = dir.path().join("out.png");
        create_test_image(input.to_str().unwrap(), 33, 17);
        compress(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::Png),
        )
        .unwrap();
        let a = image::open(&input).unwrap().to_rgba8();
        let b = image::open(&output).unwrap().to_rgba8();
        assert_eq!(a.as_raw(), b.as_raw());
    }

    #[test]
    fn target_dimensions_fit_and_exact() {
        assert_eq!(
            target_dimensions(200, 100, 50, 50, &ResizeMode::Fit),
            (50, 25)
        );
        assert_eq!(
            target_dimensions(200, 100, 80, 0, &ResizeMode::Fit),
            (80, 40)
        );
        assert_eq!(
            target_dimensions(200, 100, 0, 50, &ResizeMode::Fit),
            (100, 50)
        );
        assert_eq!(
            target_dimensions(200, 100, 50, 50, &ResizeMode::Exact),
            (50, 50)
        );
        assert_eq!(target_dimensions(10, 10, 0, 0, &ResizeMode::Fit), (10, 10));
    }

    #[test]
    fn animated_gif_requantize_keeps_frames() {
        use gif::{Encoder, Frame, Repeat};
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("anim.gif");
        {
            let file = std::fs::File::create(&input).unwrap();
            let mut palette = Vec::new();
            for i in 0..256u16 {
                palette.extend_from_slice(&[i as u8, (255 - i) as u8, 128]);
            }
            let mut enc = Encoder::new(file, 16, 16, &palette).unwrap();
            enc.set_repeat(Repeat::Infinite).unwrap();
            for f in 0..3u8 {
                let frame = Frame {
                    width: 16,
                    height: 16,
                    delay: 7,
                    buffer: std::borrow::Cow::Owned(
                        (0..256).map(|i| (i as u8).wrapping_add(f * 40)).collect(),
                    ),
                    ..Default::default()
                };
                enc.write_frame(&frame).unwrap();
            }
        }
        let output = dir.path().join("out.gif");
        compress(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::Gif),
        )
        .unwrap();

        let mut opts = gif::DecodeOptions::new();
        opts.set_color_output(gif::ColorOutput::RGBA);
        let mut dec = opts
            .read_info(std::fs::File::open(&output).unwrap())
            .unwrap();
        let mut frames = 0;
        while let Some(fr) = dec.read_next_frame().unwrap() {
            assert_eq!(fr.delay, 7);
            frames += 1;
        }
        assert_eq!(frames, 3);
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
                resize_mode: ResizeMode::Fit,
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
