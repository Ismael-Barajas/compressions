# Compressions — Claude Code Context

## Project

Desktop media compression app. Tauri v2 + React + TypeScript frontend, Rust backend, FFmpeg/Ghostscript sidecars.

## Current Status

**See [PLAN.md](PLAN.md) for full implementation plan and progress tracking.**

Phase 8 (bug fixes & quick wins) is complete. Phase 9+ (new media types, UX, persistence) is in progress — resume from **Phase 9.1: MP3 Audio Extraction**.

## Stack

- **Frontend:** React 19, TypeScript, Tailwind CSS, Zustand
- **Backend:** Rust (Tauri v2), FFmpeg/FFprobe sidecars, Ghostscript sidecar (planned)
- **Image encoding:** mozjpeg, oxipng, webp, ravif, gif crates (native Rust)
- **State:** Zustand store (`src/stores/compressionStore.ts`)
- **Progress:** Tauri `Channel<ProgressEvent>` from Rust → frontend

## Key Conventions

- Every new Tauri command must be registered in 3 places: `commands/mod.rs`, `lib.rs`, and `src/lib/commands.ts`
- Videos compress sequentially (FFmpeg sidecar, CPU-bound); images compress in parallel (native Rust)
- Output path logic lives in `src/hooks/useCompression.ts` (`getOutputDirForFile`)
- File name template logic lives in `src/lib/fileUtils.ts` (`getOutputFileName`)
- `create_dir_all` is called in both `commands/image.rs` and `commands/video.rs` before writing output

## Dev Setup

```bash
# Install dependencies
npm install

# Download FFmpeg sidecars (Windows)
powershell scripts/download-ffmpeg.ps1

# Download FFmpeg sidecars (macOS/Linux)
bash scripts/download-ffmpeg.sh

# Run dev server
npm run tauri dev

# Build
npm run tauri build
```

Prerequisites: Node 18+, Rust (rustup), NASM (`choco install nasm` / `brew install nasm`)
