# Compressions App — Plan

> Stack and conventions live in [.claude/CLAUDE.md](.claude/CLAUDE.md). This file tracks the remaining work, the design decisions that aren't obvious from the code, and how to verify the app end-to-end.

## Status

All implementation phases complete. Deferred work:

- ~~**Phase 6** — Custom app icons~~ ✅ Complete
- ~~**Phase 7** — CI/CD release workflow & auto-updater~~ ✅ Complete (code signing deferred — no Apple/Windows certs yet)
- **Phase 14** — Document-to-Markdown via Pandoc sidecar (see [Deferred: Document-to-Markdown](#deferred-document-to-markdown-pandoc-sidecar) below)

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

## Deferred: Document-to-Markdown (Pandoc sidecar)

Optional feature to convert documents (DOCX, HTML, EPUB, RTF, ODT, LaTeX) to Markdown, using Pandoc as a native binary sidecar. Deferred because the core product is media compression; this is a scope expansion. Rejected Microsoft's `markitdown` (Python 3.10+, ~150–250 MB bundle growth, breaks the "no runtime deps" pattern) in favor of Pandoc (single static binary, ~35–50 MB).

**Not covered by Pandoc:** image OCR, audio transcription, PDF text extraction, XLSX/PPTX rich content. If any of these become required, reopen the decision.

### Files to add / modify

Follow the three-place command registration rule from [.claude/CLAUDE.md](.claude/CLAUDE.md).

1. **[src-tauri/tauri.conf.json](src-tauri/tauri.conf.json)** — add `"binaries/pandoc"` to `externalBin`.
2. **`scripts/download-pandoc.ps1`** + **`scripts/download-pandoc.sh`** — new files modeled on [scripts/download-gs.sh](scripts/download-gs.sh). Source: https://github.com/jgm/pandoc/releases (static binaries for Linux x86_64/arm64, macOS universal, Windows x86_64). Pin a version as a top-of-file constant. Output naming: `pandoc-<target-triple>[.exe]` in `src-tauri/binaries/`. No resource directory needed — Pandoc is self-contained.
3. **`src-tauri/src/commands/markdown.rs`** — new command module mirroring [src-tauri/src/commands/pdf.rs](src-tauri/src/commands/pdf.rs) (sequential, indeterminate progress).
   - `#[tauri::command] async fn convert_to_markdown(app, input: String, output: String, on_progress: Channel<ProgressEvent>) -> Result<ConversionResult, String>`
   - `create_dir_all(output.parent())` before spawning.
   - Spawn via `app.shell().sidecar("pandoc")` with args `["-t", "gfm", "-o", <output>, <input>]` — Pandoc auto-detects source format from extension.
   - Emit `ProgressEvent::Started` → wait for `CommandEvent::Terminated` → emit `Finished` with stats (no progress hooks; treat as indeterminate like PDF).
4. **[src-tauri/src/commands/mod.rs](src-tauri/src/commands/mod.rs)** — add `pub mod markdown;`.
5. **[src-tauri/src/lib.rs](src-tauri/src/lib.rs)** — register `commands::markdown::convert_to_markdown` in `invoke_handler`.
6. **[src/lib/commands.ts](src/lib/commands.ts)** — add `convertToMarkdown(input, output, onProgress)` wrapper.
7. **[src/types/compression.ts](src/types/compression.ts)** — extend `MediaType` union with `"document"`.
8. **[src/lib/fileUtils.ts](src/lib/fileUtils.ts)** — add `DOCUMENT_EXTENSIONS` set (`.docx`, `.doc`, `.html`, `.htm`, `.epub`, `.rtf`, `.odt`, `.tex`), extend `getMediaType()` to return `"document"`. `getOutputFileName()` already handles format→extension mapping; callers pass `"md"`.
9. **[src/hooks/useCompression.ts](src/hooks/useCompression.ts)** — add `convertAllToMarkdown()` that iterates queued document files. Reuse `getOutputDirForFile()` and `buildOutputPath()` with `format="md"`.
10. **[src/components/controls/ToolsTab.tsx](src/components/controls/ToolsTab.tsx)** — add a "Convert Documents to Markdown" button alongside "Extract Audio from All Videos". Gate on `files.some(f => f.mediaType === "document" && f.status === "queued")`.
11. **[src/components/file-list/FileItem.tsx](src/components/file-list/FileItem.tsx)** — add a right-click "Convert to Markdown" action for `mediaType === "document"` files.
12. **[src/stores/compressionStore.ts](src/stores/compressionStore.ts)** — no new options initially. Add a `markdownOptions` slice only if advanced options (output dialect, extract media) are requested later.

### Verification

Add to the manual verification table above:

| #  | Test               | Steps                                                                    |
| -- | ------------------ | ------------------------------------------------------------------------ |
| 13 | DOCX → MD          | Drop a `.docx` with headings, lists, tables → convert → verify structure |
| 14 | HTML → MD          | Drop an `.html` file → convert → verify links and code blocks intact     |
| 15 | EPUB → MD          | Drop an `.epub` → convert → verify chapter structure                     |
| 16 | Batch conversion   | Queue 3 mixed docs → batch convert → verify all produced                 |
| 17 | Right-click single | Right-click a docx → "Convert to Markdown" → verify single-file output   |

Build checks per [.claude/CLAUDE.md](.claude/CLAUDE.md) Phase Completion Checklist: `npx tsc --noEmit`, `cargo check`, `cargo clippy -- -D warnings`.

### Known gotchas

- **Pandoc license is GPLv2+** — compatible with the app's open-source status; worth adding to the Known Gotchas table alongside Ghostscript AGPLv3.
- **GFM vs CommonMark** — default to `-t gfm` (GitHub-Flavored Markdown) as the most widely consumed variant. Make configurable only if users ask.
- **Pandoc binary size** — ~35–50 MB per platform. Adds to existing FFmpeg (~100 MB) and Ghostscript (~20 MB + resources). Mention in release notes.
