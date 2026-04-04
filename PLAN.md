# Compressions App — Implementation Plan

> **Stack:** Tauri v2 · React 19 · TypeScript · Rust · FFmpeg · Ghostscript · Tailwind CSS · Zustand

---

## Architecture

| Layer     | Technology                               | Reason                                                        |
| --------- | ---------------------------------------- | ------------------------------------------------------------- |
| Framework | Tauri v2                                 | Lightweight, native webview, Rust backend                     |
| Frontend  | React 19 + TypeScript + Tailwind CSS     | Component model, strong typing, utility-first styling         |
| State     | Zustand v5                               | Minimal, no providers, excellent TypeScript support           |
| Video     | FFmpeg sidecar                           | H.264, H.265, AV1; audio extraction; video-to-GIF            |
| Image     | mozjpeg + oxipng + webp + ravif + gif    | Better output than FFmpeg encoders, no subprocess overhead    |
| PDF       | Ghostscript sidecar                      | Industry-standard PDF recompression, font/color optimization  |
| Progress  | Tauri Channels                           | Ordered, high-throughput (~10 Hz) vs Events                   |
| Packaging | Tauri bundler                            | .exe/.msi (Windows) + .dmg (macOS) from one codebase         |

**Key design decisions:**
- **Sequential** processing for video and PDF (CPU/IO bound); **parallel** for images (up to 4 concurrent tokio tasks)
- Ghostscript resource files (`gs-res/`) bundled alongside sidecar; resolved via `CARGO_MANIFEST_DIR` (dev) or `resource_dir()` (prod)
- Output path logic centralized in `useCompression.ts`; filename templates in `fileUtils.ts`
- Every new Tauri command must be registered in 3 places: `commands/mod.rs`, `lib.rs`, `src/lib/commands.ts`

---

## Project Structure

```
compressions/
├── src/
│   ├── components/
│   │   ├── controls/
│   │   │   ├── VideoControls.tsx     # Codec, CRF, resolution, FPS, audio
│   │   │   ├── ImageControls.tsx     # Format, quality, resize, strip metadata
│   │   │   ├── AudioControls.tsx     # Format, bitrate, sample rate
│   │   │   ├── GifControls.tsx       # FPS, width, colors, dither
│   │   │   ├── PdfControls.tsx       # Quality preset, DPI override
│   │   │   └── PresetSelector.tsx    # Built-in + user preset picker
│   │   ├── dropzone/
│   │   │   └── DropZone.tsx          # Drag-drop + browse (video/image/PDF)
│   │   ├── file-list/
│   │   │   ├── FileList.tsx          # Scrollable list + toolbar + start button
│   │   │   └── FileItem.tsx          # Row: icon, progress, result, context menu
│   │   ├── layout/
│   │   │   ├── AppShell.tsx          # Left panel (files) + right panel (controls)
│   │   │   └── Header.tsx            # Title + theme toggle
│   │   └── output/
│   │       ├── OutputSettings.tsx    # Mode, dir, subfolder name, filename template
│   │       └── ResultsSummary.tsx    # Before/after totals + per-file table
│   ├── hooks/
│   │   └── useCompression.ts         # Orchestrates all compress flows + channels
│   ├── lib/
│   │   ├── commands.ts               # Typed invoke() wrappers for all Tauri commands
│   │   └── fileUtils.ts              # getMediaType, buildOutputPath, getOutputFileName
│   ├── stores/
│   │   └── compressionStore.ts       # Zustand store — all app state + actions
│   └── types/
│       ├── compression.ts            # QueuedFile, all Options types, ProgressEvent
│       └── presets.ts                # Preset type
├── src-tauri/
│   ├── tauri.conf.json               # externalBin (ffmpeg, ffprobe, gs), resources
│   ├── tauri.windows.conf.json       # Windows-only: bundles gsdll64.dll
│   ├── capabilities/default.json     # shell, dialog, fs permissions
│   ├── binaries/                     # Sidecar binaries + gs-res/ — gitignored
│   └── src/
│       ├── lib.rs                    # App builder — plugins + invoke_handler
│       ├── state.rs                  # AppState { active_jobs: HashMap<id, (child, path)> }
│       ├── types.rs                  # All shared Rust types (Serialize/Deserialize)
│       ├── commands/
│       │   ├── mod.rs
│       │   ├── video.rs              # compress_video(s_batch), cancel_compression
│       │   ├── image.rs              # compress_image(s_batch)
│       │   ├── audio.rs              # extract_audio(batch)
│       │   ├── gif.rs                # convert_video_to_gif(batch)
│       │   ├── pdf.rs                # compress_pdf(s_batch), resolve_gs_resource_dir
│       │   ├── probe.rs              # probe_file, detect_media_type
│       │   ├── scan.rs               # scan_paths (recurse dirs, filter by extension)
│       │   └── presets.rs            # get/save/delete presets, get_default_output_dir
│       ├── compression/
│       │   ├── image.rs              # Native encoding: mozjpeg/oxipng/webp/ravif/gif
│       │   └── progress.rs           # FFmpeg stderr parser (frame, time, speed → %)
│       ├── ffmpeg/
│       │   ├── args.rs               # FFmpeg + GIF arg builders
│       │   └── probe.rs              # ffprobe JSON → duration, resolution, codec
│       └── presets/
│           ├── definitions.rs        # Built-in presets
│           └── storage.rs            # Read/write presets.json in app_data_dir
└── scripts/
    ├── download-ffmpeg.sh / .ps1     # Download ffmpeg + ffprobe sidecars
    └── download-gs.sh / .ps1         # Download gs binary + Resource/lib/iccprofiles
```

---

## Tauri Command API

| Command                        | Parameters                                    | Returns               | Description                                     |
| ------------------------------ | --------------------------------------------- | --------------------- | ----------------------------------------------- |
| `compress_video`               | `input, output, options, onProgress: Channel` | `CompressionResult`   | Single video with streaming progress            |
| `compress_videos_batch`        | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Sequential batch video                          |
| `compress_image`               | `input, output, options, onProgress: Channel` | `CompressionResult`   | Single image with started/completed events      |
| `compress_images_batch`        | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Parallel batch image with per-file events       |
| `cancel_compression`           | `jobId`                                       | `()`                  | Kill active FFmpeg process via CommandChild     |
| `extract_audio`                | `input, output, options, onProgress: Channel` | `CompressionResult`   | Extract audio track via FFmpeg `-vn`            |
| `extract_audio_batch`          | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Sequential batch audio extraction               |
| `convert_video_to_gif`         | `input, output, options, onProgress: Channel` | `CompressionResult`   | Two-pass FFmpeg palette GIF conversion          |
| `convert_videos_to_gif_batch`  | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Sequential batch GIF conversion                 |
| `compress_pdf`                 | `input, output, options, onProgress: Channel` | `CompressionResult`   | Ghostscript recompression (indeterminate)       |
| `compress_pdfs_batch`          | `files[], options, onProgress: Channel`       | `CompressionResult[]` | Sequential batch PDF                            |
| `resolve_gs_resource_dir`      | —                                             | `string`              | Resolves gs-res/ path (dev vs prod)             |
| `probe_file`                   | `path`                                        | `FileInfo`            | File metadata — size, resolution, duration      |
| `detect_media_type`            | `path`                                        | `MediaType`           | Classify by extension → video/image/pdf         |
| `scan_paths`                   | `paths[]`                                     | `string[]`            | Recurse dirs, filter by supported extensions    |
| `get_presets`                  | —                                             | `Preset[]`            | Built-ins + user presets merged                 |
| `save_preset`                  | `preset`                                      | `()`                  | Persist user preset to JSON                     |
| `delete_preset`                | `id`                                          | `()`                  | Remove user preset (rejects built-ins)          |
| `get_default_output_dir`       | —                                             | `string`              | Platform Videos/Downloads/Home dir             |

---

## Built-in Presets

| ID             | Name            | Type  | Key Settings                          |
| -------------- | --------------- | ----- | ------------------------------------- |
| `video-web`    | Web Optimized   | Video | H.264, CRF 28, 720p, AAC 128k        |
| `video-high`   | High Quality    | Video | H.265, CRF 20, original res, AAC 192k |
| `video-small`  | Small File Size | Video | H.265, CRF 32, 480p, AAC 96k         |
| `video-social` | Social Media    | Video | H.264, CRF 23, 1080p, AAC 128k       |
| `image-web`    | Web Optimized   | Image | WebP, quality 80, strip metadata     |
| `image-high`   | High Quality    | Image | PNG lossless (oxipng preset 3)       |
| `image-small`  | Small File Size | Image | AVIF, quality 60                     |
| `image-thumb`  | Thumbnail       | Image | JPEG, quality 70, 300px resize       |

---

## Key Dependencies

**Rust (Cargo.toml):** `tauri 2`, `tauri-plugin-shell/dialog/fs 2`, `serde + serde_json`, `uuid v4`, `tokio full`, `image 0.25`, `mozjpeg 0.10` (needs NASM), `oxipng 9`, `webp 0.3`, `ravif 0.11` (slow first compile), `imagequant 4`, `gif 0.13`, `img-parts 0.4`, `thiserror 2`, `regex 1`, `dirs-next 2`

**Node (package.json):** `@tauri-apps/api ^2`, `@tauri-apps/plugin-dialog/fs/shell ^2`, `lucide-react`, `react ^19`, `zustand ^5`

**Prerequisites:** Node 18+, Rust (rustup), NASM (`choco install nasm` / `brew install nasm`)

---

## Implementation Status

| Phase                      | Status      | Notes                                                       |
| -------------------------- | ----------- | ----------------------------------------------------------- |
| 1 — Scaffolding            | ✅ Complete | TS 0 errors, Vite build passes                              |
| 2 — Compression Flow       | ✅ Complete | Hook wired, folder support, output modes, name templates    |
| 3 — Rust Compilation       | ✅ Complete | Compiles cleanly                                            |
| 4 — FFmpeg Integration     | ✅ Complete | H.264/H.265/AV1, cancellation, batch tested                 |
| 5 — Image Compression      | ✅ Complete | All formats; 6 bugs fixed (GIF animation, size guard, EXIF) |
| 6 — Polish                 | 🔲 Deferred | App icons, dedup, conflict handling, keyboard shortcuts     |
| 7 — CI/CD & Packaging      | 🔲 Deferred | GitHub Actions release workflow, code signing               |
| 8 — Bug Fixes & Quick Wins | ✅ Complete | AVIF input, dimensions in UI, subfolder/template output     |
| 9 — New Media Capabilities | ✅ Complete | Audio extraction, video-to-GIF, GIF optimization, PDF (GS)  |
| **10 — UX Enhancements**   | ✅ Complete | 10.1 Add files while compressing, 10.2 Clipboard paste     |
| 11 — Persistence           | 🔨 In Progress | 11.1 History ✅, 11.2 Log viewer planned                   |
| 12 — Final Polish          | 🔲 Planned  | Cross-cutting integration, keyboard shortcuts               |

---

## Phase 10 — UX Enhancements

### 10.1 Add Files While Compressing

**Problem:** `isCompressing` flag disables all file-adding UI. Files added after compression starts are never processed.

- `src/components/file-list/FileList.tsx` — Remove `disabled={isCompressing}` from "Add More" and "Add Folder". Keep on "Clear All".
- `src/hooks/useCompression.ts` — Refactor to drain-loop pattern:
  ```
  while (true) {
    const queued = getState().files.filter(f => f.status === "queued");
    if (queued.length === 0) break;
    await Promise.allSettled(processBatch(queued));
  }
  ```
- `src/components/layout/AppShell.tsx` — Move drag-drop listener to AppShell level so files can be dropped onto file list during compression.

### 10.2 Clipboard Paste Support

- **New:** `src-tauri/src/commands/clipboard.rs` — `read_clipboard_files()` (CF_HDROP via `arboard`), `save_clipboard_image()` (save to temp file)
- **New:** `src/hooks/useClipboardPaste.ts` — Listens for `paste` on document; handles image blobs and file references
- `src-tauri/Cargo.toml` — Add `arboard = "3"`
- `src-tauri/src/commands/mod.rs`, `lib.rs` — Register clipboard commands
- `src/lib/commands.ts` — Add `readClipboardFiles()`, `saveClipboardImage()`
- `src/components/layout/AppShell.tsx` — Install `useClipboardPaste` at app level

---

## Phase 11 — Persistence & Observability

### 11.1 Compression History

- **New:** `src-tauri/src/history/mod.rs` + `storage.rs` — JSON in AppData (`history.json`), cap 1000 entries, follows `presets/storage.rs` pattern
- **New:** `src-tauri/src/commands/history.rs` — `get_history`, `clear_history`
- **New:** `src/components/history/HistoryPanel.tsx` — Modal: timestamp, filename, sizes, savings %, duration. Search/filter + clear button.
- **New:** `src/stores/historyStore.ts`
- `src-tauri/src/types.rs` — Add `HistoryEntry { id, timestamp, input_path, output_path, input_size, output_size, duration_ms, media_type, success, error }`
- `src-tauri/src/commands/image.rs`, `video.rs`, `audio.rs`, `gif.rs`, `pdf.rs` — Append to history after each compression
- `src/types/compression.ts` — Add `HistoryEntry` TS interface
- `src/lib/commands.ts` — Add wrappers
- `src/components/layout/Header.tsx` — Add "History" button (Clock icon)

### 11.2 Log Viewer

- **New:** `src-tauri/src/commands/logs.rs` — `get_log_path()`, `read_logs(lines)`, `open_log_file()`
- **New:** `src/components/logs/LogViewer.tsx` — Modal: color-coded entries (INFO/WARN/ERROR), level filter, search, auto-refresh
- **New:** `src/stores/logStore.ts`
- `src-tauri/Cargo.toml` — Replace `log + env_logger` with `tracing + tracing-subscriber + tracing-appender` (JSON file + stderr dual output)
- `src-tauri/src/lib.rs` — Initialize `tracing_subscriber` with dual output
- All Rust files — Replace `log::info!()` with `tracing::info!()` with structured fields
- `src/components/layout/Header.tsx` — Add "Logs" button (Terminal icon)

---

## Phase 12 — Final Polish

- [ ] All media types work with all output modes (subfolder, custom dir, name template)
- [ ] Clipboard paste + add-during-compression work together
- [ ] AVIF round-trip (AVIF input → any format, any format → AVIF) with transparency
- [ ] History recorded for all types (video, image, PDF, audio, GIF)
- [ ] Log output verified for all operations
- [ ] Update DropZone text to reflect all supported formats
- [ ] Keyboard shortcuts: `Ctrl+V` paste, `Space` start, `Escape` cancel
- [ ] Phase 6 deferred items: app icons, file dedup, output filename conflict (`_2`, `_3`)

---

## Verification Plan

| # | Test                 | Steps                                                                             |
| - | -------------------- | --------------------------------------------------------------------------------- |
| 1 | AVIF input           | Drop AVIF (with transparency) → compress to PNG → verify alpha preserved          |
| 2 | Dimensions           | Add mixed files → verify WxH shown for each in file list                         |
| 3 | Subfolder            | Compress with "subfolder" mode → verify `compressed/` dir created                |
| 4 | Name template        | Set `{name}_{date}` → compress → verify output filename matches                  |
| 5 | Audio extraction     | Right-click video → Extract Audio → verify MP3 output                           |
| 6 | Video-to-GIF         | Right-click video → Convert to GIF → verify animated GIF + progress bar         |
| 7 | GIF optimization     | Drop large GIF → compress → verify smaller output                               |
| 8 | PDF                  | Drop PDFs → compress with Ebook preset → verify smaller output                  |
| 9 | Clipboard paste      | Copy file in Explorer → Ctrl+V → verify in queue. Copy screenshot → verify      |
| 10 | Queue during compress | Start 5 files → drag 3 more → verify they queue and process                    |
| 11 | History              | Compress files → open History → verify entries with stats                       |
| 12 | Logs                 | Run operations → open Log Viewer → verify structured entries                    |

---

## Known Challenges

| Challenge                                      | Mitigation                                                       |
| ---------------------------------------------- | ---------------------------------------------------------------- |
| `mozjpeg` requires NASM assembler              | Document in README; add to CI install steps                      |
| `ravif`/`rav1e` slow first compile (~5-10 min) | Accept; cache `target/` in CI                                    |
| FFmpeg binaries ~100 MB, not in git            | `scripts/download-ffmpeg.*` — auto-downloaded in CI             |
| macOS FFmpeg dylib resolution                  | Use statically-linked builds (BtbN GPL static)                   |
| Tauri `DragDropEvent` fires twice (#14134)     | Deduplicate by tracking last drop timestamp (100ms window)       |
| No Rust cross-compilation for Tauri            | Platform-specific CI runners (`windows-latest`, `macos-latest`)  |
| WebView2 missing on older Windows              | `downloadBootstrapper` mode in `tauri.conf.json`                 |
| Ghostscript licensed AGPL v3                   | App must remain open source or obtain a commercial license       |
