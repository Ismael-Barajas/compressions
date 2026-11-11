use std::borrow::Cow;
use std::path::Path;

use fast_image_resize::{FilterType as FrFilter, ResizeAlg, ResizeOptions, Resizer};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder};
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

    let decoded = decode_image_bounded(input)?;
    let preserve = !options.strip_metadata;

    // Resolve Original → concrete format (defensive; commands layer resolves first)
    let effective_format = options.format.resolve_for_input(input);

    // EXIF goes across only when metadata is kept, and only JPEG and WebP outputs
    // carry it. PNG gets none: its pixels are always rotated below, so a copied
    // orientation tag would rotate it a second time.
    let carries_exif = matches!(effective_format, ImageFormat::Jpeg | ImageFormat::WebP);
    let exif_to_copy = if preserve && carries_exif {
        decoded.exif
    } else {
        None
    };

    // A copied EXIF block keeps its orientation tag and viewers rotate for us, so
    // the pixels stay as they are (as before). Otherwise rotate the pixels, or
    // the output would display sideways once the tag is gone.
    let mut img = decoded.image;
    let needs_rotation = exif_to_copy.is_none() && decoded.orientation != Orientation::NoTransforms;
    if needs_rotation {
        tracing::debug!(input = %input, orientation = ?decoded.orientation, "Applying EXIF orientation to pixels");
        img.apply_orientation(decoded.orientation);
    }

    // Resize after rotating, so a fit box means what the user sees.
    let img = match options.resize {
        Some(ref resize) => resize_image(img, resize.width, resize.height, &options.resize_mode)?,
        None => img,
    };

    // The ICC profile is colour data, not personal data: kept in both strip modes.
    let icc = decoded.icc_profile;

    match effective_format {
        ImageFormat::Jpeg => encode_jpeg(&img, output, options.quality, icc, exif_to_copy),
        ImageFormat::Png => encode_png(img, output, icc, threads),
        ImageFormat::WebP => encode_webp(&img, output, options.quality, icc, exif_to_copy),
        // ravif writes no colour profile; a wide-gamut source is read as sRGB.
        ImageFormat::Avif => encode_avif(&img, output, options.quality, threads),
        ImageFormat::Gif => encode_gif(input, output, options.quality),
        ImageFormat::Heic => Err(
            "HEIC output encoding is not supported by the native encoder; use FFmpeg pipeline"
                .to_string(),
        ),
        ImageFormat::Original => unreachable!("resolve_for_input always returns concrete format"),
    }
}

/// Largest image edge the native pipeline will decode. Together with the image
/// crate's default 512 MiB allocation cap this bounds memory for a hostile header.
pub const MAX_IMAGE_EDGE: u32 = 32_768;

/// A decoded image plus what its decoder knew about it.
pub struct DecodedImage {
    pub image: DynamicImage,
    /// Orientation from the EXIF tag (or TIFF header). Not applied to `image`.
    pub orientation: Orientation,
    pub icc_profile: Option<Vec<u8>>,
    /// Raw EXIF (TIFF structure, without the JPEG `Exif\0\0` prefix).
    pub exif: Option<Vec<u8>>,
}

/// Decode with explicit dimension limits (the crate's default only caps total
/// allocation), reading orientation, ICC profile and EXIF from the same decoder
/// so the input file is read once.
pub fn decode_image_bounded(input: &str) -> Result<DecodedImage, String> {
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_EDGE);
    limits.max_image_height = Some(MAX_IMAGE_EDGE);
    let mut reader =
        image::ImageReader::open(input).map_err(|e| format!("Failed to open image: {}", e))?;
    reader.limits(limits.clone());
    // `into_decoder` applies the limits (the edge check happens here).
    let mut decoder = reader
        .with_guessed_format()
        .map_err(|e| format!("Failed to read image header: {}", e))?
        .into_decoder()
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    // `ImageReader::decode` also reserves the output buffer against the allocation
    // cap before decoding; `into_decoder` does not, so do the same here.
    limits
        .reserve(decoder.total_bytes())
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    decoder
        .set_limits(limits)
        .map_err(|e| format!("Failed to decode image: {}", e))?;

    // Metadata is best effort: a broken profile or EXIF block must not fail the file.
    let orientation = decoder.orientation().unwrap_or_else(|e| {
        tracing::warn!(input = %input, error = %e, "Could not read image orientation");
        Orientation::NoTransforms
    });
    let icc_profile = decoder.icc_profile().unwrap_or_else(|e| {
        tracing::warn!(input = %input, error = %e, "Could not read ICC profile");
        None
    });
    let exif = decoder.exif_metadata().unwrap_or_else(|e| {
        tracing::warn!(input = %input, error = %e, "Could not read EXIF");
        None
    });

    let image = DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    Ok(DecodedImage {
        image,
        orientation,
        icc_profile,
        exif,
    })
}

/// Decode with the same limits as [`decode_image_bounded`], pixels only (no
/// orientation applied).
pub fn open_image_bounded(input: &str) -> Result<DynamicImage, String> {
    decode_image_bounded(input).map(|d| d.image)
}

/// The colour space an ICC profile describes (header bytes 16..20) must match the
/// output pixels: a CMYK or grayscale profile on RGB pixels would be misread.
fn icc_fits(profile: &[u8], gray_output: bool) -> bool {
    let expected: &[u8] = if gray_output { b"GRAY" } else { b"RGB " };
    profile.get(16..20) == Some(expected)
}

/// Keep `profile` only when it describes RGB pixels (JPEG and WebP outputs are
/// always RGB). Logs when one is dropped.
fn rgb_icc(profile: Option<Vec<u8>>) -> Option<Vec<u8>> {
    let profile = profile?;
    if icc_fits(&profile, false) {
        Some(profile)
    } else {
        tracing::debug!("Dropping ICC profile that does not describe RGB pixels");
        None
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
    output: &str,
    quality: u8,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
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

    let data = embed_jpeg_metadata(data, rgb_icc(icc), exif)?;
    std::fs::write(output, data).map_err(|e| format!("Failed to write JPEG: {}", e))
}

/// Add the ICC profile and EXIF block (either may be absent) to an encoded JPEG.
/// An error rather than a silent fallback: the caller left the pixels unrotated
/// because it expects the EXIF orientation tag to arrive.
fn embed_jpeg_metadata(
    data: Vec<u8>,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    use img_parts::{ImageEXIF, ImageICC};
    if icc.is_none() && exif.is_none() {
        return Ok(data);
    }
    let mut jpeg = img_parts::jpeg::Jpeg::from_bytes(data.into())
        .map_err(|e| format!("Failed to add JPEG metadata: {}", e))?;
    if let Some(icc) = icc {
        jpeg.set_icc_profile(Some(icc.into()));
    }
    if let Some(exif) = exif {
        jpeg.set_exif(Some(exif.into()));
    }
    Ok(jpeg.encoder().bytes().to_vec())
}

/// Encode PNG by handing raw pixels straight to oxipng. The previous pipeline
/// wrote a PNG with the `image` crate, then had oxipng decode and re-encode it;
/// this skips that round trip and keeps grayscale/RGB inputs in their native
/// layout instead of inflating everything to RGBA first. Takes the image by value
/// so its buffer moves into oxipng instead of being copied (one full frame less
/// per concurrent job).
fn encode_png(
    img: DynamicImage,
    output: &str,
    icc: Option<Vec<u8>>,
    threads: usize,
) -> Result<(), String> {
    use oxipng::{BitDepth, ColorType, RawImage};

    let (width, height) = (img.width(), img.height());
    let (color_type, data): (ColorType, Vec<u8>) = match img {
        DynamicImage::ImageLuma8(b) => (
            ColorType::Grayscale {
                transparent_shade: None,
            },
            b.into_raw(),
        ),
        DynamicImage::ImageLumaA8(b) => (ColorType::GrayscaleAlpha, b.into_raw()),
        DynamicImage::ImageRgb8(b) => (
            ColorType::RGB {
                transparent_color: None,
            },
            b.into_raw(),
        ),
        DynamicImage::ImageRgba8(b) => (ColorType::RGBA, b.into_raw()),
        other if other.color().has_alpha() => (ColorType::RGBA, other.to_rgba8().into_raw()),
        other => (
            ColorType::RGB {
                transparent_color: None,
            },
            other.to_rgb8().into_raw(),
        ),
    };

    let gray_output = matches!(
        color_type,
        ColorType::Grayscale { .. } | ColorType::GrayscaleAlpha
    );
    let mut raw = RawImage::new(width, height, color_type, BitDepth::Eight, data)
        .map_err(|e| format!("Failed to build PNG image: {}", e))?;
    match icc {
        Some(profile) if icc_fits(&profile, gray_output) => raw.add_icc_profile(&profile),
        Some(_) => tracing::debug!("Dropping ICC profile that does not match the PNG pixels"),
        None => {}
    }

    // A raw image carries no chunks except the iCCP added above, so the strip
    // setting only decides that one. `Safe` keeps display chunks (iCCP, sRGB) in
    // both strip modes, and lets oxipng swap an sRGB-equivalent profile for the
    // small sRGB chunk.
    let mut opts = oxipng::Options::from_preset(2);
    opts.strip = oxipng::StripChunks::Safe;

    // oxipng parallelizes over the global rayon pool; run it inside a pool sized to
    // this job's budget so N concurrent PNG jobs do not each use every core.
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads.max(1))
        .build()
        .map_err(|e| format!("Failed to build PNG thread pool: {}", e))?;
    let optimized = pool
        .install(|| raw.create_optimized_png(&opts))
        .map_err(|e| format!("Failed to optimize PNG: {}", e))?;

    std::fs::write(output, optimized).map_err(|e| format!("Failed to write PNG: {}", e))
}

fn encode_webp(
    img: &DynamicImage,
    output: &str,
    quality: u8,
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
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

    let icc = rgb_icc(icc);
    if icc.is_none() && exif.is_none() {
        return std::fs::write(output, &*encoded)
            .map_err(|e| format!("Failed to write WebP: {}", e));
    }
    let data = embed_webp_metadata(&encoded, icc, exif)?;
    std::fs::write(output, data).map_err(|e| format!("Failed to write WebP: {}", e))
}

/// Add the ICC profile and EXIF block (either may be absent) to an encoded WebP.
/// An error rather than a silent fallback, for the same reason as JPEG.
fn embed_webp_metadata(
    data: &[u8],
    icc: Option<Vec<u8>>,
    exif: Option<Vec<u8>>,
) -> Result<Vec<u8>, String> {
    use img_parts::webp::{WebP, CHUNK_EXIF, CHUNK_ICCP, CHUNK_VP8X};
    use img_parts::{ImageEXIF, ImageICC};

    let mut webp = WebP::from_bytes(img_parts::Bytes::copy_from_slice(data))
        .map_err(|e| format!("Failed to add WebP metadata: {}", e))?;
    if let Some(icc) = icc {
        webp.set_icc_profile(Some(icc.into()));
    }
    if let Some(exif) = exif {
        webp.set_exif(Some(exif.into()));
    }

    // img-parts 0.4 sets the VP8X flags only when it creates that header, so a
    // second chunk added afterwards, or a VP8X header libwebp already wrote for
    // an alpha image, is left unflagged. Set both bits from the chunks present.
    const ICC_FLAG: u8 = 0b0010_0000;
    const EXIF_FLAG: u8 = 0b0000_1000;
    let mut flags_to_set = 0u8;
    if webp.has_chunk(CHUNK_ICCP) {
        flags_to_set |= ICC_FLAG;
    }
    if webp.has_chunk(CHUNK_EXIF) {
        flags_to_set |= EXIF_FLAG;
    }
    let vp8x = webp
        .chunks_mut()
        .iter_mut()
        .find(|chunk| chunk.id() == CHUNK_VP8X)
        .ok_or("Failed to add WebP metadata: no VP8X header")?;
    let mut header = vp8x
        .content()
        .data()
        .map(|d| d.to_vec())
        .unwrap_or_default();
    let Some(flags) = header.first_mut() else {
        return Err("Failed to add WebP metadata: empty VP8X header".to_string());
    };
    *flags |= flags_to_set;
    *vp8x = img_parts::riff::RiffChunk::new(
        CHUNK_VP8X,
        img_parts::riff::RiffContent::Data(header.into()),
    );

    Ok(webp.encoder().bytes().to_vec())
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

    /// Minimal little-endian TIFF block with one entry: Orientation = `value`.
    fn orientation_exif(value: u8) -> Vec<u8> {
        vec![
            0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, // II, 42, IFD at 8
            0x01, 0x00, // one entry
            0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, value, 0x00, 0x00, 0x00, // 0x0112
            0x00, 0x00, 0x00, 0x00, // no next IFD
        ]
    }

    /// Stand-in RGB ICC profile: only the header fields the code looks at are real.
    fn fake_rgb_icc() -> Vec<u8> {
        let mut p = vec![0u8; 200];
        p[0..4].copy_from_slice(&200u32.to_be_bytes());
        p[12..16].copy_from_slice(b"mntr");
        p[16..20].copy_from_slice(b"RGB ");
        p[36..40].copy_from_slice(b"acsp");
        p[100..200].fill(0x5A);
        p
    }

    /// 40x20 pixels, left half red and right half blue, saved as JPEG with an
    /// Orientation tag and an ICC profile. Orientation 6 displays it 20x40 with the
    /// red half on top.
    fn write_tagged_jpeg(path: &Path, orientation: u8, icc: &[u8]) {
        use img_parts::{ImageEXIF, ImageICC};
        let img = image::RgbImage::from_fn(40, 20, |x, _| {
            if x < 20 {
                image::Rgb([230, 20, 20])
            } else {
                image::Rgb([20, 20, 230])
            }
        });
        let mut plain = Vec::new();
        DynamicImage::ImageRgb8(img)
            .write_to(
                &mut std::io::Cursor::new(&mut plain),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        let mut jpeg = img_parts::jpeg::Jpeg::from_bytes(plain.into()).unwrap();
        jpeg.set_exif(Some(orientation_exif(orientation).into()));
        jpeg.set_icc_profile(Some(icc.to_vec().into()));
        std::fs::write(path, jpeg.encoder().bytes()).unwrap();
    }

    fn opts_with_strip(format: ImageFormat, strip_metadata: bool) -> ImageOptions {
        ImageOptions {
            strip_metadata,
            ..default_opts(format)
        }
    }

    /// Dimensions, ICC profile and EXIF as an independent decoder sees the output.
    fn inspect(path: &Path) -> (DynamicImage, Option<Vec<u8>>, Option<Vec<u8>>) {
        let mut decoder = image::ImageReader::open(path)
            .unwrap()
            .with_guessed_format()
            .unwrap()
            .into_decoder()
            .unwrap();
        let icc = decoder.icc_profile().unwrap();
        let exif = decoder.exif_metadata().unwrap();
        let img = DynamicImage::from_decoder(decoder).unwrap();
        (img, icc, exif)
    }

    fn top_is_red(img: &DynamicImage) -> bool {
        let rgb = img.to_rgb8();
        let p = rgb.get_pixel(rgb.width() / 2, 2);
        p[0] > 150 && p[2] < 100
    }

    #[test]
    fn jpeg_output_is_upright_when_metadata_is_stripped() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        let output = dir.path().join("out.jpg");
        write_tagged_jpeg(&input, 6, &fake_rgb_icc());

        compress_with_threads(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &opts_with_strip(ImageFormat::Jpeg, true),
            1,
        )
        .unwrap();

        let (img, _, exif) = inspect(&output);
        assert_eq!((img.width(), img.height()), (20, 40));
        assert!(top_is_red(&img), "pixels were not rotated clockwise");
        assert!(exif.is_none());
    }

    #[test]
    fn jpeg_keeps_the_tag_and_is_not_rotated_twice_when_metadata_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        let output = dir.path().join("out.jpg");
        write_tagged_jpeg(&input, 6, &fake_rgb_icc());

        compress_with_threads(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &opts_with_strip(ImageFormat::Jpeg, false),
            1,
        )
        .unwrap();

        let (img, _, exif) = inspect(&output);
        // Stored pixels unchanged; the copied tag tells viewers to rotate once.
        assert_eq!((img.width(), img.height()), (40, 20));
        let tag = exif.as_deref().and_then(Orientation::from_exif_chunk);
        assert_eq!(tag, Some(Orientation::Rotate90));
    }

    #[test]
    fn png_output_is_upright_in_both_strip_modes() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        write_tagged_jpeg(&input, 6, &fake_rgb_icc());

        for strip in [true, false] {
            let output = dir.path().join(format!("out_{strip}.png"));
            compress_with_threads(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts_with_strip(ImageFormat::Png, strip),
                1,
            )
            .unwrap();
            let (img, _, exif) = inspect(&output);
            assert_eq!((img.width(), img.height()), (20, 40), "strip={strip}");
            assert!(top_is_red(&img), "strip={strip}");
            // No EXIF on PNG: a copied tag would rotate it a second time.
            assert!(exif.is_none(), "strip={strip}");
        }
    }

    #[test]
    fn orientation_is_applied_before_resize() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        let output = dir.path().join("out.jpg");
        write_tagged_jpeg(&input, 6, &fake_rgb_icc());

        let opts = ImageOptions {
            resize: Some(Resolution {
                width: 10,
                height: 0,
            }),
            ..default_opts(ImageFormat::Jpeg)
        };
        compress_with_threads(input.to_str().unwrap(), output.to_str().unwrap(), &opts, 1).unwrap();
        // Upright source is 20x40; width 10 fits it to 10x20.
        assert_eq!(image::image_dimensions(&output).unwrap(), (10, 20));
    }

    #[test]
    fn icc_profile_survives_jpeg_webp_and_png_in_both_strip_modes() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("p3.jpg");
        let profile = fake_rgb_icc();
        write_tagged_jpeg(&input, 1, &profile);

        for (format, ext) in [
            (ImageFormat::Jpeg, "jpg"),
            (ImageFormat::WebP, "webp"),
            (ImageFormat::Png, "png"),
        ] {
            for strip in [true, false] {
                let output = dir.path().join(format!("out_{strip}.{ext}"));
                compress_with_threads(
                    input.to_str().unwrap(),
                    output.to_str().unwrap(),
                    &opts_with_strip(format.clone(), strip),
                    1,
                )
                .unwrap();
                let (_, icc, _) = inspect(&output);
                assert_eq!(icc.as_ref(), Some(&profile), "{ext} strip={strip}");
            }
        }
    }

    #[test]
    fn exif_is_copied_only_when_metadata_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("tagged.jpg");
        write_tagged_jpeg(&input, 1, &fake_rgb_icc());

        for (format, ext) in [(ImageFormat::Jpeg, "jpg"), (ImageFormat::WebP, "webp")] {
            for strip in [true, false] {
                let output = dir.path().join(format!("out_{strip}.{ext}"));
                compress_with_threads(
                    input.to_str().unwrap(),
                    output.to_str().unwrap(),
                    &opts_with_strip(format.clone(), strip),
                    1,
                )
                .unwrap();
                let (_, _, exif) = inspect(&output);
                assert_eq!(exif.is_some(), !strip, "{ext} strip={strip}");
            }
        }
    }

    #[test]
    fn webp_with_alpha_keeps_alpha_and_icc() {
        use img_parts::ImageICC;
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("alpha.png");
        let output = dir.path().join("alpha.webp");
        let rgba = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(32, 32, |x, _| {
            image::Rgba([200, 100, 50, if x < 16 { 0 } else { 255 }])
        }));
        let mut plain = Vec::new();
        rgba.write_to(
            &mut std::io::Cursor::new(&mut plain),
            image::ImageFormat::Png,
        )
        .unwrap();
        let mut png = img_parts::png::Png::from_bytes(plain.into()).unwrap();
        let profile = fake_rgb_icc();
        png.set_icc_profile(Some(profile.clone().into()));
        std::fs::write(&input, png.encoder().bytes()).unwrap();

        compress_with_threads(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &default_opts(ImageFormat::WebP),
            1,
        )
        .unwrap();
        let (img, icc, _) = inspect(&output);
        assert_eq!(icc, Some(profile));
        assert!(img.color().has_alpha());
        assert!(img.to_rgba8().get_pixel(2, 2)[3] < 50);
    }

    #[test]
    fn icc_profile_must_match_the_output_colour_space() {
        let rgb = fake_rgb_icc();
        let mut cmyk = rgb.clone();
        cmyk[16..20].copy_from_slice(b"CMYK");
        let mut gray = rgb.clone();
        gray[16..20].copy_from_slice(b"GRAY");
        assert!(icc_fits(&rgb, false));
        assert!(!icc_fits(&cmyk, false));
        assert!(!icc_fits(&gray, false));
        assert!(icc_fits(&gray, true));
        assert!(!icc_fits(&[0u8; 8], false));
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
