# Compressions

A cross-platform desktop app for compressing videos, images, and PDFs. Built with Tauri v2, React, TypeScript, and Rust.

## Features

### Compression

- **Video compression** — H.264, H.265/HEVC, AV1 via FFmpeg; control CRF, resolution, frame rate, audio codec/bitrate
- **Image compression** — JPEG (MozJPEG), PNG (oxipng), WebP, AVIF, animated GIF; resize, strip metadata
- **PDF compression** — Ghostscript-powered with Screen / Ebook / Printer / Prepress quality presets and DPI override
- **Audio extraction** — Extract MP3, AAC, FLAC, Opus, or WAV from any video via right-click context menu
- **Video-to-GIF conversion** — Two-pass palette FFmpeg approach with FPS, width, color count, and dither controls

### Performance

- **Hardware-accelerated video encoding** — Auto-detects VideoToolbox (macOS) and NVENC (Windows/Linux) with automatic software fallback
- **Optimized FFmpeg presets** — `-preset fast` for H.264/H.265, `-preset 7` for SVT-AV1
- **Parallel image processing** — Concurrent batch encoding with semaphore-based concurrency limiting
- **Tuned AVIF encoder** — ravif speed 7 with thread capping to prevent contention in batches

### Workflow

- Drag-and-drop file and folder input
- **Clipboard paste** — `Ctrl/Cmd+V` to add files from clipboard or paste screenshots directly
- **Add files during compression** — queue new files while a batch is processing
- Built-in presets (Web Optimized, High Quality, Small File Size, Social Media) with custom preset save/delete
- Batch processing with per-file progress bars (indeterminate for PDF)
- **Per-file cancellation** during compression
- Output modes: same folder, subfolder, or custom directory
- Customizable output filename templates (`{name}`, `{date}`, `{time}`)
- **Output filename conflict resolution** — automatic `_2`, `_3` suffixes to prevent overwrites
- Automatic "already optimized" detection — original kept if output would be larger
- Before/after file size comparison
- **Keyboard shortcuts** — `Space` to start, `Escape` to cancel, `Ctrl/Cmd+V` to paste
- Dark/light theme

### Observability

- **Compression history** — Searchable log of all past compressions with size savings and duration
- **Application log viewer** — Filterable by level (ERROR, WARN, INFO, DEBUG, TRACE) with search
- **Input validation** — All compression parameters validated server-side before processing

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

### Generate app icons

```bash
npx tauri icon path/to/your-icon.png
```

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

- Windows: `src-tauri/target/release/bundle/nsis/Compressions_0.1.0_x64-setup.exe`
- macOS: `src-tauri/target/release/bundle/dmg/Compressions_0.1.0_aarch64.dmg`

## Tech Stack

| Layer     | Technology                                      |
| --------- | ----------------------------------------------- |
| Framework | Tauri v2                                        |
| Frontend  | React 19 + TypeScript + Tailwind CSS            |
| Backend   | Rust                                            |
| Video     | FFmpeg sidecar                                  |
| Image     | mozjpeg, oxipng, webp, ravif, gif (native Rust) |
| PDF       | Ghostscript sidecar                             |
| State     | Zustand                                         |

## License

MIT
