# Compressions

A cross-platform desktop app for compressing videos, images, audio, and PDFs. Built with Tauri v2, React, TypeScript, and Rust.

## Features

### Video Compression

- **Codecs** — H.264, H.265/HEVC, AV1 (via SVT-AV1)
- **Hardware acceleration** — Detects NVIDIA NVENC (Windows/Linux) and Apple VideoToolbox (macOS), verifies each encoder with a test encode at startup, and falls back to software automatically (an encoder that fails mid-batch is disabled for the rest of the session)
- **Quality control** — CRF 0–51 or bitrate override
- **Resolution scaling** — Original, 4K, 1080p, 720p, or 480p (downscale only, aspect ratio preserved, even dimensions enforced)
- **Frame rate** — Original, 60, 30, 24, or 15 fps
- **Audio track** — AAC, Opus, copy original, or remove; bitrate 64k–320k
- **FastStart** — Moves the moov atom to the front for web streaming (MP4)
- **Progress** — Percent, encoder speed, and ETA parsed from FFmpeg's progress stream
- **Input formats** — MP4, MKV, AVI, MOV, WebM, FLV, WMV, M4V, TS

### Image Compression

- **Output formats** — JPEG (MozJPEG), PNG (oxipng), WebP, AVIF (ravif), or Original (keep the source format; BMP/TIFF/HEIC become PNG/JPEG since they have no encoder)
- **Animated GIF** — GIF inputs are re-quantized frame by frame with imagequant and stay animated GIFs
- **Quality** — 1–100 for lossy formats; PNG is lossless
- **Resize** — Fit (one dimension, aspect locked) or Exact (both dimensions); SIMD-accelerated Lanczos3 scaling
- **Metadata** — Strip or preserve EXIF
- **Parallel processing** — Up to 8 files at once, with encoder threads budgeted across the batch
- **Input formats** — JPG, PNG, WebP, AVIF, BMP, TIFF, GIF, HEIC, HEIF (AVIF/HEIC are decoded through FFmpeg)

### PDF Compression

- **Presets** — Screen (72 DPI), Ebook (150 DPI), Printer (300 DPI), Prepress (300 DPI)
- **Image DPI override** — Default, 72, 150, 200, or 300
- **Parallel batches** — A few PDFs are processed at once (Ghostscript is single-threaded)
- Powered by Ghostscript

### Audio Compression

- **Output formats** — MP3, AAC, Opus, FLAC, WAV, or Original (keep the source format; niche formats such as WMA, APE, and DTS fall back to MP3)
- **Bitrate** — Presets (64k–320k) or custom input (lossy formats)
- **Sample rate** — Original, 96000, 48000, 44100, or 22050 Hz
- **Parallel batches** — Several files encode at once (the encoders are single-threaded)
- **Audio wave progress** — Animated waveform equalizer during compression
- **Input formats** — MP3, AAC, M4A, FLAC, WAV, OGG, Opus, WMA, AIFF, APE, ALAC, AC3, DTS, PCM, AMR

### Audio Extraction

- Extract audio from a video via the right-click menu, or from every queued video via the Tools tab
- **Formats** — MP3, AAC, Opus, FLAC, WAV
- **Bitrate** — 64k–320k (lossy formats)
- **Sample rate** — Original, 48000, 44100, or 22050 Hz

### Video-to-GIF Conversion

- Palette-based encoding (palettegen + paletteuse) in a single FFmpeg pass
- **Controls** — FPS (5–30), width (Original, 640, 480, 320, 240 px), color count (16–256), dither mode (Floyd-Steinberg, Bayer, None)
- Convert one video via the right-click menu, or every queued video via the Tools tab

### Presets

- **Video** — Web Optimized, High Quality, Small File Size, Social Media
- **Image** — Web Optimized, High Quality, Small File Size, Thumbnail
- Changing any setting switches to Custom

### Workflow

- Drag-and-drop files and folders (folders are scanned recursively; hidden directories are skipped)
- **Add / Folder** buttons and the empty-state drop zone accept every supported type
- **Clipboard paste** — `Ctrl/Cmd+V` to add copied files or paste a screenshot directly
- **Add files during compression** — new files join the running queue
- **Queue controls** — Pause, Resume, and Cancel All; per-file cancel and retry
- Batch progress bar and per-file progress with ETA
- Smaller files first for video, PDF, and audio; larger first for images to keep parallel workers busy
- Output modes: same folder, subfolder, or custom directory
- Filename templates (`{name}`, `{date}`, `{time}`); one timestamp per batch
- Automatic `_2`, `_3` suffixes to prevent overwrites
- Original kept if the compressed output would be larger (near-instant on filesystems with reflinks)
- Before/after size comparison and a batch results summary
- File thumbnails (toggle on/off), generated for the rows on screen
- **Keyboard shortcuts** — `Space` to start, `Escape` to cancel processing files, `Ctrl/Cmd+V` to paste
- Dark/light theme with system preference detection; bundled fonts (no network at launch)
- Built-in auto-updater
- Close prompt while compressions are running

### Observability

- **Compression history** — Searchable log of the last 1000 compressions with size savings and duration
- **Application log viewer** — Filterable by level (ERROR, WARN, INFO, DEBUG, TRACE) with search; seven days of daily log files are kept
- **Input validation** — All compression parameters are validated before processing

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77.2+
- [NASM](https://www.nasm.us/) (required for mozjpeg compilation)
  - Windows: `choco install nasm`
  - macOS: `brew install nasm`
- Linux additionally needs the Tauri system packages: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev patchelf libgtk-3-dev`

## Setup

```bash
# Install frontend dependencies
npm install

# Download FFmpeg sidecars
bash scripts/download-ffmpeg.sh        # macOS / Linux / Git Bash
powershell scripts/download-ffmpeg.ps1 # Windows PowerShell

# Download Ghostscript sidecars (required for PDF compression)
bash scripts/download-gs.sh            # macOS / Linux / Git Bash
powershell scripts/download-gs.ps1     # Windows PowerShell (requires 7-Zip)
```

> **Windows note:** `download-gs.ps1` requires [7-Zip](https://www.7-zip.org/) to extract the Ghostscript installer (`choco install 7zip`).

### Start development

**macOS / Linux:**

```bash
npm run tauri dev
```

**Windows PowerShell** (Rust and NASM must be on PATH):

```powershell
$env:PATH += ";$env:USERPROFILE\.cargo\bin;C:\Program Files\NASM"; npm run tauri dev
```

Dependencies are compiled with optimizations even in dev builds, so image encoding stays fast while iterating.

## Build

```bash
npm run tauri build
```

Output (`<version>` is the value in `package.json`):

- Windows: `src-tauri/target/release/bundle/nsis/Compressions_<version>_x64-setup.exe`
- macOS: `src-tauri/target/release/bundle/dmg/Compressions_<version>_aarch64.dmg`
- Linux: `src-tauri/target/release/bundle/` (deb, rpm, AppImage)

## Development

```bash
npm run test:run        # Frontend unit tests (vitest)
npm run test:coverage   # ...with coverage
npm run test:bench      # Frontend micro-benchmarks
npx tsc --noEmit        # Typecheck

cargo test  --manifest-path src-tauri/Cargo.toml --lib     # Rust tests
cargo clippy --manifest-path src-tauri/Cargo.toml --lib -- -D warnings
cargo bench --manifest-path src-tauri/Cargo.toml --bench compression_bench   # Image encoder benchmarks (criterion)

npm run version:patch   # Bump package.json, Cargo.toml, tauri.conf.json together
npm run version:check   # Verify the three manifests agree
```

CI runs the frontend tests, typecheck, Rust tests (Linux/macOS/Windows), clippy, and rustfmt on every push to `main` and every pull request. Benchmarks run on demand via the workflow's manual trigger. Releases are built and published by the `Release` workflow when a `v*` tag is pushed.

## Project layout

| Path | What lives there |
| --- | --- |
| `src/` | React UI. `stores/` (zustand), `lib/compressionController.ts` (queue drain loop and IPC wiring), `lib/commands.ts` (typed Tauri commands), `components/` |
| `src-tauri/src/commands/` | Tauri commands per media type; `job.rs` is the shared sidecar runner (claim output, spawn, stream progress, finish, history) |
| `src-tauri/src/compression/` | Native image encoders and the FFmpeg progress parser |
| `src-tauri/src/ffmpeg/` | FFmpeg/ffprobe argument builders, probing, and hardware-encoder detection |
| `src-tauri/src/media.rs` | Supported extensions (single source of truth, exposed to the frontend over IPC) |
| `src-tauri/benches/` | Criterion benchmarks for the image pipeline |
| `docs/OPTIMIZATION_PLAN.md` | Performance audit and the rationale behind the current architecture |

## Tech Stack

| Layer     | Technology                                                          |
| --------- | ------------------------------------------------------------------- |
| Framework | Tauri v2                                                            |
| Frontend  | React 19 + TypeScript + Tailwind CSS + zustand + TanStack Virtual   |
| Backend   | Rust (tokio)                                                        |
| Video     | FFmpeg sidecar                                                      |
| Audio     | FFmpeg sidecar                                                      |
| Image     | mozjpeg, oxipng, webp, ravif, gif + imagequant, fast_image_resize   |
| PDF       | Ghostscript sidecar                                                 |

## Third-Party Licenses

This application bundles the following external binaries as sidecars:

- **FFmpeg / FFprobe** — Licensed under [GPL v2+](https://www.ffmpeg.org/legal.html). Source available at [ffmpeg.org](https://ffmpeg.org/).
- **Ghostscript** — Licensed under [AGPL v3](https://www.ghostscript.com/licensing/). Source available at [ghostscript.com](https://www.ghostscript.com/).

Bundled fonts: [Bricolage Grotesque](https://github.com/ateliertriay/bricolage) and [IBM Plex Mono](https://github.com/IBM/plex), both under the SIL Open Font License.

The Compressions application code itself is MIT-licensed. The bundled sidecar binaries retain their original licenses.

## License

MIT
