use criterion::{criterion_group, criterion_main, Criterion, black_box};
use compressions_lib::compression::progress::parse_progress_line;
use compressions_lib::compression::image::compress;
use compressions_lib::types::{ImageFormat, ImageOptions};

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
            ).unwrap()
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
            ).unwrap()
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
            ).unwrap()
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
                ).unwrap()
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
    bench_jpeg_scaling,
);
criterion_main!(benches);
