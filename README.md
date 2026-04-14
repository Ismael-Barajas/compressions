# Compressions

A cross-platform desktop app for compressing videos, images, audio, and PDFs. Built with Tauri v2, React, TypeScript, and Rust.

## Features

### Video Compression

- **Codecs** — H.264, H.265/HEVC, AV1 (via SVT-AV1)
- **Hardware acceleration** — Auto-detects NVIDIA NVENC (Windows/Linux) and Apple VideoToolbox (macOS) with automatic software fallback
- **Quality control** — CRF 0–51 or bitrate override
- **Resolution scaling** — Original, 4K, 1080p, 720p, 480p, or custom (aspect ratio preserved)
- **Frame rate** — Original, 60, 30, 24, 15 fps, or custom
- **Audio track** — AAC, Opus, copy original, or remove; bitrate 64k–320k
- **FastStart** — Moves moov atom to front for web streaming (MP4)
- **Input formats** — MP4, MKV, AVI, MOV, WebM, FLV, WMV, M4V, TS

### Image Compression

- **Output formats** — JPEG (MozJPEG), PNG (oxipng), WebP, AVIF (ravif), GIF, or Original (preserve format)
- **Quality** — 1–100 per format
- **Resize** — Fit or Exact mode, lock/unlock aspect ratio, width presets
- **Metadata** — Strip or preserve EXIF data
- **Parallel processing** — Up to 8 concurrent encode tasks
- **Input formats** — JPG, PNG, WebP, AVIF, BMP, TIFF, GIF

### PDF Compression

- **Presets** — Screen (72 DPI), Ebook (150 DPI), Printer (300 DPI), Prepress (300 DPI)
- **DPI override** — 72, 150, 200, or 300
- Powered by Ghostscript

### Audio Compression

- **Output formats** — MP3, AAC, Opus, FLAC, WAV, or Original (preserve format)
- **Bitrate** — Presets (64k–320k) or custom input (lossy formats)
- **Sample rate** — Original, 48000, 44100, or 22050 Hz
- **Original format** — Keeps source format with new encoding settings; niche formats (WMA, APE, DTS, etc.) fall back to MP3
- **Audio wave progress** — Animated waveform equalizer during compression
- **Input formats** — MP3, AAC, M4A, FLAC, WAV, OGG, Opus, WMA, AIFF, APE, ALAC, AC3, DTS, PCM, AMR

### Audio Extraction

- Extract audio from any video via right-click context menu
- **Formats** — MP3, AAC, FLAC, Opus, WAV
- **Bitrate** — 64k–320k (lossy formats)
- **Sample rate** — Original, 48000, 44100, or 22050 Hz

### Video-to-GIF Conversion

- Two-pass palette-based encoding for optimal color quality
- **Controls** — FPS (5–30), max width, color count (16–256), dither mode (Floyd-Steinberg, Bayer, None)

### Presets

- **Built-in video presets** — Web Optimized, High Quality, Small File Size, Social Media
- **Built-in image presets** — Web Optimized, High Quality, Small File Size, Thumbnail
- Save and delete custom presets

### Workflow

- Drag-and-drop files and folders
- **Clipboard paste** — `Ctrl/Cmd+V` to add files or paste screenshots directly
- **Add files during compression** — queue new files while a batch is running
- Batch processing with per-file progress bars and ETA
- **Per-file cancellation** during compression
- Output modes: same folder, subfolder, or custom directory
- Customizable filename templates (`{name}`, `{date}`, `{time}`)
- Automatic `_2`, `_3` suffixes to prevent overwrites
- Original kept if compressed output would be larger
- Before/after file size comparison
- File thumbnails (toggle on/off)
- Right-click context menu on video files (Extract Audio, Convert to GIF)
- **Keyboard shortcuts** — `Space` to start, `Escape` to cancel, `Ctrl/Cmd+V` to paste
- Dark/light theme with system preference detection
- Built-in auto-updater

### Observability

- **Compression history** — Searchable log of past compressions with size savings and duration
- **Application log viewer** — Filterable by level (ERROR, WARN, INFO, DEBUG, TRACE) with search
- **Input validation** — All compression parameters validated before processing

## Prerequisites

- [Node.js](https://nodejs.org/) 18+
- [Rust](https://rustup.rs/) 1.77.2+
- [NASM](https://www.nasm.us/) (required for mozjpeg compilation)
  - Windows: `choco install nasm`
  - macOS: `brew install nasm`

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

## Build

```bash
npm run tauri build
```

Output:

- Windows: `src-tauri/target/release/bundle/nsis/Compressions_1.0.0_x64-setup.exe`
- macOS: `src-tauri/target/release/bundle/dmg/Compressions_1.0.0_aarch64.dmg`

## Tech Stack

| Layer     | Technology                                      |
| --------- | ----------------------------------------------- |
| Framework | Tauri v2                                        |
| Frontend  | React 19 + TypeScript + Tailwind CSS            |
| Backend   | Rust                                            |
| Video     | FFmpeg sidecar                                  |
| Audio     | FFmpeg sidecar                                  |
| Image     | mozjpeg, oxipng, webp, ravif, gif (native Rust) |
| PDF       | Ghostscript sidecar                             |
| State     | Zustand                                         |

## Third-Party Licenses

This application bundles the following external binaries as sidecars:

- **FFmpeg / FFprobe** — Licensed under [GPL v2+](https://www.ffmpeg.org/legal.html). Source available at [ffmpeg.org](https://ffmpeg.org/).
- **Ghostscript** — Licensed under [AGPL v3](https://www.ghostscript.com/licensing/). Source available at [ghostscript.com](https://www.ghostscript.com/).

The Compressions application code itself is MIT-licensed. The bundled sidecar binaries retain their original licenses.

## License

MIT
