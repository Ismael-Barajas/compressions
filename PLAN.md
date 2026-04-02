# Compressions App — Implementation Plan

> **Stack:** Tauri v2 · React 19 · TypeScript · Rust · FFmpeg · Tailwind CSS · Zustand

---

## Architecture Overview

### Tech Stack Decisions

| Layer | Technology | Reason |
|---|---|---|
| Desktop framework | Tauri v2 | Lightweight (~10-30 MB), native OS webview, Rust backend |
| Frontend | React 19 + TypeScript | Component model, strong typing, large ecosystem |
| Styling | Tailwind CSS v3 + CSS custom properties | Utility-first, dark mode trivial, design tokens via variables |
| State | Zustand v5 | Minimal, no providers, excellent TypeScript support |
| Video compression | FFmpeg (sidecar) | Gold standard — H.264, H.265, AV1 support |
| Image compression | mozjpeg + oxipng + webp + ravif (native Rust) | Better output than FFmpeg image encoders, no subprocess overhead |
| Progress streaming | Tauri Channels | Ordered, high-throughput (~10 Hz FFmpeg updates) vs Events |
| Packaging | Tauri bundler | .exe/.msi (Windows) + .dmg (macOS) from one codebase |

### Key Design Decisions

1. **Channels over Events** — FFmpeg progress updates at ~10 Hz; Channels deliver ordered data at high throughput
2. **Sequential video processing** — FFmpeg is CPU/IO intensive; one process at a time prevents resource contention
3. **Parallel image processing** — Images are fast; up to 4 concurrent tokio tasks for batch throughput
4. **Native Rust image crates** — mozjpeg/oxipng/webp/ravif produce better output than FFmpeg image encoders
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
│       │   ├── image.rs              # compress_image, compress_images_batch
│       │   ├── probe.rs              # probe_file, detect_media_type
│       │   └── presets.rs            # get_presets, save_preset, delete_preset, get_default_output_dir
│       ├── compression/
│       │   ├── mod.rs
│       │   ├── image.rs              # mozjpeg/oxipng/webp/ravif encoding
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

| Command | Parameters | Returns | Description |
|---|---|---|---|
| `compress_video` | `input, output, options, onProgress: Channel` | `CompressionResult` | Single video with streaming progress |
| `compress_videos_batch` | `files[], options, onProgress: Channel` | `CompressionResult[]` | Sequential batch video |
| `compress_image` | `input, output, options` | `CompressionResult` | Single image (blocking in spawn_blocking) |
| `compress_images_batch` | `files[], options` | `CompressionResult[]` | Parallel batch image (≤4 concurrent) |
| `cancel_compression` | `jobId` | `()` | Kill active FFmpeg process via CommandChild |
| `probe_file` | `path` | `FileInfo` | File metadata — size, resolution, duration |
| `detect_media_type` | `path` | `MediaType` | Classify by extension → video/image |
| `get_presets` | — | `Preset[]` | Built-ins + user presets merged |
| `save_preset` | `preset` | `()` | Persist user preset to JSON |
| `delete_preset` | `id` | `()` | Remove user preset (rejects built-ins) |
| `get_default_output_dir` | — | `string` | Platform Videos/Downloads/Home dir |

---

## Built-in Presets

| ID | Name | Type | Key Settings |
|---|---|---|---|
| `video-web` | Web Optimized | Video | H.264, CRF 28, 720p, AAC 128k |
| `video-high` | High Quality | Video | H.265, CRF 20, original res, AAC 192k |
| `video-small` | Small File Size | Video | H.265, CRF 32, 480p, AAC 96k |
| `video-social` | Social Media | Video | H.264, CRF 23, 1080p, AAC 128k |
| `image-web` | Web Optimized | Image | WebP, quality 80, strip metadata |
| `image-high` | High Quality | Image | PNG lossless (oxipng preset 3) |
| `image-small` | Small File Size | Image | AVIF, quality 60 |
| `image-thumb` | Thumbnail | Image | JPEG, quality 70, 300px resize |

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
image = { version = "0.25", features = ["jpeg", "png", "webp"] }
mozjpeg = "0.10"          # MozJPEG (requires NASM assembler)
oxipng = "9"              # PNG lossless optimizer
webp = "0.3"              # WebP encoder (libwebp-sys)
ravif = "0.11"            # AVIF encoder (rav1e-based, slow first compile)
rgb = "0.8"               # Pixel format for ravif
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

### 🔲 Phase 2 — Wire Up Compression Flow
Connect the frontend to the Rust backend so compression actually runs.

- [ ] Create `src/hooks/useCompression.ts`
  - [ ] Build output path logic (same-as-source vs custom dir)
  - [ ] Create Tauri `Channel<ProgressEvent>` before invoking commands
  - [ ] Route `ProgressEvent.started` → set file status to `processing`
  - [ ] Route `ProgressEvent.progress` → call `updateProgress(jobId, payload)`
  - [ ] Route `ProgressEvent.completed` → call `markComplete(jobId, result)`
  - [ ] Route `ProgressEvent.error` → call `markError(jobId, message)`
  - [ ] Call `probeFile` for each dropped/browsed file to populate real size + resolution
  - [ ] Separate video files and image files into two batches
  - [ ] Invoke `compress_videos_batch` for video batch
  - [ ] Invoke `compress_images_batch` for image batch
  - [ ] Handle cancellation via `cancel_compression(jobId)`
  - [ ] Set `isCompressing` flag at start/end
- [ ] Wire "Start Compression" button in `FileList.tsx` to `useCompression`
- [ ] Wire cancel button (per-file while processing) to `cancelCompression`
- [ ] Call `probe_file` when files are dropped/browsed — update file sizes in store
- [ ] Call `get_default_output_dir` on app load — set as default output dir

---

### 🔲 Phase 3 — Rust Compilation & Bug Fixes
Get Rust compiling and fix any issues found.

- [ ] Install Rust via rustup (user prerequisite)
- [ ] Install NASM (user prerequisite — `choco install nasm`)
- [ ] Run `cargo check` inside `src-tauri/` — fix any compilation errors
- [ ] Verify `State<'_, Mutex<AppState>>` cloning works in `compress_videos_batch`
- [ ] Verify `Channel<ProgressEvent>` cloning works in batch command
- [ ] Fix any serde rename issues between Rust enums and TypeScript string literals
- [ ] Verify `dirs-next` crate resolves Video/Download dirs on both platforms
- [ ] Run `npm run tauri dev` — verify app launches and window appears

---

### 🔲 Phase 4 — FFmpeg Integration
Download FFmpeg and verify video compression end-to-end.

- [ ] Run `powershell scripts/download-ffmpeg.ps1` to download FFmpeg binaries
- [ ] Verify binaries are named correctly (`ffmpeg-x86_64-pc-windows-msvc.exe`)
- [ ] Test `compress_video` command with a sample MP4 file
- [ ] Verify progress events reach the frontend (progress bar animates)
- [ ] Verify output file is created and smaller than input
- [ ] Test H.264, H.265, and AV1 codec selection
- [ ] Test resolution scaling (720p, 480p)
- [ ] Test frame rate limiting
- [ ] Test audio codec options (AAC, Opus, Copy, None)
- [ ] Test cancellation mid-compression
- [ ] Test batch video compression (multiple files)

---

### 🔲 Phase 5 — Image Compression Integration
Verify all four image formats compress correctly.

- [ ] Test JPEG compression (mozjpeg) with quality slider
- [ ] Test PNG optimization (oxipng)
- [ ] Test WebP encoding
- [ ] Test AVIF encoding (ravif — slow first compile, fast thereafter)
- [ ] Test resize with aspect ratio lock
- [ ] Test strip metadata option
- [ ] Test batch image compression (multiple files in parallel)
- [ ] Verify before/after sizes in ResultsSummary

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

| Challenge | Mitigation |
|---|---|
| `mozjpeg` requires NASM assembler | Document in README, add to CI install steps |
| `ravif`/`rav1e` slow first compile (~5-10 min) | Accept — incremental rebuilds are fast, cache `target/` in CI |
| FFmpeg binaries are ~100 MB each, not in git | `scripts/download-ffmpeg.*` — auto-downloaded in CI |
| macOS FFmpeg dylib resolution issues | Use statically-linked FFmpeg builds (BtbN GPL static) |
| Tauri `DragDropEvent` fires twice (#14134) | Deduplicate by tracking last drop timestamp (100ms window) |
| No Rust cross-compilation for Tauri | Platform-specific CI runners (`windows-latest`, `macos-latest`) |
| WebView2 missing on older Windows | `downloadBootstrapper` mode in `tauri.conf.json` |
| `State<'_>` cannot be cloned directly | Pass `AppHandle` instead and re-acquire state lock per command |

---

## Progress Tracking

| Phase | Status | Notes |
|---|---|---|
| 1 — Scaffolding | ✅ Complete | Frontend builds, TS 0 errors, Vite ✅ |
| 2 — Compression Flow | 🔲 Not started | useCompression hook needed |
| 3 — Rust Compilation | 🔲 Blocked | Requires Rust + NASM install |
| 4 — FFmpeg Integration | 🔲 Not started | Blocked on Phase 3 |
| 5 — Image Compression | 🔲 Not started | Blocked on Phase 3 |
| 6 — Polish | 🔲 Not started | |
| 7 — CI/CD & Packaging | 🔲 Not started | |
