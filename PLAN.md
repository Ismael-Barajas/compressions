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

## Implementation Status

| Phase                      | Status      | Notes                                                       |
| -------------------------- | ----------- | ----------------------------------------------------------- |
| 1–5 — Core App             | ✅ Complete | Scaffolding, compression flow, Rust, FFmpeg, image formats  |
| 6 — Polish                 | 🔲 Deferred | App icons, dedup, conflict handling, keyboard shortcuts     |
| 7 — CI/CD & Packaging      | 🔲 Deferred | GitHub Actions release workflow, code signing               |
| 8 — Bug Fixes & Quick Wins | ✅ Complete | AVIF input, dimensions in UI, subfolder/template output     |
| 9 — New Media Capabilities | ✅ Complete | Audio extraction, video-to-GIF, GIF optimization, PDF (GS)  |
| 10 — UX Enhancements       | ✅ Complete | Add-during-compress, clipboard paste                        |
| 11 — Persistence           | ✅ Complete | History panel, log viewer                                   |
| **12 — Testing**           | ✅ Complete | Unit tests, integration tests, perf benchmarks, CI          |
| 13 — Final Polish          | 🔲 Planned  | Cross-cutting integration, keyboard shortcuts               |

---

## Phase 13 — Final Polish

- [ ] All media types work with all output modes (subfolder, custom dir, name template)
- [ ] Clipboard paste + add-during-compression work together
- [ ] AVIF round-trip (AVIF input → any format, any format → AVIF) with transparency
- [ ] History recorded for all types (video, image, PDF, audio, GIF)
- [ ] Log output verified for all operations
- [ ] Update DropZone text to reflect all supported formats
- [ ] Keyboard shortcuts: `Ctrl+V` paste, `Space` start, `Escape` cancel
- [ ] Phase 6 deferred items: app icons, file dedup, output filename conflict (`_2`, `_3`)

---

## Phase 12 — Testing & Performance Benchmarking

### 13.1 Infrastructure Setup

**Frontend (Vitest):**
- Install: `vitest`, `@vitest/coverage-v8`, `jsdom`
- Add scripts to `package.json`: `test`, `test:run`, `test:coverage`, `test:bench`
- Add `test` block to `vite.config.ts` (globals, jsdom, coverage)

**Rust (Criterion):**
- Add to `Cargo.toml` dev-deps: `criterion 0.5` (html_reports), `tempfile 3`
- Add `[[bench]] name = "compression_bench" harness = false`

### 13.2 Rust Unit Tests (Pure Functions)

`#[cfg(test)]` blocks added to each file — no visibility changes needed.

| File | Tests |
|------|-------|
| `compression/progress.rs` | `parse_progress_line`: typical line, time-only, capped at 100%, zero duration, garbled → None, ETA accuracy |
| `ffmpeg/args.rs` | `build_video_args`: all 3 codecs (AV1 requires `-pix_fmt yuv420p`), resolution filter, audio modes, MP4 faststart, bitrate/fps. `build_audio_extraction_args`: codec mapping, no `-b:a` for lossless. `build_gif_palette_args` + `build_gif_encode_args`: dither modes, filtergraph |
| `commands/pdf.rs` | `build_gs_args`: each quality preset, DPI args present/absent, resource dir `-I` paths, always-present flags |
| `commands/probe.rs` | `detect_media_type`: all extensions, case insensitivity, unknown/missing → Err |
| `commands/clipboard.rs` | `urldecode`: %20, passthrough, empty, truncated % → no panic. `hex_val`: 0-9, a-f, A-F, invalid |
| `logging/setup.rs` | `parse_log_line`: INFO/WARN/ERROR/DEBUG with target, empty/garbage → None |

### 13.3 Frontend Unit Tests (Pure Functions + Store)

| File | Tests |
|------|-------|
| `src/lib/fileUtils.test.ts` | All 10 functions: `getMediaType` (all extensions, case), `getFileName` (Unix/Windows), `formatFileSize` (0 B, boundaries, 1.5 KB), `getSavingsPercent` (div-by-zero guard), `getOutputFileName` ({name}/{date}/{time} substitution, GIF preservation), `getAudioExtension`, `getParentDir`, `buildOutputPath` |
| `src/stores/compressionStore.test.ts` | `addFiles` deduplication, state transitions (queued→processing→complete/error), `retryFile`, `updateProgress` by jobId, theme toggle + localStorage |

### 13.4 Rust Integration Tests (Image Compression)

In `compression/image.rs` `#[cfg(test)]` block. Generate images programmatically with `image` crate; use `tempfile` for output.

- JPEG: output is valid JPEG, smaller than raw
- PNG: valid PNG output
- WebP: output starts with RIFF/WEBP magic bytes
- AVIF: output exists and non-empty
- Resize: dimensions ≤ target, aspect ratio preserved
- Quality extremes (1, 100): both produce valid output
- Missing input: returns `Err`

### 13.5 Performance Benchmarks

**Rust (`src-tauri/benches/compression_bench.rs`):**
| Benchmark | Target |
|-----------|--------|
| `parse_progress_line` × 10,000 | < 10ms |
| JPEG q80 (1920×1080) | < 200ms |
| PNG (1920×1080) | < 1000ms |
| WebP q75 (1920×1080) | < 500ms |
| AVIF q60 (1920×1080) | < 3000ms |
| JPEG q80 at 256², 1024², 1920×1080, 3840×2160 | scaling profile |

**Frontend (`src/lib/fileUtils.bench.ts`, `src/stores/compressionStore.bench.ts`):**
| Benchmark | Target |
|-----------|--------|
| `getOutputFileName` × 10,000 | < 50ms |
| `getMediaType` × 100,000 | < 100ms |
| `addFiles` with 1,000 files | < 50ms |
| `updateProgress` on 500-file store | < 5ms |

### 13.6 CI/CD Pipeline (`.github/workflows/test.yml`)

| Job | Runs On | What |
|-----|---------|------|
| `frontend-tests` | ubuntu-latest | `npm ci` → `vitest run` → coverage upload |
| `rust-tests` | ubuntu + macos + windows | Install NASM + Tauri deps → `cargo test --lib` |
| `rust-benchmarks` | ubuntu (main push only) | `cargo bench` → artifact upload |

Cross-platform Rust tests catch mozjpeg/oxipng/ravif platform-specific compile issues.

### Files to Create

| File | Purpose |
|------|---------|
| `src/lib/fileUtils.test.ts` | Frontend utility unit tests |
| `src/stores/compressionStore.test.ts` | Store state transition tests |
| `src/lib/fileUtils.bench.ts` | Frontend perf benchmarks |
| `src/stores/compressionStore.bench.ts` | Store perf benchmarks |
| `src-tauri/benches/compression_bench.rs` | Rust Criterion benchmarks |
| `.github/workflows/test.yml` | CI/CD pipeline |

### Files to Modify

| File | Change |
|------|--------|
| `package.json` | Add vitest deps + test scripts |
| `vite.config.ts` | Add `test` config block |
| `src-tauri/Cargo.toml` | Add criterion, tempfile + bench target |
| `src-tauri/src/compression/progress.rs` | Add `#[cfg(test)]` module |
| `src-tauri/src/ffmpeg/args.rs` | Add `#[cfg(test)]` module |
| `src-tauri/src/commands/pdf.rs` | Add `#[cfg(test)]` module |
| `src-tauri/src/commands/probe.rs` | Add `#[cfg(test)]` module |
| `src-tauri/src/commands/clipboard.rs` | Add `#[cfg(test)]` module |
| `src-tauri/src/logging/setup.rs` | Add `#[cfg(test)]` module |
| `src-tauri/src/compression/image.rs` | Add `#[cfg(test)]` integration tests |

### Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib
npx vitest run
cargo bench --manifest-path src-tauri/Cargo.toml --bench compression_bench
npx vitest bench
npx tsc --noEmit
```

---

## Verification Plan (Manual)

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
