# Compressions App — Plan

> Stack and conventions live in [.claude/CLAUDE.md](.claude/CLAUDE.md). This file tracks the remaining work, the design decisions that aren't obvious from the code, and how to verify the app end-to-end.

## Status

All implementation phases complete. Deferred work:

- **Phase 6** — Custom app icons (needs design)
- **Phase 7** — CI/CD release workflow & code signing

## Key Design Decisions

- **Image batch concurrency** capped via semaphore at `min(available_parallelism, 8)` in `src-tauri/src/commands/image.rs` — prevents OOM and thread thrashing on 500-file batches.
- **Ghostscript resource files** (`gs-res/`) bundled alongside the sidecar; resolved via `CARGO_MANIFEST_DIR` (dev) or `resource_dir()` (prod).
- **Hardware video acceleration** auto-detected at startup (VideoToolbox on macOS where available, NVENC on Windows). Cached in `AppState`. Falls back to software encode if HW encode fails. No UI changes — entirely transparent.
- **Release profile uses `opt-level = 3`** (not `"s"`). Image encoders gain 15–46% (PNG, AVIF, GIF, WebP); JPEG regresses ~7% (mozjpeg's hand-tuned SIMD prefers size-optimized codegen). Binary grows ~3 MB — negligible vs the 80+ MB FFmpeg sidecar.
- **FFmpeg presets** explicitly set: `-preset fast` for libx264/libx265, `-preset 7` for libsvtav1.
- **Image encoder benchmarks:** `cd src-tauri && cargo bench --bench compression_bench`.

## Manual Verification

| #  | Test                  | Steps                                                                       |
| -- | --------------------- | --------------------------------------------------------------------------- |
| 1  | AVIF input            | Drop AVIF (with transparency) → compress to PNG → verify alpha preserved    |
| 2  | Dimensions            | Add mixed files → verify WxH shown for each in file list                    |
| 3  | Subfolder             | Compress with "subfolder" mode → verify `compressed/` dir created           |
| 4  | Name template         | Set `{name}_{date}` → compress → verify output filename matches             |
| 5  | Audio extraction      | Right-click video → Extract Audio → verify MP3 output                       |
| 6  | Video-to-GIF          | Right-click video → Convert to GIF → verify animated GIF + progress bar     |
| 7  | GIF optimization      | Drop large GIF → compress → verify smaller output                           |
| 8  | PDF                   | Drop PDFs → compress with Ebook preset → verify smaller output              |
| 9  | Clipboard paste       | Copy file in Explorer → Ctrl+V → verify in queue. Copy screenshot → verify  |
| 10 | Queue during compress | Start 5 files → drag 3 more → verify they queue and process                 |
| 11 | History               | Compress files → open History → verify entries with stats                   |
| 12 | Logs                  | Run operations → open Log Viewer → verify structured entries                |

## Known Gotchas

| Issue                                          | Mitigation                                                  |
| ---------------------------------------------- | ----------------------------------------------------------- |
| `mozjpeg` requires NASM assembler              | Document in README; CI installs it                          |
| `ravif`/`rav1e` slow first compile (~5–10 min) | Accept; cache `target/` in CI                               |
| FFmpeg binaries ~100 MB, not in git            | `scripts/download-ffmpeg.*` — auto-downloaded in CI         |
| macOS FFmpeg dylib resolution                  | Use statically-linked builds (BtbN GPL static)              |
| Tauri `DragDropEvent` fires twice (#14134)     | Deduplicate by tracking last drop timestamp (100 ms window) |
| No Rust cross-compilation for Tauri            | Platform-specific CI runners                                |
| WebView2 missing on older Windows              | `downloadBootstrapper` mode in `tauri.conf.json`            |
| Ghostscript licensed AGPL v3                   | App must remain open source or obtain a commercial license  |
