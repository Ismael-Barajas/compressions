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
