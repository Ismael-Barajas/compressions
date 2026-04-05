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
| **O — Optimization**       | 🔲 Planned  | Benchmarks, opt-level, FFmpeg presets, AVIF tuning, HW accel |
| 13 — Final Polish          | 🔲 Planned  | Cross-cutting integration, keyboard shortcuts               |

---

## Phase O — Performance Optimization

> Goal: Measurably improve compression speed across all platforms using real benchmark data, without overcomplicating the codebase.

### O-1: Expand Benchmarks (baseline first)

- [ ] Add AVIF 1080p benchmark to `src-tauri/benches/compression_bench.rs`
- [ ] Add AVIF speed comparison group (speed 6 vs 7 vs 8)
- [ ] Add GIF re-quantization benchmark
- [ ] Run full suite and save baseline: `cargo bench -- --save-baseline before-optimization`

### O-2: Release Profile — `opt-level "s"` → `3`

- [ ] Change `opt-level = "s"` to `opt-level = 3` in `src-tauri/Cargo.toml`
- [ ] Run benchmarks, compare against baseline (expected: 10-30% faster images)

**Why:** All native encoders (mozjpeg, oxipng, ravif, webp, imagequant) currently compile for binary size, not speed. `opt-level = 3` enables auto-vectorization and SIMD. Binary grows ~3MB vs 80MB+ sidecar — negligible.

### O-3: FFmpeg Preset Flags

- [ ] Add `-preset fast` for libx264 in `src-tauri/src/ffmpeg/args.rs`
- [ ] Add `-preset fast` for libx265
- [ ] Add `-preset 7` for libsvtav1 (scale 0-13, lower=slower; default 10 is very slow)
- [ ] Verify with test video (expected: 40-60% faster video encoding)

**Why:** Currently no preset flag is passed. libx264/x265 default to "medium", SVT-AV1 to preset 10. Quality impact at same CRF: <0.5 dB PSNR — visually imperceptible.

### O-4: AVIF Encoder Tuning

- [ ] Change ravif speed from 6 → 7 in `src-tauri/src/compression/image.rs`
- [ ] Add `.with_num_threads(Some(4))` to prevent rayon pool contention in batches
- [ ] Run AVIF benchmarks (expected: 40-60% faster per-image)

**Why:** Speed 7 is ~40-60% faster with minimal quality loss. Thread cap prevents multiple concurrent AVIF encodes from each claiming all cores.

### O-5: Image Batch Concurrency Limit

- [ ] Add `tokio::sync::Semaphore` in `compress_images_batch` (`src-tauri/src/commands/image.rs`)
- [ ] Cap at `std::thread::available_parallelism().min(8)` — no new crate needed
- [ ] Test with 50+ image batch (expected: 20-50% faster, prevents OOM)

**Why:** Currently 500 images spawn 500 simultaneous tasks → memory pressure + thread thrashing. Semaphore ensures CPU saturation without overload.

### O-6: Video Hardware Acceleration (VideoToolbox / NVENC)

- [ ] Add `detect_hw_encoders()` in `src-tauri/src/ffmpeg/probe.rs` — runs `ffmpeg -encoders` at startup
- [ ] Cache results in `AppState` (`src-tauri/src/state.rs`)
- [ ] In `build_video_args`, remap H264→`h264_videotoolbox` / H265→`hevc_videotoolbox` on macOS when available
- [ ] Map CRF to VideoToolbox `-q:v` quality parameter
- [ ] Add fallback: if HW encode fails, retry with software encoder
- [ ] No change for AV1 (no good HW encoder available)
- [ ] Test on Apple Silicon (expected: 3-10x faster H264/H265)

**Why:** Bundled FFmpeg already has `h264_videotoolbox` and `hevc_videotoolbox` compiled in but never used. Auto-detect with graceful fallback — no UI changes needed.

### Optimization Summary

| Sub-phase | Target | Expected Gain | Complexity |
|-----------|--------|---------------|------------|
| O-1 | Benchmarks | Baseline data | Low (~60 lines) |
| O-2 | opt-level | 10-30% images | Trivial (1 line) |
| O-3 | FFmpeg presets | 40-60% video | Low (~15 lines) |
| O-4 | AVIF tuning | 40-60% AVIF | Low (2 lines) |
| O-5 | Batch semaphore | 20-50% batches | Medium (~30 lines) |
| O-6 | HW acceleration | 3-10x video | High (~150 lines) |

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
