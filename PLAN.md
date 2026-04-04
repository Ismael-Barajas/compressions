# Compressions App — Implementation Plan

> **Stack:** Tauri v2 · React 19 · TypeScript · Rust · FFmpeg · Tailwind CSS · Zustand

---

## Architecture Overview

### Tech Stack Decisions

| Layer              | Technology                                    | Reason                                                           |
| ------------------ | --------------------------------------------- | ---------------------------------------------------------------- |
| Desktop framework  | Tauri v2                                      | Lightweight (~10-30 MB), native OS webview, Rust backend         |
| Frontend           | React 19 + TypeScript                         | Component model, strong typing, large ecosystem                  |
| Styling            | Tailwind CSS v3 + CSS custom properties       | Utility-first, dark mode trivial, design tokens via variables    |
| State              | Zustand v5                                    | Minimal, no providers, excellent TypeScript support              |
| Video compression  | FFmpeg (sidecar)                              | Gold standard — H.264, H.265, AV1 support                        |
| Image compression  | mozjpeg + oxipng + webp + ravif + gif (native Rust) | Better output than FFmpeg image encoders, no subprocess overhead |
| Progress streaming | Tauri Channels                                | Ordered, high-throughput (~10 Hz FFmpeg updates) vs Events       |
| Packaging          | Tauri bundler                                 | .exe/.msi (Windows) + .dmg (macOS) from one codebase             |

### Key Design Decisions

1. **Channels over Events** — FFmpeg progress updates at ~10 Hz; Channels deliver ordered data at high throughput
2. **Sequential video processing** — FFmpeg is CPU/IO intensive; one process at a time prevents resource contention
3. **Parallel image processing** — Images are fast; up to 4 concurrent tokio tasks for batch throughput
4. **Native Rust image crates** — mozjpeg/oxipng/webp/ravif/gif produce better output than FFmpeg image encoders
5. **Zustand** — Minimal, no provider wrappers, great TypeScript support, React 19 compatible

---

## Project Structure

```
compressions/
├── src/                              # React frontend
│   ├── main.tsx                      # ReactDOM render entry
│   ├── App.tsx                       # Root — theme provider + layout
│   ├── vite-env.d.ts
│   ├── styles/
│   │   └── globals.css               # Tailwind + CSS custom property tokens
│   ├── components/
│   │   ├── layout/
│   │   │   ├── Header.tsx            # App bar — title + theme toggle
│   │   │   └── AppShell.tsx          # Main grid — left file panel + right controls
│   │   ├── dropzone/
│   │   │   └── DropZone.tsx          # Drag-drop target + browse button
│   │   ├── file-list/
│   │   │   ├── FileList.tsx          # Scrollable list + toolbar + start button
│   │   │   └── FileItem.tsx          # Single file row — status, progress bar, result
│   │   ├── controls/
│   │   │   ├── VideoControls.tsx     # Codec, CRF, resolution, FPS, audio settings
│   │   │   ├── ImageControls.tsx     # Format, quality, resize, strip metadata
│   │   │   └── PresetSelector.tsx    # Built-in + user preset picker
│   │   └── output/
│   │       ├── OutputSettings.tsx    # Output directory picker
│   │       └── ResultsSummary.tsx    # Before/after totals + per-file table
│   ├── hooks/
│   │   └── useCompression.ts         # Orchestrates compress flow + Channel listeners
│   ├── stores/
│   │   └── compressionStore.ts       # Zustand store — all app state + actions
│   ├── types/
│   │   ├── compression.ts            # QueuedFile, VideoOptions, ImageOptions, etc.
│   │   └── presets.ts                # Preset type
│   └── lib/
│       ├── commands.ts               # Typed invoke() wrappers for all Tauri commands
│       └── fileUtils.ts              # getMediaType, formatFileSize, getOutputPath, etc.
│
├── src-tauri/                        # Rust backend
│   ├── Cargo.toml
│   ├── build.rs
│   ├── tauri.conf.json
│   ├── capabilities/
│   │   └── default.json              # shell, dialog, fs permissions
│   ├── binaries/                     # FFmpeg/ffprobe sidecars — gitignored
│   │   └── .gitkeep
│   ├── icons/                        # App icons — generated via `npx tauri icon`
│   └── src/
│       ├── main.rs                   # Desktop entry — calls lib::run()
│       ├── lib.rs                    # App builder — plugins, commands, managed state
│       ├── state.rs                  # AppState { active_jobs: HashMap<id, CommandChild> }
│       ├── types.rs                  # All shared types (Serialize + Deserialize)
│       ├── commands/
│       │   ├── mod.rs
│       │   ├── video.rs              # compress_video, compress_videos_batch, cancel_compression
│       │   ├── image.rs              # compress_image, compress_images_batch (with Channel progress)
│       │   ├── probe.rs              # probe_file, detect_media_type
│       │   └── presets.rs            # get_presets, save_preset, delete_preset, get_default_output_dir
│       ├── compression/
│       │   ├── mod.rs
│       │   ├── image.rs              # mozjpeg/oxipng/webp/ravif/gif encoding + EXIF metadata copy
│       │   └── progress.rs           # FFmpeg stderr parser (frame, time, speed)
│       ├── ffmpeg/
│       │   ├── mod.rs
│       │   ├── args.rs               # Build FFmpeg CLI argument vector
│       │   └── probe.rs              # ffprobe JSON parser (duration, resolution, codec)
│       └── presets/
│           ├── mod.rs
│           ├── definitions.rs        # 8 hardcoded built-in presets
│           └── storage.rs            # Read/write user presets.json in app_data_dir
│
├── scripts/
│   ├── download-ffmpeg.sh            # macOS/Linux FFmpeg binary downloader
│   └── download-ffmpeg.ps1           # Windows FFmpeg binary downloader
│
├── .github/
│   └── workflows/
│       └── release.yml               # CI — builds Windows .exe and macOS .dmg
│
├── package.json
├── vite.config.ts
├── tsconfig.json
├── tailwind.config.js
├── postcss.config.js
├── index.html
├── .gitignore
└── README.md
```

---

## Tauri Command API

| Command                  | Parameters                                    | Returns               | Description                                 |
| ------------------------ | --------------------------------------------- | --------------------- | ------------------------------------------- |
| `compress_video`         | `input, output, options, onProgress: Channel` | `CompressionResult`   | Single video with streaming progress        |
| `compress_videos_batch`  | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Sequential batch video                      |
| `compress_image`         | `input, output, options, onProgress: Channel` | `CompressionResult`   | Single image with started/completed events  |
| `compress_images_batch`  | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Parallel batch image with per-file events   |
| `cancel_compression`     | `jobId`                                       | `()`                  | Kill active FFmpeg process via CommandChild |
| `probe_file`             | `path`                                        | `FileInfo`            | File metadata — size, resolution, duration  |
| `detect_media_type`      | `path`                                        | `MediaType`           | Classify by extension → video/image         |
| `get_presets`            | —                                             | `Preset[]`            | Built-ins + user presets merged             |
| `save_preset`            | `preset`                                      | `()`                  | Persist user preset to JSON                 |
| `delete_preset`          | `id`                                          | `()`                  | Remove user preset (rejects built-ins)      |
| `get_default_output_dir` | —                                             | `string`              | Platform Videos/Downloads/Home dir          |

---

## Built-in Presets

| ID             | Name            | Type  | Key Settings                          |
| -------------- | --------------- | ----- | ------------------------------------- |
| `video-web`    | Web Optimized   | Video | H.264, CRF 28, 720p, AAC 128k         |
| `video-high`   | High Quality    | Video | H.265, CRF 20, original res, AAC 192k |
| `video-small`  | Small File Size | Video | H.265, CRF 32, 480p, AAC 96k          |
| `video-social` | Social Media    | Video | H.264, CRF 23, 1080p, AAC 128k        |
| `image-web`    | Web Optimized   | Image | WebP, quality 80, strip metadata      |
| `image-high`   | High Quality    | Image | PNG lossless (oxipng preset 3)        |
| `image-small`  | Small File Size | Image | AVIF, quality 60                      |
| `image-thumb`  | Thumbnail       | Image | JPEG, quality 70, 300px resize        |

---

## Dependencies

### Cargo.toml (Rust)

```toml
tauri = "2"
tauri-plugin-shell = "2"
tauri-plugin-dialog = "2"
tauri-plugin-fs = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uuid = { version = "1", features = ["v4"] }
tokio = { version = "1", features = ["full"] }
image = { version = "0.25", features = ["jpeg", "png", "webp", "gif"] }
mozjpeg = "0.10"          # MozJPEG (requires NASM assembler)
oxipng = "9"              # PNG lossless optimizer
webp = "0.3"              # WebP encoder (libwebp-sys)
ravif = "0.11"            # AVIF encoder (rav1e-based, slow first compile)
rgb = "0.8"               # Pixel format for ravif
gif = "0.13"              # Animated GIF decode/re-encode
img-parts = "0.4"         # EXIF copy for JPEG and WebP
thiserror = "2"
regex = "1"
log = "0.4"
env_logger = "0.11"
dirs-next = "2"
```

### package.json (Node/TypeScript)

```json
dependencies:
  @tauri-apps/api ^2
  @tauri-apps/plugin-dialog ^2
  @tauri-apps/plugin-fs ^2
  @tauri-apps/plugin-shell ^2
  lucide-react ^0.468
  react ^19
  react-dom ^19
  zustand ^5

devDependencies:
  @tauri-apps/cli ^2
  @types/react ^19
  @types/react-dom ^19
  @vitejs/plugin-react ^4
  tailwindcss ^3
  typescript ~5.7
  vite ^6
```

### Prerequisites

- Node.js 18+
- Rust 1.77.2+ (via rustup)
- NASM assembler (for mozjpeg: `choco install nasm` / `brew install nasm`)

---

## Implementation Phases

### ✅ Phase 1 — Project Scaffolding

Set up the project skeleton so the app launches with an empty window.

- [x] Initialize Vite + React + TypeScript project manually
- [x] Configure `vite.config.ts` with Tauri dev host support
- [x] Configure `tsconfig.json` with path aliases
- [x] Configure `tailwind.config.js` + `postcss.config.js`
- [x] Create `index.html`
- [x] Write `src/styles/globals.css` with CSS custom property design tokens
- [x] Write `src/main.tsx` entry point
- [x] Write `src/App.tsx` root with theme toggle
- [x] Write `src/vite-env.d.ts`
- [x] Create all TypeScript types (`src/types/compression.ts`, `src/types/presets.ts`)
- [x] Create Zustand store (`src/stores/compressionStore.ts`)
- [x] Create typed Tauri command wrappers (`src/lib/commands.ts`)
- [x] Create file utility functions (`src/lib/fileUtils.ts`)
- [x] Write all React components (Header, AppShell, DropZone, FileList, FileItem, VideoControls, ImageControls, PresetSelector, OutputSettings, ResultsSummary)
- [x] Configure `src-tauri/Cargo.toml` with all dependencies
- [x] Configure `src-tauri/tauri.conf.json` (window, bundle, sidecar, platform settings)
- [x] Configure `src-tauri/capabilities/default.json` (shell, dialog, fs permissions)
- [x] Write `src-tauri/build.rs`
- [x] Write all Rust source files (main.rs, lib.rs, state.rs, types.rs)
- [x] Write all Rust command modules (video.rs, image.rs, probe.rs, presets.rs)
- [x] Write compression modules (image.rs, progress.rs)
- [x] Write FFmpeg modules (args.rs, probe.rs)
- [x] Write preset modules (definitions.rs, storage.rs)
- [x] Create FFmpeg download scripts (`.sh` + `.ps1`)
- [x] Configure `.gitignore`
- [x] Update `README.md` with setup instructions
- [x] Run `npm install` — **142 packages, 0 vulnerabilities**
- [x] TypeScript type check — **✅ 0 errors**
- [x] Vite production build — **✅ built in 3.08s**

---

### ✅ Phase 2 — Wire Up Compression Flow

Connect the frontend to the Rust backend so compression actually runs.

- [x] Create `src/hooks/useCompression.ts`
  - [x] Build output path logic (same-as-source vs custom dir)
  - [x] Create Tauri `Channel<ProgressEvent>` before invoking commands
  - [x] Route `ProgressEvent.started` → set file status to `processing`
  - [x] Route `ProgressEvent.progress` → call `updateProgress(jobId, payload)`
  - [x] Route `ProgressEvent.completed` → call `markComplete(jobId, result)`
  - [x] Route `ProgressEvent.error` → call `markError(jobId, message)`
  - [x] Call `probeFile` for each dropped/browsed file to populate real size + resolution
  - [x] Separate video files and image files into two batches
  - [x] Invoke `compress_videos_batch` for video batch
  - [x] Invoke `compress_images_batch` for image batch
  - [x] Handle cancellation via `cancel_compression(jobId)`
  - [x] Set `isCompressing` flag at start/end
- [x] Wire "Start Compression" button in `FileList.tsx` to `useCompression`
- [x] Wire cancel button (per-file while processing) to `cancelCompression`
- [x] Call `probe_file` when files are dropped/browsed — update file sizes in store
- [x] Call `get_default_output_dir` on app load — set as default output dir
- [x] Add `scan_paths` Rust command — recursive directory walker for folder support
- [x] Folder drag-drop and browse-folder support in `DropZone.tsx`
- [x] "Add Folder" button with recursive scan in `FileList.tsx`

---

### ✅ Phase 3 — Rust Compilation & Bug Fixes

Get Rust compiling and fix any issues found.

- [x] Run `cargo check` inside `src-tauri/` — fix any compilation errors
- [x] Verify `State<'_, Mutex<AppState>>` cloning works in `compress_videos_batch`
- [x] Verify `Channel<ProgressEvent>` cloning works in batch command
- [x] Fix any serde rename issues between Rust enums and TypeScript string literals
- [x] Verify `dirs-next` crate resolves Video/Download dirs on both platforms
- [x] Run `npm run tauri dev` — verify app launches and window appears

---

### 🔲 Phase 4 — FFmpeg Integration

Download FFmpeg and verify video compression end-to-end.

- [x] Run `powershell scripts/download-ffmpeg.ps1` to download FFmpeg binaries
- [x] Verify binaries are named correctly (`ffmpeg-x86_64-pc-windows-msvc.exe`)
- [x] Test `compress_video` command with a sample MP4 file
- [x] Verify progress events reach the frontend (progress bar animates)
- [x] Verify output file is created and smaller than input
- [x] Test H.264, H.265, and AV1 codec selection
- [x] Test resolution scaling (720p, 480p)
- [x] Test frame rate limiting
- [x] Test audio codec options (AAC, Opus, Copy, None)
- [x] Test cancellation mid-compression
- [x] Test batch video compression (multiple files)

---

### ✅ Phase 5 — Image Compression Integration

Verify all image formats compress correctly and fix bugs found during testing.

- [x] Test JPEG compression (mozjpeg) with quality slider
- [x] Test PNG optimization (oxipng)
- [x] Test WebP encoding
- [x] Test AVIF encoding (ravif — slow first compile, fast thereafter)
- [x] Test resize with aspect ratio lock
- [x] Test strip metadata option
- [x] Test batch image compression (multiple files in parallel)
- [x] Verify before/after sizes in ResultsSummary

**Bugs fixed during Phase 5 testing:**

- [x] Progress bar stuck at 0% for all image formats — added `Channel<ProgressEvent>` to image commands, emitting `Started`/`Completed`/`Error` events; frontend shows animated indeterminate bar while processing
- [x] Output file larger than input — added size guard: if compressed output ≥ input size, original is copied to output path and result shows "Already optimized"
- [x] GIF loses animation — GIF inputs now bypass format selection and re-encode all frames via the `gif` crate, preserving animation, delays, and palette
- [x] Strip metadata toggle not implemented — JPEG and WebP now copy EXIF from source via `img_parts` when `strip_metadata = false`; PNG uses `oxipng::StripChunks::All/None` explicitly
- [x] AVIF metadata preservation — when `strip_metadata = false`, routes through FFmpeg sidecar (`libaom-av1 -map_metadata 0`) instead of ravif; falls back with error if codec unavailable
- [x] `ImageFormat` type missing `Gif` — added to both Rust enum and TypeScript union

---

### 🔲 Phase 6 — Polish & App Icons

Final refinements before packaging.

- [ ] Generate app icons — create a 1024×1024 source icon, run `npx tauri icon icon.png`
- [ ] Add toast/notification for errors (file open failure, unsupported format)
- [ ] Add deduplication of files already in queue (by path)
- [ ] Improve output filename conflict handling (append `_2`, `_3` if file exists)
- [ ] Fix the dynamic import warning for `fileUtils.ts` in Vite build
- [ ] Add keyboard shortcut: `Space` to start compression, `Escape` to cancel
- [ ] Validate output directory exists and is writable before starting
- [ ] Add "Open output folder" button in ResultsSummary
- [ ] Ensure `+faststart` flag only applies to `.mp4` outputs (already done in args.rs)
- [ ] Handle mixed batches (video + image files) in a single compress run

---

### 🔲 Phase 7 — CI/CD & Packaging

Automated cross-platform builds and releases.

- [ ] Create `.github/workflows/release.yml`
  - [ ] `build-windows` job on `windows-latest`
    - [ ] Install Rust stable
    - [ ] Install Node.js 20
    - [ ] Install NASM via Chocolatey
    - [ ] Run `download-ffmpeg.ps1`
    - [ ] `npm install` + `tauri-apps/tauri-action`
    - [ ] Upload `.exe` installer as release asset
  - [ ] `build-macos` job on `macos-latest`
    - [ ] Install Rust stable + `aarch64-apple-darwin` target
    - [ ] Install Node.js 20
    - [ ] Install NASM via Homebrew
    - [ ] Run `download-ffmpeg.sh`
    - [ ] `npm install` + `tauri-apps/tauri-action --target universal-apple-darwin`
    - [ ] Upload `.dmg` as release asset
  - [ ] Trigger on `push` to `v*` tags
- [ ] Add GitHub secrets documentation to README
  - [ ] Windows code signing: `TAURI_SIGNING_PRIVATE_KEY`
  - [ ] macOS signing: `APPLE_CERTIFICATE`, `APPLE_CERTIFICATE_PASSWORD`, `APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD`, `APPLE_TEAM_ID`
- [ ] Test `npm run tauri build` locally on Windows

---

## Known Challenges & Mitigations

| Challenge                                      | Mitigation                                                      |
| ---------------------------------------------- | --------------------------------------------------------------- |
| `mozjpeg` requires NASM assembler              | Document in README, add to CI install steps                     |
| `ravif`/`rav1e` slow first compile (~5-10 min) | Accept — incremental rebuilds are fast, cache `target/` in CI   |
| FFmpeg binaries are ~100 MB each, not in git   | `scripts/download-ffmpeg.*` — auto-downloaded in CI             |
| macOS FFmpeg dylib resolution issues           | Use statically-linked FFmpeg builds (BtbN GPL static)           |
| Tauri `DragDropEvent` fires twice (#14134)     | Deduplicate by tracking last drop timestamp (100ms window)      |
| No Rust cross-compilation for Tauri            | Platform-specific CI runners (`windows-latest`, `macos-latest`) |
| WebView2 missing on older Windows              | `downloadBootstrapper` mode in `tauri.conf.json`                |
| `State<'_>` cannot be cloned directly          | Pass `AppHandle` instead and re-acquire state lock per command  |

---

## Progress Tracking

| Phase                  | Status         | Notes                                                  |
| ---------------------- | -------------- | ------------------------------------------------------ |
| 1 — Scaffolding        | ✅ Complete    | Frontend builds, TS 0 errors, Vite ✅                  |
| 2 — Compression Flow   | ✅ Complete    | Hook wired, folder support added, TS 0 errors, Vite ✅ |
| 3 — Rust Compilation   | ✅ Complete    | Compiles cleanly                                       |
| 4 — FFmpeg Integration | ✅ Complete    | H.264/H.265/AV1, cancellation, batch tested            |
| 5 — Image Compression  | ✅ Complete    | All formats tested; 6 bugs found and fixed             |
| 6 — Polish             | 🔲 Not started |                                                        |
| 7 — CI/CD & Packaging  | 🔲 Not started |                                                        |

---

## Feature Expansion Plan (Phase 8+)

> Added 2026-04-03. These features extend the app beyond core compression.

### Decisions

- **AVIF decoding:** Route through FFmpeg sidecar (zero new build deps, already bundled)
- **PDF compression:** Ghostscript sidecar (best quality, industry standard, ~30MB binary)
- **Audio extraction UX:** Context menu on video files (right-click -> "Extract Audio")

---

### Phase 8 — Bug Fixes & Quick Wins

#### 8.1 AVIF Input Support (Bug Fix)

**Problem:** `image` crate in `Cargo.toml` only enables `["jpeg", "png", "webp", "gif"]`. `image::open()` at `src-tauri/src/compression/image.rs:20` fails for AVIF inputs. The UI falsely claims AVIF support.

**Solution:** Decode AVIF inputs via FFmpeg to a temp PNG before passing to the existing pipeline.

**Files to modify:**
- `src-tauri/src/compression/image.rs` — Add `is_avif()` check at the top of `compress()`. If input is AVIF, skip `image::open()` and accept a pre-decoded `DynamicImage` instead.
- `src-tauri/src/commands/image.rs` — Before calling `compress()`, detect AVIF input and shell out to FFmpeg: `ffmpeg -i input.avif -pix_fmt rgba temp.png`. Pass the temp PNG path to `compress()`, then clean up the temp file. `AppHandle` is already available for sidecar spawning.
- `src-tauri/src/commands/probe.rs` — For AVIF files, `image::image_dimensions()` also fails. Route AVIF dimension probing through FFprobe instead.

**Transparency:** FFmpeg preserves RGBA when outputting to PNG. `encode_avif()` at `image.rs:143` already uses `img.to_rgba8()`, so alpha channels flow through.

#### 8.2 Pixel Dimensions in Preview

**Problem:** File items only show name and size. Resolution is already probed by `probe_file` but not passed to the UI.

**Files to modify:**
- `src/types/compression.ts` — Add `resolution?: { width: number; height: number } | null` and `duration?: number | null` to `QueuedFile`.
- `src/stores/compressionStore.ts` — Extend `updateFileProbe` (line 86) to accept and store `resolution` and `duration`.
- `src/components/layout/AppShell.tsx` — Pass `resolution` and `duration` from probe result to `updateFileProbe`.
- `src/components/file-list/FileItem.tsx` — After file size, render `WxH` badge (e.g., `1920×1080`). For videos, also show duration as `mm:ss`.

#### 8.3 Subfolder Export Option

**Problem:** Output can only go to same directory or custom directory. Users want a "subfolder within source dir" option.

**Files to modify:**
- `src/types/compression.ts` — Add `type OutputMode = "sameDir" | "subfolder" | "customDir"`.
- `src/stores/compressionStore.ts` — Replace `sameAsSource: boolean` with `outputMode: OutputMode` (default `"sameDir"`) and `subfolderName: string` (default `"compressed"`).
- `src/hooks/useCompression.ts` — Update `getOutputDir` (line 42) for three modes: `sameDir` → `getParentDir()`, `subfolder` → `getParentDir() + sep + subfolderName`, `customDir` → `outputDir`.
- `src/components/output/OutputSettings.tsx` — Replace checkbox with 3 radio options. Show editable subfolder name when "subfolder" selected.
- `src-tauri/src/commands/image.rs` and `video.rs` — Add `std::fs::create_dir_all()` on output parent dir before writing.

#### 8.4 Customizable Output File Name Format

**Problem:** Hardcoded `{name}_compressed.{ext}` pattern at `fileUtils.ts:53`.

**Files to modify:**
- `src/stores/compressionStore.ts` — Add `outputTemplate: string` (default `"{name}_compressed"`) and `setOutputTemplate`. Persist to localStorage.
- `src/lib/fileUtils.ts` — Refactor `getOutputFileName()` to accept a template. Tokens: `{name}`, `{date}` (YYYY-MM-DD), `{time}` (HH-MM-SS), `{quality}`. Extension auto-determined. GIF-always-GIF rule preserved.
- `src/hooks/useCompression.ts` — Read `outputTemplate` from store and pass to `buildOutputPath`.
- `src/components/output/OutputSettings.tsx` — Add template text input with live preview + tooltip for available tokens.

---

### Phase 9 — New Media Capabilities

#### 9.1 MP3 Audio Extraction (Context Menu)

**Approach:** Right-click video → "Extract Audio". Uses FFmpeg with `-vn` flag.

**New files:**
- `src-tauri/src/commands/audio.rs` — `extract_audio` and `extract_audio_batch`. Spawns FFmpeg sidecar, parses progress via Channel.
- `src/components/controls/AudioControls.tsx` — Format selector (MP3, AAC, FLAC, Opus, WAV), bitrate dropdown.

**Files to modify:**
- `src-tauri/src/types.rs` — Add `AudioExtractionOptions { format: AudioOutputFormat, bitrate: Option<String>, sample_rate: Option<u32> }` and `AudioOutputFormat` enum.
- `src-tauri/src/ffmpeg/args.rs` — Add `build_audio_extraction_args()`: `-i input -vn -c:a {codec} -b:a {bitrate} output.{ext}`.
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — Register commands.
- `src/types/compression.ts` — Add `AudioOptions`, `AudioOutputFormat` types.
- `src/lib/commands.ts` — Add `extractAudio()`, `extractAudioBatch()` wrappers.
- `src/stores/compressionStore.ts` — Add `audioOptions` state.
- `src/components/file-list/FileItem.tsx` — Add right-click context menu on video files with "Extract Audio" option. Shows popover with AudioControls + Start button.
- `src/lib/fileUtils.ts` — Handle audio output extensions.

#### 9.2 GIF Compression & Video-to-GIF

**Part A: Video-to-GIF** via FFmpeg two-pass palette approach.

**New files:**
- `src-tauri/src/commands/gif.rs` — `convert_video_to_gif` command. Two-pass FFmpeg with palette generation. Progress uses existing `progress.rs` parser.
- `src/components/controls/GifControls.tsx` — FPS, max width, color count, dither mode.

**Files to modify:**
- `src-tauri/src/types.rs` — Add `GifConversionOptions { fps, width, max_colors, dither }`.
- `src-tauri/src/ffmpeg/args.rs` — Add `build_video_to_gif_args()` using FFmpeg complex filter.
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — Register.
- `src/types/compression.ts`, `src/lib/commands.ts`, `src/stores/compressionStore.ts` — Types, wrapper, state.
- `src/components/file-list/FileItem.tsx` — Add "Convert to GIF" to video context menu.

**Part B: GIF Optimization** — Enhance `encode_gif()` at `image.rs:168`.

- `src-tauri/Cargo.toml` — Add `imagequant = "4"` for palette optimization.
- `src-tauri/src/compression/image.rs` — Add color quantization + frame deduplication in `encode_gif()`.

**Real-time progress for video-to-GIF:** Automatic — FFmpeg emits `time=` during GIF encoding, `progress.rs` already parses it.

#### 9.3 PDF Compression (Ghostscript Sidecar)

**New files:**
- `src-tauri/src/commands/pdf.rs` — `compress_pdf` and `compress_pdfs_batch`. Spawns Ghostscript: `gs -sDEVICE=pdfwrite -dCompatibilityLevel=1.4 -dPDFSETTINGS=/{preset} -dNOPAUSE -dBATCH -sOutputFile=output.pdf input.pdf`. Progress is indeterminate (Started/Completed only).
- `src/components/controls/PdfControls.tsx` — Quality preset selector (Screen/Ebook/Printer/Prepress), optional DPI input.

**Files to modify:**
- `src-tauri/tauri.conf.json` — Add Ghostscript to `externalBin` sidecar list.
- `src-tauri/binaries/` — Bundle Ghostscript platform-specific binaries.
- `src-tauri/src/types.rs` — Add `MediaType::Pdf`, `PdfOptions`, `PdfQuality` enum.
- `src-tauri/src/commands/scan.rs` — Add `"pdf"` to extension list.
- `src-tauri/src/commands/probe.rs` — Add `MediaType::Pdf` detection. Returns file size only.
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — Register.
- `src/types/compression.ts` — Add `"pdf"` to `MediaType`, `PdfOptions`, `PdfQuality`.
- `src/lib/fileUtils.ts` — Add `.pdf` to extensions.
- `src/lib/commands.ts` — Add `compressPdf`, `compressPdfsBatch`.
- `src/stores/compressionStore.ts` — Add `pdfOptions`.
- `src/hooks/useCompression.ts` — Add PDF processing path (sequential, like video).
- `src/components/layout/AppShell.tsx` — Show PdfControls when PDFs in queue.
- `src/components/dropzone/DropZone.tsx` — Add `.pdf` to filters and dropzone text.
- `src/components/file-list/FileItem.tsx` — Add PDF icon (FileText from lucide-react).

---

### Phase 10 — UX Enhancements

#### 10.1 Add Files While Compressing

**Problem:** `isCompressing` flag disables all file-adding UI. Files added after compression starts are never processed.

**Files to modify:**
- `src/components/file-list/FileList.tsx` — Remove `disabled={isCompressing}` from "Add More" and "Add Folder". Keep on "Clear All".
- `src/hooks/useCompression.ts` — Refactor to drain-loop pattern:
  ```
  while (true) {
    const queued = getState().files.filter(f => f.status === "queued");
    if (queued.length === 0) break;
    // process batch... await Promise.allSettled(promises);
  }
  ```
- `src/components/layout/AppShell.tsx` — Move drag-drop listener from DropZone to AppShell (or shared hook) so files can be dropped onto file list during compression.

#### 10.2 Clipboard Paste Support

**New files:**
- `src-tauri/src/commands/clipboard.rs` — `read_clipboard_files() -> Vec<String>` (uses `arboard` for CF_HDROP on Windows), `save_clipboard_image(data: Vec<u8>) -> String` (saves to temp file).
- `src/hooks/useClipboardPaste.ts` — Listens for `paste` on document. Handles image blobs (save to temp) and file references (invoke Rust for native clipboard).

**Files to modify:**
- `src-tauri/Cargo.toml` — Add `arboard = "3"`.
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — Register clipboard commands.
- `src/lib/commands.ts` — Add `readClipboardFiles()`, `saveClipboardImage()`.
- `src/App.tsx` or `src/components/layout/AppShell.tsx` — Install `useClipboardPaste` hook at app level.

---

### Phase 11 — Persistence & Observability

#### 11.1 Compression History

**New files:**
- `src-tauri/src/history/mod.rs` and `storage.rs` — JSON file in AppData (`history.json`). Follows `presets/storage.rs` pattern. Cap at 1000 entries.
- `src-tauri/src/commands/history.rs` — `get_history`, `clear_history`.
- `src/components/history/HistoryPanel.tsx` — Modal with scrollable list. Shows timestamp, filename, sizes, savings %, duration. Search/filter. "Clear History" button.
- `src/stores/historyStore.ts` — Separate Zustand store.

**Files to modify:**
- `src-tauri/src/types.rs` — Add `HistoryEntry { id, timestamp, input_path, output_path, input_size, output_size, duration_ms, media_type, success, error }`.
- `src-tauri/src/commands/image.rs` and `video.rs` — Append to history after each compression.
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — Register.
- `src/types/compression.ts` or `src/types/history.ts` — `HistoryEntry` TS interface.
- `src/lib/commands.ts` — Add wrappers.
- Header component — Add "History" button (Clock icon from lucide-react).

#### 11.2 Log Viewer & Enhanced Logging

**New files:**
- `src-tauri/src/commands/logs.rs` — `get_log_path()`, `read_logs(lines)`, `open_log_file()`.
- `src/components/logs/LogViewer.tsx` — Modal with color-coded entries (INFO=blue, WARN=yellow, ERROR=red). Level filter, search, auto-refresh, "Open Log File" button.
- `src/stores/logStore.ts` — Separate Zustand store.

**Files to modify:**
- `src-tauri/Cargo.toml` — Replace `log + env_logger` with `tracing + tracing-subscriber + tracing-appender`. JSON format for file output, dual stderr + rolling daily file.
- `src-tauri/src/lib.rs` — Initialize `tracing_subscriber` with dual output.
- All Rust files using `log::info!()` etc. — Replace with `tracing::info!()` with structured fields.
- `src-tauri/src/commands/mod.rs`, `src-tauri/src/lib.rs` — Register log commands.
- `src/lib/commands.ts` — Add wrappers.
- Header component — Add "Logs" button (Terminal icon from lucide-react).

---

### Phase 12 — Integration & Final Polish

- [ ] Ensure all new media types (PDF, audio) work with output mode options (subfolder, custom dir, name template)
- [ ] Verify clipboard paste + add-during-compression work together
- [ ] Test AVIF round-trip (AVIF input → any format, any format → AVIF output) with transparency
- [ ] Test history recording for all compression types (video, image, PDF, audio extraction, GIF conversion)
- [ ] Test log output for all operations
- [ ] Update DropZone text to accurately reflect all supported formats
- [ ] Add keyboard shortcuts: `Ctrl+V` paste, `Space` start, `Escape` cancel

---

### Verification Plan

| # | Test | Steps |
|---|------|-------|
| 1 | AVIF input | Drop AVIF (with transparency) → compress to PNG → verify alpha preserved |
| 2 | Dimensions | Add mixed files → verify WxH shown for each in file list |
| 3 | Subfolder | Compress with "subfolder" mode → verify `compressed/` dir created |
| 4 | Name template | Set `{name}_{date}` → compress → verify output filename matches |
| 5 | Audio extraction | Right-click video → Extract Audio → verify MP3 output |
| 6 | Video-to-GIF | Right-click video → Convert to GIF → verify animated GIF + progress bar |
| 7 | GIF optimization | Drop large GIF → compress → verify smaller output |
| 8 | PDF | Drop PDFs → compress with Ebook preset → verify smaller output |
| 9 | Clipboard paste | Copy file in Explorer → Ctrl+V → verify in queue. Copy screenshot → Ctrl+V → verify |
| 10 | Queue during compression | Start 5 files → drag 3 more → verify they queue and process |
| 11 | History | Compress files → open History → verify entries with stats |
| 12 | Logs | Run operations → open Log Viewer → verify structured entries |

---

### Cross-Cutting: Files Affected by Multiple Features

Every new Tauri command must be registered in three places:
1. `src-tauri/src/commands/mod.rs` (module declaration)
2. `src-tauri/src/lib.rs` (invoke_handler macro)
3. `src/lib/commands.ts` (typed invoke wrapper)

Adding `MediaType::Pdf` touches: `types.rs`, `compression.ts`, `fileUtils.ts`, `scan.rs`, `probe.rs`, `DropZone.tsx`, `FileList.tsx`, `FileItem.tsx`, `AppShell.tsx`, `useCompression.ts`, `definitions.rs`, `PresetSelector.tsx`.
