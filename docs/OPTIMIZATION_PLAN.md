# Compressions — Performance & Efficiency Optimization Plan

Status: **implemented on `claude/app-optimization-plan-xhv7ln`** (all four phases; see CHANGELOG "Unreleased"). Kept as the rationale record for the changes.
Scope: full pass over the frontend (`src/`), the Rust backend (`src-tauri/`), build config, and CI as of v1.1.2.

Each item lists what happens today, why it costs something, the proposed change, expected payoff,
effort (S / M / L), and risk. Items are grouped into three tiers and then sequenced into phases at the end.

---

## Executive summary

The app is already in reasonable shape (batch probing, virtualized list, memoized rows, RAII output
claims, cancel flag). The remaining waste clusters into six themes:

| # | Theme | Where | Payoff |
|---|-------|-------|--------|
| 1 | Single-threaded sidecar batches run strictly sequentially (audio, PDF) | `commands/audio.rs`, `commands/pdf.rs` | 2–4× faster audio/PDF batches on multi-core machines |
| 2 | History file is re-read, re-parsed, re-serialized and re-written on **every** completed file, under a blocking mutex, on the async runtime | `history/storage.rs` | Removes O(n²) disk churn on large image batches; unblocks tokio workers |
| 3 | Progress events fire at 10 Hz and every one rebuilds the whole `files` array and re-renders every `files` subscriber | `ffmpeg/args.rs`, `stores/compressionStore.ts`, ~8 components | Big drop in main-thread work during long batches, especially with 500+ files queued |
| 4 | Redundant process spawns: ffprobe re-run before every video/audio/GIF job even though the file was already probed; two full video decodes for GIF; HW encoder attempted and failed on every file when the GPU is absent | `commands/video.rs`, `audio.rs`, `gif.rs`, `ffmpeg/probe.rs` | One fewer process per job; GIF conversion ~2× faster; no wasted HW attempt per file |
| 5 | Blocking `std::fs` work inside async Tauri commands (incl. copying a multi-GB video on a tokio worker) and CPU-heavy sync commands on the main thread (`scan_paths`, `read_logs`) | several `commands/*.rs` | Prevents UI stalls and runtime starvation |
| 6 | Encoder-level inefficiencies in the native image pipeline (PNG encode→decode→re-encode round trip, extra pixel copies for AVIF, thread oversubscription, slow Lanczos resize) | `compression/image.rs`, `commands/image.rs` | 10–40% faster per-image, less memory |

Plus a hygiene tier: dead code (whole Rust preset subsystem is unused by the UI), five copies of the
extension lists (with a real bug: the file-list "Add" dialog can't pick audio files), unbounded log
directory growth, CI disabled on PRs, fonts fetched from Google at runtime (blocked by CSP anyway).

---

## Tier 1 — high impact

### 1. Parallelize audio and PDF batches
**Today.** `compress_audio_batch`, `extract_audio_batch`, and `compress_pdfs_batch` loop `for entry in files { ... .await }` — one sidecar at a time. The encoders involved (libmp3lame, aac, libopus, flac, pcm, Ghostscript `pdfwrite`) are all single-threaded, so on an 8-core machine 7 cores idle during a 200-file audio batch.
**Video stays sequential** — libx264/x265/SVT-AV1 already saturate all cores (HW encoders are a possible exception, see item 7).

**Change.**
- Reuse the pattern from `compress_images_batch`: `Arc<Semaphore>` + `tokio::spawn` per entry, re-check `CancelFlag` after acquiring a permit, collect results in input order.
- Concurrency: audio `min(available_parallelism, 4)`; PDF `min(available_parallelism / 2, 3)` (Ghostscript is memory-hungry).
- Frontend needs no change: `useCompression` already matches `Started` by `inputPath` and `Progress/Completed/Error` by `jobId`; `AppState.active_jobs` is keyed by job id so per-file cancel and Cancel All keep working.
- Keep the "smaller first" ordering from `scheduling.ts`; with parallelism it still helps by letting short jobs finish and free the UI early.

**Effort** S–M. **Risk** low; behavior is already proven for images. Watch for disk contention on HDDs (cap at 4).

### 2. Stop rewriting `history.json` on every completed file
**Today.** `history::append_entry` takes a global `std::sync::Mutex`, reads the whole file, `serde_json` parses up to 1000 entries, pushes, serializes, writes — once per finished file, called from inside async commands. For a 500-image batch with 8 workers: 500 full read/parse/write cycles, serialized on a blocking lock held on tokio worker threads.

**Change.**
- Load history once at startup into managed state: `HistoryStore(Mutex<Vec<HistoryEntry>>, dirty: AtomicBool)`.
- `append_entry` becomes an in-memory push + cap + mark dirty. `get_history` reads from memory.
- Flush to disk from a single background task with coalescing (e.g. flush ≤1×/second while dirty, plus on `RunEvent::Exit`) using `tokio::fs` or `spawn_blocking`. Write to a temp file and rename for crash safety.
- Optional: switch the on-disk format to JSON Lines so a flush is an append instead of a rewrite; keep the 1000-entry cap by compacting when the file exceeds ~2× cap.

**Effort** M. **Risk** low; add a unit test for cap + flush ordering. Existing `history.json` must still load (same entry shape).

### 3. Throttle progress and make per-file updates O(1)
**Today.**
- FFmpeg is launched with `-progress pipe:2 -stats_period 0.1` → ~10 progress blocks/sec, each block is ~10 stderr lines, each line goes through the tokio channel and three regexes. Every `out_time=` line produces an IPC `Progress` event.
- Note: with `-progress` the `speed=` value is on a separate line from `out_time=`, so `speed` and `eta_seconds` in `ProgressPayload` are always `None` — they are computed but never populated, and the frontend only reads `percent` anyway.
- In the store, `updateProgress` does `files.map(...)` over the whole array, producing a new `files` reference. Every component that selects `s.files` (`AppShell`, `FileList`, `Sidebar`, `CompressTab`, `ToolsTab`, `BatchProgressBar`, `ResultsSummary`) re-renders and re-derives (`filter`/`some`/`reduce`) on each tick. `AppShell`'s probe effect also re-runs its `files.filter` on every tick.
- With 4 media batches running concurrently that is up to ~40 store writes/sec × O(n) work each.

**Change (backend).**
- Coalesce in the Rust loop: only send `Progress` when `percent` moved ≥ 0.5 or ≥ 250 ms elapsed since the last send. Raise `-stats_period` to `0.25`.
- Trim `ProgressPayload` to what is used (`job_id`, `percent`; keep `speed`/`eta` only if the UI will show them — if so, parse the multi-line `-progress` block properly instead of per-line regex).

**Change (frontend).**
- Keep `jobId → fileId` and `fileId → index` maps in the store (maintained in `addFiles`/`removeFile`/`setFileStatus`/`clearFiles`) so `updateProgress`, `markComplete`, `markError` touch one element instead of mapping the array.
- Store summary counters updated incrementally (`counts: { queued, processing, complete, error }`, `mediaCounts`). Components then select primitives: `hasVideos`, `hasQueued`, `allComplete`, `availableTabs`. Zustand skips re-render when the selected primitive is unchanged.
- `BatchProgressBar` can select `files` but derive with a cheap running sum, or subscribe with `useShallow` to `[completedCount, processingProgressSum]` maintained in the store.
- Make the `AppShell` probe effect key off a `unprobedCount`/`files.length` change rather than `files` identity, or trigger probing directly from `addFiles`.

**Effort** M (backend S, frontend M). **Risk** medium — the store shape change touches many actions; existing store tests (`tests/stores/compressionStore.test.ts`) cover `updateProgress`/transitions and should be extended for the maps and counters before refactoring.

### 4. Reuse probe results instead of spawning ffprobe again per job
**Today.** Every video/audio/GIF job calls `probe_video_duration` (a fresh `ffprobe` spawn with a 30 s timeout) before spawning FFmpeg, even though the file was already probed when it was added (`probe_files_batch`) and the frontend holds `duration` in `QueuedFile`.

**Change.**
- Add `duration: Option<f64>` to `BatchEntry` (and to the single-file commands); frontend passes `file.duration`. Backend only probes when it is `None`.
- Alternatively/additionally: a small in-process probe cache keyed by `(path, mtime, len)` in managed state, populated by `probe_files_batch`, so retries and the tools tab hit the cache.

**Effort** S. **Risk** low. Saves one process spawn (typically 50–300 ms, more on Windows) per job — noticeable on batches of many short audio files.

### 5. Move blocking work off the async runtime and off the main thread
**Today.**
- Async commands call `std::fs::copy(&input, &output)` when the compressed output is not smaller. For video this copies gigabytes synchronously on a tokio worker thread, starving progress delivery and other jobs.
- `std::fs::metadata`, `create_dir_all`, `remove_file`, `read_to_string`, `write` are used throughout async command bodies (small, but they add up under an 8-way image batch).
- Sync (non-`async`) commands run on the webview main thread in Tauri v2. `scan_paths` recursively walks whole directory trees (no symlink-loop guard, no skipping of `.git`/`node_modules`), `read_logs` reads and parses every log file, `save_clipboard_image` PNG-encodes — all freezing the UI while they run.

**Change.**
- Replace large copies with `tokio::fs::copy` (or `spawn_blocking`). Prefer a reflink/clone when the filesystem supports it (`reflink-copy` crate: APFS, Btrfs, ReFS, XFS) with copy as fallback; this makes "keep original" near-instant.
- Wrap remaining `std::fs` calls in hot paths with `spawn_blocking` or switch to `tokio::fs`.
- Make `scan_paths`, `read_logs`, `read_clipboard_files`, `save_clipboard_image` `async` and do the work in `spawn_blocking`. Use `walkdir` (or `ignore` for a parallel walker) with `follow_links(false)` and skip hidden/vendor directories.

**Effort** S–M. **Risk** low.

### 6. Hardware-encoder detection and fallback
**Today.** Startup runs `ffmpeg -encoders` and treats any listed `*_nvenc`/`*_videotoolbox` as usable. On a Windows/Linux box without an NVIDIA GPU, `h264_nvenc` is still listed, so **every** video first spawns FFmpeg with NVENC, fails, deletes the partial output, and re-runs with software. That is a wasted spawn plus a second `Started`-less run per file, and the log shows "Trying HW encoder" for each.

**Change.**
- Verify capability at startup with a real 1-frame encode: `ffmpeg -f lavfi -i nullsrc=s=256x256:d=0.1 -frames:v 1 -c:v h264_nvenc -f null -` (same for hevc/videotoolbox). Only encoders that succeed go into `HwEncoders`.
- On first runtime failure of an encoder, remove it from `HwEncoders` (write lock) so later files skip straight to software.
- Optional: allow concurrency 2 for video batches when a HW encoder is in use (HW encoders do not saturate the CPU). Gate behind a setting; measure first.

**Effort** S. **Risk** low.

---

## Tier 2 — medium impact

### 7. GIF conversion in a single FFmpeg pass
**Today.** Two full decodes of the source video (palettegen pass, then paletteuse pass) plus a temp palette PNG and two process spawns.
**Change.** One invocation with `-filter_complex "[0:v]fps=..,scale=..,split[a][b];[a]palettegen=..[p];[b][p]paletteuse=.."`. Progress parsing works unchanged. Roughly halves decode time and removes the temp file.
**Effort** S. **Risk** low; `stats_mode=diff` and dither options carry over verbatim. Keep the two-pass path behind a flag only if a quality regression is observed in testing.

### 8. Native image pipeline (`compression/image.rs`)
| Sub-item | Today | Change | Effort |
|---|---|---|---|
| PNG round trip | Encode with `image` (Fast) → `oxipng::optimize_from_memory` decodes it again and re-encodes with preset 2 | Feed raw pixels via `oxipng::RawImage::new(...)` and `.create_optimized_png(&opts)`, skipping the intermediate encode/decode. Pass the image's native color type instead of forcing `to_rgba8()` (grayscale/RGB inputs currently get inflated to RGBA and rely on oxipng reductions to shrink back) | S |
| AVIF pixel copy | Builds a fresh `Vec<rgb::RGBA8>` from `rgba.pixels()` | Use `rgb::FromSlice::as_rgba()` on `rgba.as_raw()` (zero-copy cast). For images with no alpha, use `encode_rgb` (cheaper alpha plane handling) | S |
| AVIF threads | `with_num_threads(Some(4))` per job × up to 8 concurrent jobs = 32 encoder threads + oxipng's rayon pool + tokio workers | Budget threads: `per_job = max(1, cores / concurrent_jobs)`; or run the batch semaphore at `cores` and set encoder threads to 1 when the batch has more files than cores | S |
| AVIF speed | Speed 6 (the bench in `benches/compression_bench.rs` compares 6/7/8) | Expose speed as an advanced option or default to 7 for batches > N files; measure size/time from the existing bench | S |
| JPEG | `img.to_rgb8()` always allocates a copy even when the decoded image is already RGB8 | Borrow when `DynamicImage::ImageRgb8`, convert otherwise (`Cow`) | S |
| Resize | `imageops::resize(Lanczos3)` at full precision for every downscale | Use `fast_image_resize` (SIMD, 5–10× faster) or two-step: box/triangle down to ~2× target then Lanczos3 | S–M |
| Metadata preservation | Re-reads the entire input file and clones the encoded buffer | Read only the leading segments needed for EXIF; avoid `data.clone()` by building the output from parts | S |
| Animated GIF | All frames decoded to RGBA and kept in memory; per-frame local palette; dithering level 1.0 (max, slowest); no frame-diff optimization | Stream frame-by-frame (decode → quantize → write); default dither 0.5; optional shared global palette from a histogram of all frames (better size, one quantization) | M |
| GIF `Original` handling | Frontend `keepExt` treats `.gif` as "keep" while `getOutputFileName` also forces `.gif` — two places encode the same rule | Fold into one helper | S |

### 9. Frontend structure: de-duplicate and de-hook the compression driver
**Today.**
- `useCompression()` is instantiated in every `FileItem` (N rows), plus `FileList`, `ToolsTab`, and `useKeyboardShortcuts`. Each instance creates 6 store subscriptions and 9 `useCallback`s; with 1000 files that is ~9000 selector evaluations per store update.
- `startCompression` contains four ~80-line near-identical blocks (video/image/pdf/audio) plus two batch tool functions and two single-file functions, all repeating channel wiring and error mapping.
- Error paths do `for f of batch: getState().files.find(...)` then a separate `setState` per file → O(n²) and N renders on a batch-level failure.
- `Started` handlers do `batch.find(f => f.path === ...)` per event.

**Change.**
- Move all driver logic into a plain module (`src/lib/compressionController.ts`) that reads `useCompressionStore.getState()` and exposes functions; keep a thin `useCompression()` that just returns those stable functions (no subscriptions). `FileItem` imports the functions directly.
- Extract `runBatch({ files, invoke, buildOutput, onStarted })` and `createProgressChannel(pathToId: Map)` so each media type is ~10 lines.
- Batch-failure marking becomes one `setState` with a `Set<string>` of ids.
- `Escape` shortcut should call `cancelAllCompression()` once instead of N sequential `cancelFile` IPC calls.

**Effort** M. **Risk** low–medium; add tests for the controller (mock `invoke`/`Channel`) before refactoring — none exist today.

### 10. Probe result flushing is a debounce, should be a throttle
**Today.** `AppShell` pushes probe events into a buffer and resets a 150 ms timer on every event. While events keep arriving (a folder of thousands of images probes very quickly) the timer never fires, so no sizes appear until the whole batch finishes.
**Change.** Flush at most every 150 ms with leading + trailing behavior (or flush when buffer ≥ 50 items). **Effort** S.

### 11. Thumbnail generation trigger misses scroll
**Today.** The effect re-runs on `virtualizer.getVirtualItems().length`, which does not change when scrolling by whole rows, so rows scrolled into view may never request thumbnails until the count changes. The results loop is also O(n²) (`needThumbnails.find` per result).
**Change.** Key the effect off `virtualizer.range?.startIndex/endIndex` (or use the virtualizer `onChange` callback) and use a `Map<path, id>`. **Effort** S.

### 12. Log retention and log viewer
**Today.** `tracing_appender::rolling::daily` with no `max_log_files` → the log directory grows forever. `read_log_entries` reads and parses **every** file, then keeps the last 2000 lines. `LogViewer` renders up to 2000 rows unvirtualized with `key={i}`; `HistoryPanel` renders up to 1000 rows unvirtualized.
**Change.** Use the rolling `Builder` with `max_log_files(7)`; read files newest-first and stop once 2000 parsed lines are collected (tail read). Virtualize both panels with the already-bundled `@tanstack/react-virtual`. **Effort** S–M.

### 13. Self-host fonts
**Today.** `index.html` loads Bricolage Grotesque and IBM Plex Mono from Google Fonts on every launch. The production CSP (`style-src 'self' 'unsafe-inline'`, no `font-src`) blocks both the stylesheet and the font files, so the app pays for a network request and preconnects, then falls back to system fonts anyway (and flashes if the request is slow).
**Change.** Bundle the fonts (`@fontsource-variable/bricolage-grotesque`, `@fontsource/ibm-plex-mono`) and remove the external links. Deterministic rendering, works offline, no startup network. **Effort** S. Verify by checking the webview console for CSP violations in a release build first.

### 14. Sidecar job runner de-duplication (Rust)
**Today.** `video.rs`, `audio.rs` (×2), `pdf.rs`, `gif.rs` each contain the same ~100-line sequence: claim output → spawn → register in `active_jobs` → send `Started` → loop `CommandEvent`s → on `Terminated` compute size/success → keep-original copy → `Completed`/`Error` (cancel-aware) → history → return.
**Change.** One `run_sidecar_job(app, state, JobSpec) -> CompressionResult` with a pluggable stderr handler (progress vs. capture). Not a runtime win by itself, but it is the precondition for items 1, 3, 4, 5 to be applied once rather than five times. **Effort** M.

---

## Tier 3 — hygiene, correctness, DX

### 15. Single source of truth for supported extensions
Five copies today: `src/lib/fileUtils.ts`, `DropZone.tsx`, `FileList.tsx`, `commands/probe.rs`, `commands/scan.rs`, `commands/thumbnail.rs`. They have already drifted: the `FileList` "Add" dialog filter omits every audio extension and `.ts`, so audio can only be added via drag-drop or the empty-state drop zone.
**Change.** Define once in Rust (`media/extensions.rs`), expose via a `get_supported_extensions` command or generate the TS constant at build time; derive dialog filters from it. **Effort** S.

### 16. Remove dead code and unused dependencies
- The entire Rust preset subsystem (`presets/definitions.rs`, `presets/storage.rs`, `commands/presets.rs` except `get_default_output_dir`, the `Preset` type, and the `getPresets/savePreset/deletePreset` wrappers in `commands.ts`) is never called: `PresetSelector.tsx` hardcodes `BUILTIN_PRESETS`. The README's "save and delete custom presets" is not wired up. Decide: delete, or wire the UI to it (a separate feature ticket).
- Single-file commands (`compress_video`, `compress_image`, `compress_audio`, `compress_pdf`, `probe_file`, `detect_media_type`, `generate_thumbnail`) are exposed in `generate_handler!` but never invoked from the frontend — keep them as internal `fn`s, drop them from the IPC surface (smaller attack surface, less generated glue).
- npm: `@tauri-apps/plugin-fs` and `@tauri-apps/plugin-shell` are not imported anywhere in `src/` (the Rust `tauri-plugin-shell` crate is still required).
- Cargo: `tokio = { features = ["full"] }` — trim to `rt-multi-thread, macros, sync, time, fs, process`. `chrono` is only used for one RFC3339 timestamp (could use `time` or `std::time` + manual format) — optional.
**Effort** S.

### 17. Developer-loop speed
- Add `[profile.dev.package."*"] opt-level = 3` (and `[profile.dev] opt-level = 1`) so mozjpeg/ravif/oxipng/imagequant are not 20–50× slower in `tauri dev`.
- `vite.config.ts`: set `build.target` to the actual webview floor (`safari13` / `chrome105`) to avoid unnecessary down-leveling.
**Effort** S.

### 18. CI
`.github/workflows/test.yml` is `workflow_dispatch` only, so nothing runs on pushes/PRs. Re-enable `pull_request` + `push: main` for `frontend-tests` and `rust-lint` (fast, cached), keep `rust-tests` on the matrix for PRs, and leave `rust-benchmarks` manual/nightly. Add `vitest` coverage upload so the refactors in items 3 and 9 are measurable. **Effort** S.

### 19. Small frontend fixes noticed along the way
- `useUpdateCheck` is instantiated twice (`App` with auto-check, `Header` without) with independent state, so the header dot does not reflect the automatic check. Lift into a small store. (S)
- `getOutputFileName` evaluates `{date}`/`{time}` per file, so files in one batch can get different timestamps; compute once per batch. (S)
- `encode_webp` uses `webp::Encoder::from_image`, which errors for grayscale (`Luma`) inputs — convert to RGB(A) first. Correctness, not perf. (S)
- Thumbnail cache key is a hash of the path only; a modified file keeps the stale thumbnail until the cache is cleared. Include `mtime`+`len` in the hash. (S)

---

## Explicitly not recommended (for now)
- Replacing the FFmpeg-decode path for AVIF/HEIC with native decoders (`dav1d`/`libheif` bindings): large build complexity for a small win; the QOI temp-file path is already cheap.
- Rewriting the `files` array as a normalized map exposed to components: item 3's index maps + counters get most of the benefit without changing every component's data shape.
- Web Workers / off-main-thread rendering: after items 3 and 9 the main thread is no longer the bottleneck.

---

## Phased implementation plan

Each phase is independently shippable and ends with the existing test suites green (`npm run test:run`, `cargo test --lib`, `cargo clippy -D warnings`, `cargo fmt --check`) plus the validation listed.

### Phase 0 — Safety net (½ day)
1. Re-enable CI on PRs (item 18).
2. Add tests that pin current behavior before refactoring: store transitions with several concurrent jobs, `cancelAllCompression` resets, `updateFileProbes`; a controller test harness that mocks `invoke`/`Channel` and asserts the drain loop's event handling (item 9 prerequisite).
3. Capture baselines: `npm run test:bench`, `cargo bench` (JPEG/PNG/WebP/AVIF/GIF), and two manual timings on a dev machine — a 200-file MP3 batch and a 500-image JPEG→WebP batch (wall clock + CPU %).

### Phase 1 — Backend throughput (2–3 days)
1. Extract `run_sidecar_job` (item 14).
2. Parallel audio/PDF batches (item 1).
3. Pass `duration` from the frontend; probe only when missing (item 4).
4. Progress coalescing + `-stats_period 0.25`; drop unused payload fields (item 3, backend half).
5. Async/blocking fixes: `tokio::fs`/reflink copy, `spawn_blocking` for `scan_paths`/`read_logs`/clipboard, `walkdir` with no symlink following (item 5).
6. History in memory with coalesced flush (item 2).
7. HW encoder capability probe + disable-on-failure (item 6).
**Validate:** audio batch timing vs. baseline (expect ≥2× on 4+ cores); no `Started` index drift with 4 concurrent audio jobs; Cancel All mid-batch leaves no partial files; history contents identical after a 500-file batch; UI stays responsive while scanning a 50k-file folder.

### Phase 2 — Frontend hot path (2 days)
1. Store: `jobIdToFileId` / `fileIdToIndex` maps, incremental counters, O(1) `updateProgress`/`markComplete`/`markError` (item 3, frontend half).
2. Narrow selectors in `AppShell`, `Sidebar`, `CompressTab`, `ToolsTab`, `FileList`, `BatchProgressBar`, `ResultsSummary`.
3. Controller extraction + `runBatch` helper; `FileItem` stops calling `useCompression` (item 9).
4. Probe flush throttle (item 10); thumbnail range trigger (item 11).
**Validate:** React Profiler on a 1000-file queue during a video encode — `FileItem` commits per second should drop to ~the number of processing rows; `npm run test:bench` `updateProgress` should be O(1); manual scroll test shows thumbnails filling in.

### Phase 3 — Native image encoders (1–2 days)
1. PNG via `oxipng::RawImage`, native color type (item 8).
2. AVIF zero-copy + `encode_rgb` for opaque + thread budgeting (item 8).
3. JPEG borrow, fast resize (item 8).
4. GIF single-pass filter_complex (item 7).
**Validate:** `cargo bench` before/after for each encoder; byte-for-byte or size-within-1% outputs on a fixed corpus; a GIF conversion on a 30 s clip timed before/after.

### Phase 4 — Hygiene (1 day)
1. Extension single source of truth + fix the Add dialog filter (item 15).
2. Dead code and dependency removal (item 16); decide the preset system's fate.
3. Log retention + tail read + virtualized Log/History panels (item 12).
4. Self-hosted fonts (item 13), dev profile opt-levels and Vite target (item 17), small fixes (item 19).
**Validate:** release build size and startup time before/after; no CSP violations in the console; log dir stays ≤ 7 files.

---

## Files touched per phase (for scoping PRs)

| Phase | Rust | TypeScript / config |
|---|---|---|
| 0 | — | `.github/workflows/test.yml`, `tests/**` |
| 1 | `commands/{video,audio,pdf,gif,scan,logs,clipboard,queue}.rs`, `ffmpeg/{args,probe}.rs`, `compression/progress.rs`, `history/storage.rs`, `state.rs`, `lib.rs`, `types.rs`, `Cargo.toml` | `lib/commands.ts`, `hooks/useCompression.ts` (pass `duration`) |
| 2 | — | `stores/compressionStore.ts`, `hooks/useCompression.ts` → `lib/compressionController.ts`, `components/layout/AppShell.tsx`, `components/file-list/{FileList,FileItem}.tsx`, `components/output/*.tsx`, `components/controls/{CompressTab,ToolsTab}.tsx`, `hooks/useKeyboardShortcuts.ts` |
| 3 | `compression/image.rs`, `commands/image.rs`, `ffmpeg/args.rs`, `commands/gif.rs`, `Cargo.toml` | — |
| 4 | `presets/*`, `commands/presets.rs`, `logging/setup.rs`, `commands/{probe,scan,thumbnail}.rs`, `Cargo.toml` | `lib/fileUtils.ts`, `components/dropzone/DropZone.tsx`, `components/file-list/FileList.tsx`, `components/logs/LogViewer.tsx`, `components/history/HistoryPanel.tsx`, `index.html`, `vite.config.ts`, `package.json` |
