// Measurement harness for the native image pipeline, used by docs/PERFORMANCE_ROUND_3.md.
// It times the real encoders on real files and checks orientation / colour-profile handling.
//
//   cargo run --release --example perf_probe -- <mode> <photo.jpg> <screenshot.png> <out_dir>
//
// Modes: single | moz | webp | batch | orient | all
//
// On Windows with an Intel hybrid CPU, a process started from a background shell can be
// confined to the efficiency cores, which roughly halves the batch numbers. See the doc.
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use compressions_lib::compression::image::{compress_with_threads, open_image_bounded};
use compressions_lib::types::{ImageFormat, ImageOptions, ResizeMode};

fn opts(format: ImageFormat, quality: u8, strip: bool) -> ImageOptions {
    ImageOptions {
        format,
        quality,
        resize: None,
        resize_mode: ResizeMode::Fit,
        strip_metadata: strip,
    }
}

fn best_of<T>(runs: usize, mut f: impl FnMut() -> T) -> (T, f64) {
    let mut best = f64::MAX;
    let mut last = None;
    for _ in 0..runs {
        let t = Instant::now();
        let v = f();
        best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        last = Some(v);
    }
    (last.unwrap(), best)
}

fn size(p: &str) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

fn single(jpg: &str, png: &str, out: &str) {
    println!("== single-image timings (best of 3 unless noted)");
    let (img, ms) = best_of(3, || open_image_bounded(jpg).unwrap());
    println!("decode jpg {}x{}: {:.0} ms", img.width(), img.height(), ms);
    let (img2, ms) = best_of(3, || open_image_bounded(png).unwrap());
    println!(
        "decode png {}x{}: {:.0} ms",
        img2.width(),
        img2.height(),
        ms
    );

    let o = format!("{out}/s.jpg");
    let (_, ms) = best_of(3, || {
        compress_with_threads(jpg, &o, &opts(ImageFormat::Jpeg, 80, true), 1).unwrap()
    });
    println!("jpg->jpeg q80: {:.0} ms, {} bytes", ms, size(&o));

    let o = format!("{out}/s.webp");
    let (_, ms) = best_of(3, || {
        compress_with_threads(jpg, &o, &opts(ImageFormat::WebP, 80, true), 1).unwrap()
    });
    println!("jpg->webp q80: {:.0} ms, {} bytes", ms, size(&o));

    for threads in [1usize, 4, 32] {
        let o = format!("{out}/s_{threads}.png");
        let (_, ms) = best_of(3, || {
            compress_with_threads(png, &o, &opts(ImageFormat::Png, 80, true), threads).unwrap()
        });
        println!(
            "png->png threads={threads}: {:.0} ms, {} bytes",
            ms,
            size(&o)
        );
    }
    let o = format!("{out}/s_photo.png");
    let (_, ms) = best_of(1, || {
        compress_with_threads(jpg, &o, &opts(ImageFormat::Png, 80, true), 4).unwrap()
    });
    println!(
        "jpg->png threads=4 (1 run): {:.0} ms, {} bytes",
        ms,
        size(&o)
    );

    for threads in [1usize, 4, 8, 32] {
        let o = format!("{out}/s_{threads}.avif");
        let (_, ms) = best_of(1, || {
            compress_with_threads(jpg, &o, &opts(ImageFormat::Avif, 80, true), threads).unwrap()
        });
        println!(
            "jpg->avif q80 speed6 threads={threads} (1 run): {:.0} ms, {} bytes",
            ms,
            size(&o)
        );
    }
}

fn moz_encode(raw: &[u8], w: usize, h: usize, cfg: &dyn Fn(&mut mozjpeg::Compress)) -> Vec<u8> {
    let mut comp = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    comp.set_size(w, h);
    cfg(&mut comp);
    let mut c = comp.start_compress(Vec::new()).unwrap();
    c.write_scanlines(raw).unwrap();
    c.finish().unwrap()
}

fn moz(jpg: &str) {
    println!("== mozjpeg settings, q80, encode only (best of 3)");
    let rgb = open_image_bounded(jpg).unwrap().to_rgb8();
    let (w, h) = (rgb.width() as usize, rgb.height() as usize);
    let raw = rgb.as_raw();
    let variants: Vec<(&str, Box<dyn Fn(&mut mozjpeg::Compress)>)> = vec![
        (
            "app today (mozjpeg defaults)",
            Box::new(|c| c.set_quality(80.0)),
        ),
        (
            "optimize_scans off",
            Box::new(|c| {
                c.set_quality(80.0);
                c.set_optimize_scans(false);
            }),
        ),
        (
            "fastest profile + progressive + optimized huffman",
            Box::new(|c| {
                c.set_fastest_defaults();
                c.set_quality(80.0);
                c.set_progressive_mode();
                c.set_optimize_coding(true);
            }),
        ),
        (
            "fastest profile + optimized huffman (baseline)",
            Box::new(|c| {
                c.set_fastest_defaults();
                c.set_quality(80.0);
                c.set_optimize_coding(true);
            }),
        ),
        (
            "fastest profile (libjpeg-turbo-like)",
            Box::new(|c| {
                c.set_fastest_defaults();
                c.set_quality(80.0);
            }),
        ),
    ];
    let mut base = 0usize;
    for (name, cfg) in &variants {
        let (data, ms) = best_of(3, || moz_encode(raw, w, h, cfg.as_ref()));
        if base == 0 {
            base = data.len();
        }
        println!(
            "{name}: {:.0} ms, {} bytes ({:+.1}% vs today)",
            ms,
            data.len(),
            (data.len() as f64 / base as f64 - 1.0) * 100.0
        );
    }
}

fn webp_variants(jpg: &str) {
    println!("== webp settings, q80, encode only (best of 3)");
    let img = image::DynamicImage::ImageRgb8(open_image_bounded(jpg).unwrap().to_rgb8());
    let enc = webp::Encoder::from_image(&img).unwrap();
    let (d, ms) = best_of(3, || enc.encode(80.0).len());
    println!(
        "app today (simple API, method 4): {:.0} ms, {} bytes",
        ms, d
    );
    for (method, threads) in [(4, 1), (3, 0), (3, 1), (2, 0)] {
        let mut cfg = webp::WebPConfig::new().unwrap();
        cfg.quality = 80.0;
        cfg.method = method;
        cfg.thread_level = threads;
        let (d, ms) = best_of(3, || enc.encode_advanced(&cfg).unwrap().len());
        println!(
            "method {method} thread_level {threads}: {:.0} ms, {} bytes",
            ms, d
        );
    }
}

fn run_batch(n: usize, workers: usize, job: &(dyn Fn(usize) + Sync)) -> f64 {
    let next = AtomicUsize::new(0);
    let t = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                if i >= n {
                    break;
                }
                job(i);
            });
        }
    });
    t.elapsed().as_secs_f64()
}

fn batch(jpg: &str, png: &str, out: &str) {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    println!("== batch scaling ({cores} logical cores); app today = 8 workers");
    let n = 96;
    for workers in [8usize, 12, 16, 24, 32] {
        let secs = run_batch(n, workers, &|i| {
            let o = format!("{out}/b_{i}.jpg");
            compress_with_threads(jpg, &o, &opts(ImageFormat::Jpeg, 80, true), 1).unwrap();
        });
        println!(
            "jpeg  n={n} workers={workers:>2}: {:.2} s, {:.1} img/s",
            secs,
            n as f64 / secs
        );
    }
    let n = 96;
    for workers in [8usize, 16, 24, 32] {
        let secs = run_batch(n, workers, &|i| {
            let o = format!("{out}/b_{i}.webp");
            compress_with_threads(jpg, &o, &opts(ImageFormat::WebP, 80, true), 1).unwrap();
        });
        println!(
            "webp  n={n} workers={workers:>2}: {:.2} s, {:.1} img/s",
            secs,
            n as f64 / secs
        );
    }
    let n = 48;
    for (workers, threads) in [(8usize, 4usize), (16, 2), (32, 1)] {
        let secs = run_batch(n, workers, &|i| {
            let o = format!("{out}/b_{i}.png");
            compress_with_threads(png, &o, &opts(ImageFormat::Png, 80, true), threads).unwrap();
        });
        println!(
            "png   n={n} workers={workers:>2} threads={threads}: {:.2} s, {:.1} img/s",
            secs,
            n as f64 / secs
        );
    }
    let n = 32;
    for (workers, threads) in [(8usize, 4usize), (16, 2), (32, 1)] {
        let secs = run_batch(n, workers, &|i| {
            let o = format!("{out}/b_{i}.avif");
            compress_with_threads(jpg, &o, &opts(ImageFormat::Avif, 80, true), threads).unwrap();
        });
        println!(
            "avif  n={n} workers={workers:>2} threads={threads}: {:.2} s, {:.2} img/s",
            secs,
            n as f64 / secs
        );
    }
}

fn orient(jpg: &str, out: &str) {
    use img_parts::{ImageEXIF, ImageICC};
    println!("== orientation and colour profile handling");
    // 400x200 landscape pixels tagged Orientation=6 (viewers show it as 200x400 portrait).
    let img = image::RgbImage::from_fn(400, 200, |x, _| image::Rgb([(x % 256) as u8, 80, 160]));
    let mut plain = Vec::new();
    image::DynamicImage::ImageRgb8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut plain),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let tiff: Vec<u8> = vec![
        0x49, 0x49, 0x2A, 0x00, 0x08, 0x00, 0x00, 0x00, // II, 42, IFD at 8
        0x01, 0x00, // one entry
        0x12, 0x01, 0x03, 0x00, 0x01, 0x00, 0x00, 0x00, 0x06, 0x00, 0x00,
        0x00, // Orientation = 6
        0x00, 0x00, 0x00, 0x00,
    ];
    let mut j = img_parts::jpeg::Jpeg::from_bytes(plain.into()).unwrap();
    j.set_exif(Some(tiff.into()));
    let src = format!("{out}/orient6.jpg");
    std::fs::write(&src, j.encoder().bytes()).unwrap();

    for strip in [true, false] {
        let o = format!("{out}/orient6_strip{strip}.jpg");
        compress_with_threads(&src, &o, &opts(ImageFormat::Jpeg, 80, strip), 1).unwrap();
        let outj = img_parts::jpeg::Jpeg::from_bytes(std::fs::read(&o).unwrap().into()).unwrap();
        let dims = image::image_dimensions(&o).unwrap();
        println!(
            "orientation-6 input 400x200, strip_metadata={strip}: output pixels {}x{}, exif kept={}",
            dims.0,
            dims.1,
            outj.exif().is_some()
        );
    }
    let inj = img_parts::jpeg::Jpeg::from_bytes(std::fs::read(jpg).unwrap().into()).unwrap();
    println!(
        "photo input icc profile bytes: {:?}",
        inj.icc_profile().map(|b| b.len())
    );
    for (fmt, ext) in [(ImageFormat::Jpeg, "jpg"), (ImageFormat::WebP, "webp")] {
        for strip in [true, false] {
            let o = format!("{out}/icc_strip{strip}.{ext}");
            compress_with_threads(jpg, &o, &opts(fmt.clone(), 80, strip), 1).unwrap();
            let bytes = std::fs::read(&o).unwrap();
            let has_icc = bytes.windows(4).any(|w| w == b"ICCP")
                || bytes.windows(11).any(|w| w == b"ICC_PROFILE");
            println!("photo -> {ext}, strip_metadata={strip}: icc profile kept={has_icc}");
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (mode, jpg, png, out) = (a[1].as_str(), a[2].as_str(), a[3].as_str(), a[4].as_str());
    std::fs::create_dir_all(out).unwrap();
    if matches!(mode, "single" | "all") {
        single(jpg, png, out);
    }
    if matches!(mode, "moz" | "all") {
        moz(jpg);
    }
    if matches!(mode, "webp" | "all") {
        webp_variants(jpg);
    }
    if matches!(mode, "batch" | "all") {
        batch(jpg, png, out);
    }
    if matches!(mode, "orient" | "all") {
        orient(jpg, out);
    }
}
