# Compressions

A cross-platform desktop app for compressing videos and images. Built with Tauri v2, React, TypeScript, and Rust.

## Features

- Drag-and-drop file input
- Video compression with H.264, H.265/HEVC, and AV1 codecs via FFmpeg
- Image compression with JPEG (MozJPEG), PNG (oxipng), WebP, and AVIF
- Advanced controls: quality/CRF, resolution, frame rate, audio codec/bitrate
- Built-in presets (Web Optimized, High Quality, Small File Size, Social Media)
- Batch processing with per-file progress
- Before/after file size comparison
- Dark/light theme
- Saves to user-selected output location

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

# Download FFmpeg binaries for your platform
# macOS / Git Bash:
bash scripts/download-ffmpeg.sh

# Windows PowerShell:
powershell scripts/download-ffmpeg.ps1
```

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

| Layer | Technology |
|---|---|
| Framework | Tauri v2 |
| Frontend | React 19 + TypeScript + Tailwind CSS |
| Backend | Rust |
| Video | FFmpeg (sidecar) |
| Image | mozjpeg, oxipng, webp, ravif (native Rust) |
| State | Zustand |

## License

MIT
