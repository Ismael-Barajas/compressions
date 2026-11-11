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

    let decoded = decode_image_bounded(input, true)?;
    let preserve = !options.strip_metadata;

    // Resolve Original → concrete format (defensive; commands layer resolves first)
    let effective_format = options.format.resolve_for_input(input);

    // EXIF goes across only when metadata is kept, and only JPEG and WebP outputs
    // carry it.
    let carries_exif = matches!(effective_format, ImageFormat::Jpeg | ImageFormat::WebP);
    let mut exif_to_copy = if preserve && carries_exif {
        decoded.exif
    } else {
        None
    };

    // Output pixels are always upright: without this a stripped output displays
    // sideways once the tag is gone. A copied EXIF block gets its orientation tag
    // reset, so viewers that honour the tag do not rotate a second time and
    // viewers that ignore it (common for WebP) still show the image upright.
    let mut img = decoded.image;
    let needs_rotation = decoded.orientation != Orientation::NoTransforms;
    if needs_rotation {
        tracing::debug!(input = %input, orientation = ?decoded.orientation, "Applying EXIF orientation to pixels");
        img.apply_orientation(decoded.orientation);
        if let Some(exif) = exif_to_copy.as_mut() {
            // None means the block held no readable tag, so there is nothing to reset.
            let _ = Orientation::remove_from_exif_chunk(exif);
            // The embedded thumbnail is still sideways and is now untagged.
            if drop_exif_thumbnail(exif) {
                tracing::debug!(input = %input, "Dropped EXIF thumbnail after rotating pixels");
            } else {
                tracing::debug!(input = %input, "EXIF block unreadable; thumbnail left as is");
            }
        }
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
///
/// `read_icc` is false for callers that throw the profile away (thumbnails): for a
/// JPEG every metadata call re-parses the headers, so skipping it saves one pass.
pub fn decode_image_bounded(input: &str, read_icc: bool) -> Result<DecodedImage, String> {
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
    // EXIF is read once and the orientation taken from that block. Only without a
    // readable tag is the decoder asked (TIFF keeps orientation in its own header;
    // the JPEG and WebP decoders answer from what `exif_metadata` already cached).
    let exif = decoder.exif_metadata().unwrap_or_else(|e| {
        tracing::warn!(input = %input, error = %e, "Could not read EXIF");
        None
    });
    let orientation_from_exif = exif.as_deref().and_then(Orientation::from_exif_chunk);
    let orientation = match orientation_from_exif {
        Some(o) => o,
        None => decoder.orientation().unwrap_or_else(|e| {
            tracing::warn!(input = %input, error = %e, "Could not read image orientation");
            Orientation::NoTransforms
        }),
    };
    let icc_profile = if read_icc {
        decoder.icc_profile().unwrap_or_else(|e| {
            tracing::warn!(input = %input, error = %e, "Could not read ICC profile");
            None
        })
    } else {
        None
    };

    let image = DynamicImage::from_decoder(decoder)
        .map_err(|e| format!("Failed to decode image: {}", e))?;
    Ok(DecodedImage {
        image,
        orientation,
        icc_profile,
        exif,
    })
}

/// Cut the EXIF thumbnail (IFD1) off a raw TIFF-structured EXIF block by zeroing
/// the "next IFD" offset that follows IFD0. Used when the pixels were rotated
/// upright and the Orientation tag reset: the thumbnail is still stored sideways
/// and nothing would tell a viewer to rotate it any more. The thumbnail bytes stay
/// in the block, unreferenced. Returns false, leaving the block untouched, when the
/// structure is not one it can read safely.
fn drop_exif_thumbnail(exif: &mut [u8]) -> bool {
    next_ifd_offset_range(exif)
        .and_then(|range| exif.get_mut(range))
        .map(|next| next.fill(0))
        .is_some()
}

/// Byte range of the 4-byte "next IFD" offset after IFD0, if the TIFF header and
/// IFD0 entry count can be read and the range lies inside `exif`.
fn next_ifd_offset_range(exif: &[u8]) -> Option<std::ops::Range<usize>> {
    let little_endian = match exif.get(0..4)? {
        [0x49, 0x49, 0x2A, 0x00] => true,
        [0x4D, 0x4D, 0x00, 0x2A] => false,
        _ => return None,
    };
    let ifd0_bytes: [u8; 4] = exif.get(4..8)?.try_into().ok()?;
    let ifd0 = if little_endian {
        u32::from_le_bytes(ifd0_bytes)
    } else {
        u32::from_be_bytes(ifd0_bytes)
    };
    let ifd0 = usize::try_from(ifd0).ok()?;
    let count_bytes: [u8; 2] = exif.get(ifd0..ifd0.checked_add(2)?)?.try_into().ok()?;
    let entry_count = if little_endian {
        u16::from_le_bytes(count_bytes)
    } else {
        u16::from_be_bytes(count_bytes)
    };
    // IFD0 = 2-byte count, 12 bytes per entry, then the 4-byte next-IFD offset.
    let start = ifd0
        .checked_add(2)?
        .checked_add(usize::from(entry_count) * 12)?;
    let end = start.checked_add(4)?;
    (end <= exif.len()).then_some(start..end)
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
/// An error rather than a silent fallback, so metadata the user asked to keep is
/// never dropped without a trace.
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
    match exif {
        Some(exif) if exif.len() > JPEG_MAX_EXIF_LEN => {
            // A PNG eXIf or WebP EXIF chunk can be larger than one APP1 segment, and
            // img-parts would panic encoding the length. JPEG has no standard way to
            // split EXIF, so the output goes without it.
            tracing::warn!(
                exif_len = exif.len(),
                max = JPEG_MAX_EXIF_LEN,
                "EXIF block too large for one JPEG APP1 segment; writing the JPEG without EXIF"
            );
        }
        Some(exif) => jpeg.set_exif(Some(exif.into())),
        None => {}
    }
    Ok(jpeg.encoder().bytes().to_vec())
}

/// Largest raw EXIF block one JPEG APP1 segment holds. The segment length field is
/// a u16 that counts itself (2 bytes), and img-parts prepends `Exif\0\0` (6 bytes):
/// 65535 - 2 - 6.
const JPEG_MAX_EXIF_LEN: usize = u16::MAX as usize - 2 - 6;

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
    put_webp_chunks_in_spec_order(webp.chunks_mut())?;

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

/// The extended WebP layout is `VP8X, ICCP, (ANIM), ALPH, VP8 , EXIF, XMP`, and
/// Chromium refuses a file whose ICCP comes later. img-parts 0.4 places ICCP in
/// front of the `VP8 ` chunk, which for an alpha image is after ALPH. Move ICCP to
/// directly after VP8X and EXIF to the end (it is the only trailing chunk we write).
fn put_webp_chunks_in_spec_order(
    chunks: &mut Vec<img_parts::riff::RiffChunk>,
) -> Result<(), String> {
    use img_parts::webp::{CHUNK_EXIF, CHUNK_ICCP, CHUNK_VP8X};

    if let Some(i) = chunks.iter().position(|c| c.id() == CHUNK_ICCP) {
        let iccp = chunks.remove(i);
        let vp8x = chunks
            .iter()
            .position(|c| c.id() == CHUNK_VP8X)
            .ok_or("Failed to add WebP metadata: no VP8X header")?;
        chunks.insert(vp8x + 1, iccp);
    }
    if let Some(i) = chunks.iter().position(|c| c.id() == CHUNK_EXIF) {
        let exif = chunks.remove(i);
        chunks.push(exif);
    }
    Ok(())
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
    fn kept_exif_has_its_orientation_reset_so_nothing_rotates_twice() {
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
        // Pixels are upright and the copied EXIF says "no rotation needed".
        assert_eq!((img.width(), img.height()), (20, 40));
        assert!(top_is_red(&img), "pixels were not rotated clockwise");
        let tag = exif.as_deref().and_then(Orientation::from_exif_chunk);
        assert_eq!(tag, Some(Orientation::NoTransforms));
    }

    #[test]
    fn rotated_jpeg_to_webp_is_upright_when_metadata_is_kept() {
        // Many viewers ignore EXIF orientation in WebP, so the pixels must carry it.
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        let output = dir.path().join("out.webp");
        write_tagged_jpeg(&input, 6, &fake_rgb_icc());

        compress_with_threads(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &opts_with_strip(ImageFormat::WebP, false),
            1,
        )
        .unwrap();

        let (img, _, exif) = inspect(&output);
        assert_eq!((img.width(), img.height()), (20, 40));
        assert!(top_is_red(&img), "pixels were not rotated clockwise");
        let tag = exif.as_deref().and_then(Orientation::from_exif_chunk);
        assert_eq!(tag, Some(Orientation::NoTransforms));
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
            // PNG output carries no EXIF.
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

    /// A small but complete ICC v2 display profile (sRGB primaries adapted to D50,
    /// gamma 2.2 curves): header, tag table and the tags a colour engine needs.
    fn minimal_srgb_icc() -> Vec<u8> {
        fn s15(v: f64) -> [u8; 4] {
            ((v * 65536.0).round() as i32).to_be_bytes()
        }
        fn xyz(x: f64, y: f64, z: f64) -> Vec<u8> {
            let mut t = b"XYZ \0\0\0\0".to_vec();
            for v in [x, y, z] {
                t.extend_from_slice(&s15(v));
            }
            t
        }
        let curve = b"curv\0\0\0\0\0\0\0\x01\x02\x33".to_vec(); // gamma 2.2 (u8Fixed8)
        let mut desc = b"desc\0\0\0\0".to_vec();
        let name = b"Test sRGB\0";
        desc.extend_from_slice(&(name.len() as u32).to_be_bytes());
        desc.extend_from_slice(name);
        desc.extend_from_slice(&[0u8; 4 + 4 + 2 + 1 + 67]); // empty Unicode and ScriptCode parts
        let tags: Vec<(&[u8; 4], Vec<u8>)> = vec![
            (b"desc", desc),
            (b"wtpt", xyz(0.9642, 1.0, 0.8249)),
            (b"rXYZ", xyz(0.4361, 0.2225, 0.0139)),
            (b"gXYZ", xyz(0.3851, 0.7169, 0.0971)),
            (b"bXYZ", xyz(0.1431, 0.0606, 0.7141)),
            (b"rTRC", curve.clone()),
            (b"gTRC", curve.clone()),
            (b"bTRC", curve),
            (b"cprt", b"text\0\0\0\0No copyright\0".to_vec()),
        ];
        let table_len = 4 + 12 * tags.len();
        let mut body = Vec::new();
        let mut table = (tags.len() as u32).to_be_bytes().to_vec();
        for (sig, data) in &tags {
            let offset = 128 + table_len + body.len();
            table.extend_from_slice(*sig);
            table.extend_from_slice(&(offset as u32).to_be_bytes());
            table.extend_from_slice(&(data.len() as u32).to_be_bytes());
            body.extend_from_slice(data);
            while body.len() % 4 != 0 {
                body.push(0);
            }
        }
        let size = 128 + table_len + body.len();
        let mut p = vec![0u8; 128];
        p[0..4].copy_from_slice(&(size as u32).to_be_bytes());
        p[8..12].copy_from_slice(&[0x02, 0x10, 0, 0]);
        p[12..16].copy_from_slice(b"mntr");
        p[16..20].copy_from_slice(b"RGB ");
        p[20..24].copy_from_slice(b"XYZ ");
        p[36..40].copy_from_slice(b"acsp");
        p[68..72].copy_from_slice(&s15(0.9642));
        p[72..76].copy_from_slice(&s15(1.0));
        p[76..80].copy_from_slice(&s15(0.8249));
        p.extend_from_slice(&table);
        p.extend_from_slice(&body);
        p
    }

    /// Chunk ids of a RIFF WebP file, in file order.
    fn webp_chunk_ids(data: &[u8]) -> Vec<String> {
        assert_eq!(&data[0..4], b"RIFF");
        assert_eq!(&data[8..12], b"WEBP");
        let mut ids = Vec::new();
        let mut at = 12;
        while at + 8 <= data.len() {
            ids.push(String::from_utf8_lossy(&data[at..at + 4]).into_owned());
            let size = u32::from_le_bytes(data[at + 4..at + 8].try_into().unwrap()) as usize;
            at += 8 + size + (size & 1);
        }
        assert_eq!(at, data.len(), "chunk sizes do not add up to the file");
        ids
    }

    /// 32x32 PNG, half transparent when `alpha`, carrying the given ICC profile and
    /// EXIF block. eXIf goes before IDAT so the decoder reads it with the header.
    fn write_png_with_metadata(path: &Path, alpha: bool, icc: Option<&[u8]>, exif: Option<&[u8]>) {
        use img_parts::ImageICC;
        let img = if alpha {
            DynamicImage::ImageRgba8(image::RgbaImage::from_fn(32, 32, |x, _| {
                image::Rgba([200, 100, 50, if x < 16 { 0 } else { 255 }])
            }))
        } else {
            DynamicImage::ImageRgb8(image::RgbImage::from_fn(32, 32, |x, y| {
                image::Rgb([(x * 8) as u8, (y * 8) as u8, 90])
            }))
        };
        let mut plain = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut plain),
            image::ImageFormat::Png,
        )
        .unwrap();
        let mut png = img_parts::png::Png::from_bytes(plain.into()).unwrap();
        if let Some(icc) = icc {
            png.set_icc_profile(Some(icc.to_vec().into()));
        }
        if let Some(exif) = exif {
            let chunk = img_parts::png::PngChunk::new(*b"eXIf", exif.to_vec().into());
            png.chunks_mut().insert(1, chunk);
        }
        std::fs::write(path, png.encoder().bytes()).unwrap();
    }

    #[test]
    fn webp_metadata_chunks_follow_the_container_spec_order() {
        let dir = tempfile::tempdir().unwrap();
        let profile = minimal_srgb_icc();
        let exif = orientation_exif(1);
        for alpha in [true, false] {
            for (with_icc, with_exif) in [(true, false), (false, true), (true, true)] {
                let case = format!("alpha={alpha} icc={with_icc} exif={with_exif}");
                let input = dir
                    .path()
                    .join(format!("in_{alpha}_{with_icc}_{with_exif}.png"));
                let output = input.with_extension("webp");
                write_png_with_metadata(
                    &input,
                    alpha,
                    with_icc.then_some(profile.as_slice()),
                    with_exif.then_some(exif.as_slice()),
                );
                compress_with_threads(
                    input.to_str().unwrap(),
                    output.to_str().unwrap(),
                    &opts_with_strip(ImageFormat::WebP, false),
                    1,
                )
                .unwrap();

                let data = std::fs::read(&output).unwrap();
                let mut expected = vec!["VP8X"];
                if with_icc {
                    expected.push("ICCP");
                }
                if alpha {
                    expected.push("ALPH");
                }
                expected.push("VP8 ");
                if with_exif {
                    expected.push("EXIF");
                }
                assert_eq!(webp_chunk_ids(&data), expected, "{case}");

                // VP8X flags match the chunks present.
                let flags = data[20];
                assert_eq!(flags & 0b0010_0000 != 0, with_icc, "ICC flag, {case}");
                assert_eq!(flags & 0b0000_1000 != 0, with_exif, "EXIF flag, {case}");
                assert_eq!(flags & 0b0001_0000 != 0, alpha, "alpha flag, {case}");

                let (_, icc, out_exif) = inspect(&output);
                assert_eq!(icc, with_icc.then(|| profile.clone()), "{case}");
                assert_eq!(out_exif.is_some(), with_exif, "{case}");
            }
        }
    }

    /// A JPEG made by the image crate, small enough to keep the test quick.
    fn tiny_jpeg() -> Vec<u8> {
        let mut data = Vec::new();
        DynamicImage::ImageRgb8(image::RgbImage::new(8, 8))
            .write_to(
                &mut std::io::Cursor::new(&mut data),
                image::ImageFormat::Jpeg,
            )
            .unwrap();
        data
    }

    #[test]
    fn jpeg_exif_that_fits_one_app1_segment_is_kept() {
        use img_parts::ImageEXIF;
        for len in [JPEG_MAX_EXIF_LEN - 1, JPEG_MAX_EXIF_LEN] {
            let mut exif = orientation_exif(1);
            exif.resize(len, 0xAB);
            let out = embed_jpeg_metadata(tiny_jpeg(), None, Some(exif.clone())).unwrap();
            let parsed = img_parts::jpeg::Jpeg::from_bytes(out.clone().into()).unwrap();
            assert_eq!(parsed.exif().as_deref(), Some(exif.as_slice()), "len={len}");
            assert!(image::load_from_memory(&out).is_ok(), "len={len}");
        }
    }

    #[test]
    fn jpeg_exif_too_large_for_one_app1_segment_is_dropped_without_panicking() {
        use img_parts::ImageEXIF;
        for len in [JPEG_MAX_EXIF_LEN + 1, JPEG_MAX_EXIF_LEN + 7, 200_000] {
            let mut exif = orientation_exif(1);
            exif.resize(len, 0xAB);
            let icc = fake_rgb_icc();
            let out = embed_jpeg_metadata(tiny_jpeg(), Some(icc.clone()), Some(exif)).unwrap();
            let parsed = img_parts::jpeg::Jpeg::from_bytes(out.clone().into()).unwrap();
            assert!(parsed.exif().is_none(), "len={len}");
            // The ICC profile still goes in.
            use img_parts::ImageICC;
            assert_eq!(parsed.icc_profile().as_deref(), Some(icc.as_slice()));
            assert!(image::load_from_memory(&out).is_ok(), "len={len}");
        }
    }

    /// TIFF-structured EXIF: IFD0 with an Orientation entry, then IFD1 describing a
    /// 4-byte "thumbnail". The next-IFD offset after IFD0 sits at bytes 22..26.
    fn exif_with_thumbnail(little_endian: bool, orientation: u16) -> Vec<u8> {
        let mut b = Vec::new();
        let p16 = |b: &mut Vec<u8>, v: u16| {
            b.extend_from_slice(&if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            })
        };
        let p32 = |b: &mut Vec<u8>, v: u32| {
            b.extend_from_slice(&if little_endian {
                v.to_le_bytes()
            } else {
                v.to_be_bytes()
            })
        };
        b.extend_from_slice(if little_endian { b"II" } else { b"MM" });
        p16(&mut b, 42);
        p32(&mut b, 8); // IFD0 at 8
        p16(&mut b, 1);
        p16(&mut b, 0x0112); // Orientation, SHORT, 1 value
        p16(&mut b, 3);
        p32(&mut b, 1);
        p16(&mut b, orientation);
        p16(&mut b, 0);
        p32(&mut b, 26); // IFD1 at 26
        p16(&mut b, 2);
        p16(&mut b, 0x0201); // JPEGInterchangeFormat, LONG
        p16(&mut b, 4);
        p32(&mut b, 1);
        p32(&mut b, 56);
        p16(&mut b, 0x0202); // JPEGInterchangeFormatLength, LONG
        p16(&mut b, 4);
        p32(&mut b, 1);
        p32(&mut b, 4);
        p32(&mut b, 0); // no IFD2
        b.extend_from_slice(&[0xFF, 0xD8, 0xFF, 0xD9]);
        assert_eq!(b.len(), 60);
        b
    }

    #[test]
    fn exif_thumbnail_is_unlinked_in_both_byte_orders() {
        for little_endian in [true, false] {
            let original = exif_with_thumbnail(little_endian, 6);
            assert_ne!(&original[22..26], &[0, 0, 0, 0]);
            let mut exif = original.clone();
            assert!(drop_exif_thumbnail(&mut exif), "le={little_endian}");
            assert_eq!(&exif[22..26], &[0, 0, 0, 0], "le={little_endian}");
            // Nothing else changes, and IFD0 still reads.
            assert_eq!(&exif[..22], &original[..22]);
            assert_eq!(&exif[26..], &original[26..]);
            assert_eq!(
                Orientation::from_exif_chunk(&exif),
                Some(Orientation::Rotate90)
            );
        }
    }

    #[test]
    fn malformed_exif_is_left_alone() {
        for little_endian in [true, false] {
            let full = exif_with_thumbnail(little_endian, 6);
            // Cut anywhere before the end of the next-IFD field: refused, unchanged.
            for len in 0..26 {
                let mut exif = full[..len].to_vec();
                assert!(!drop_exif_thumbnail(&mut exif), "len={len}");
                assert_eq!(exif, &full[..len]);
            }
            // IFD0 offset past the end, and an entry count running past the end.
            let mut far = full.clone();
            far[4..8].copy_from_slice(&if little_endian {
                u32::MAX.to_le_bytes()
            } else {
                u32::MAX.to_be_bytes()
            });
            let before = far.clone();
            assert!(!drop_exif_thumbnail(&mut far));
            assert_eq!(far, before);
            let mut many = full.clone();
            many[8..10].copy_from_slice(&[0xFF, 0xFF]);
            let before = many.clone();
            assert!(!drop_exif_thumbnail(&mut many));
            assert_eq!(many, before);
        }
        let mut not_tiff = b"Exif\0\0II*\0\x08\0\0\0".to_vec();
        assert!(!drop_exif_thumbnail(&mut not_tiff));
    }

    /// Like `write_tagged_jpeg`, with `exif` as the whole EXIF block.
    fn write_jpeg_with_exif(path: &Path, exif: &[u8]) {
        use img_parts::ImageEXIF;
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
        jpeg.set_exif(Some(exif.to_vec().into()));
        std::fs::write(path, jpeg.encoder().bytes()).unwrap();
    }

    #[test]
    fn rotated_output_drops_the_sideways_exif_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        for little_endian in [true, false] {
            let input = dir.path().join(format!("thumb6_{little_endian}.jpg"));
            let output = dir.path().join(format!("thumb6_{little_endian}_out.jpg"));
            write_jpeg_with_exif(&input, &exif_with_thumbnail(little_endian, 6));
            compress_with_threads(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts_with_strip(ImageFormat::Jpeg, false),
                1,
            )
            .unwrap();
            let (img, _, exif) = inspect(&output);
            assert_eq!((img.width(), img.height()), (20, 40));
            let exif = exif.expect("EXIF kept");
            assert_eq!(&exif[22..26], &[0, 0, 0, 0], "le={little_endian}");
            assert_eq!(
                Orientation::from_exif_chunk(&exif),
                Some(Orientation::NoTransforms)
            );
        }
    }

    #[test]
    fn unrotated_output_keeps_the_exif_block_as_is() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("thumb1.jpg");
        let output = dir.path().join("thumb1_out.jpg");
        let block = exif_with_thumbnail(true, 1);
        write_jpeg_with_exif(&input, &block);
        compress_with_threads(
            input.to_str().unwrap(),
            output.to_str().unwrap(),
            &opts_with_strip(ImageFormat::Jpeg, false),
            1,
        )
        .unwrap();
        let (_, _, exif) = inspect(&output);
        assert_eq!(exif, Some(block));
    }

    #[test]
    fn decode_without_icc_still_reads_orientation_and_exif() {
        let dir = tempfile::tempdir().unwrap();
        let input = dir.path().join("rotated.jpg");
        write_tagged_jpeg(&input, 6, &fake_rgb_icc());

        let full = decode_image_bounded(input.to_str().unwrap(), true).unwrap();
        assert_eq!(full.orientation, Orientation::Rotate90);
        assert_eq!(full.icc_profile, Some(fake_rgb_icc()));
        assert!(full.exif.is_some());

        let lean = decode_image_bounded(input.to_str().unwrap(), false).unwrap();
        assert_eq!(lean.orientation, Orientation::Rotate90);
        assert_eq!(lean.icc_profile, None);
        assert_eq!(lean.exif, full.exif);
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
