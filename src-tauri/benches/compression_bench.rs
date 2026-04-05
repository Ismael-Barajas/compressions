use compressions_lib::compression::image::compress;
use compressions_lib::compression::progress::parse_progress_line;
use compressions_lib::types::{ImageFormat, ImageOptions};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};

fn bench_parse_progress(c: &mut Criterion) {
    let line = "frame=  120 fps=30 time=00:00:04.00 speed=1.5x";
    c.bench_function("parse_progress_line", |b| {
        b.iter(|| parse_progress_line(black_box(line), black_box(10.0)))
    });
}

fn create_test_image(path: &std::path::Path, width: u32, height: u32) {
    let img = image::RgbaImage::from_fn(width, height, |x, y| {
        image::Rgba([(x % 256) as u8, (y % 256) as u8, 128, 255])
    });
    img.save(path).expect("Failed to save test image");
}

fn opts(format: ImageFormat, quality: u8) -> ImageOptions {
    ImageOptions {
        format,
        quality,
        resize: None,
        strip_metadata: true,
    }
}

fn bench_jpeg_1080p(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input_1080.png");
    create_test_image(&input, 1920, 1080);

    c.bench_function("jpeg_q80_1920x1080", |b| {
        let output = dir.path().join("out.jpg");
        b.iter(|| {
            compress(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts(ImageFormat::Jpeg, 80),
            )
            .unwrap()
        })
    });
}

fn bench_png_1080p(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input_1080.png");
    create_test_image(&input, 1920, 1080);

    c.bench_function("png_1920x1080", |b| {
        let output = dir.path().join("out.png");
        b.iter(|| {
            compress(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts(ImageFormat::Png, 80),
            )
            .unwrap()
        })
    });
}

fn bench_webp_1080p(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input_1080.png");
    create_test_image(&input, 1920, 1080);

    c.bench_function("webp_q75_1920x1080", |b| {
        let output = dir.path().join("out.webp");
        b.iter(|| {
            compress(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts(ImageFormat::WebP, 75),
            )
            .unwrap()
        })
    });
}

fn bench_avif_1080p(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input_1080.png");
    create_test_image(&input, 1920, 1080);

    c.bench_function("avif_q80_1920x1080", |b| {
        let output = dir.path().join("out.avif");
        b.iter(|| {
            compress(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts(ImageFormat::Avif, 80),
            )
            .unwrap()
        })
    });
}

fn bench_avif_speed_comparison(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input_720.png");
    create_test_image(&input, 1280, 720);

    let rgba = image::open(&input).unwrap().to_rgba8();
    let (width, height) = image::GenericImageView::dimensions(&rgba);
    let pixels: Vec<rgb::RGBA8> = rgba
        .pixels()
        .map(|p| rgb::RGBA8 {
            r: p[0],
            g: p[1],
            b: p[2],
            a: p[3],
        })
        .collect();

    let mut group = c.benchmark_group("avif_speed");
    for speed in [6, 7, 8] {
        group.bench_with_input(BenchmarkId::new("speed", speed), &speed, |b, &spd| {
            b.iter(|| {
                let img_ref =
                    ravif::Img::new(&pixels[..], width as usize, height as usize);
                ravif::Encoder::new()
                    .with_quality(80.0)
                    .with_speed(black_box(spd))
                    .encode_rgba(img_ref)
                    .unwrap()
            })
        });
    }
    group.finish();
}

fn create_test_gif(path: &std::path::Path, width: u16, height: u16, frames: usize) {
    use gif::{Encoder, Frame, Repeat};
    use std::fs::File;

    let file = File::create(path).unwrap();
    let mut palette = Vec::with_capacity(256 * 3);
    for i in 0..256u16 {
        palette.push(i as u8);
        palette.push((255 - i) as u8);
        palette.push(128);
    }
    let mut encoder = Encoder::new(file, width, height, &palette).unwrap();
    encoder.set_repeat(Repeat::Infinite).unwrap();

    for f in 0..frames {
        let pixel_count = width as usize * height as usize;
        let pixels: Vec<u8> = (0..pixel_count)
            .map(|i| ((i + f * 7) % 256) as u8)
            .collect();
        let mut frame = Frame::default();
        frame.width = width;
        frame.height = height;
        frame.delay = 5;
        frame.buffer = std::borrow::Cow::Owned(pixels);
        encoder.write_frame(&frame).unwrap();
    }
}

fn bench_gif_requantize(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("input.gif");
    create_test_gif(&input, 320, 240, 10);

    c.bench_function("gif_requantize_320x240_10frames", |b| {
        let output = dir.path().join("out.gif");
        b.iter(|| {
            compress(
                input.to_str().unwrap(),
                output.to_str().unwrap(),
                &opts(ImageFormat::Gif, 80),
            )
            .unwrap()
        })
    });
}

fn bench_jpeg_scaling(c: &mut Criterion) {
    let dir = tempfile::tempdir().unwrap();
    let sizes: &[(u32, u32)] = &[(256, 256), (1024, 1024), (1920, 1080)];

    for &(w, h) in sizes {
        let input = dir.path().join(format!("input_{}x{}.png", w, h));
        create_test_image(&input, w, h);
        let output = dir.path().join(format!("out_{}x{}.jpg", w, h));

        c.bench_function(&format!("jpeg_q80_{}x{}", w, h), |b| {
            b.iter(|| {
                compress(
                    input.to_str().unwrap(),
                    output.to_str().unwrap(),
                    &opts(ImageFormat::Jpeg, 80),
                )
                .unwrap()
            })
        });
    }
}

criterion_group!(
    benches,
    bench_parse_progress,
    bench_jpeg_1080p,
    bench_png_1080p,
    bench_webp_1080p,
    bench_avif_1080p,
    bench_avif_speed_comparison,
    bench_gif_requantize,
    bench_jpeg_scaling,
);
criterion_main!(benches);
