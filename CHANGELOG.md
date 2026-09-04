# Changelog

All notable changes to Compressions are documented here.

## [Unreleased]

### Performance

- **Parallel audio and PDF batches**: audio encoders and Ghostscript are single-threaded, so batches now run several files at once instead of one at a time (video and GIF stay sequential; those encoders already use every core)
- **No redundant probing**: the duration probed when a file is added is passed to the encoder, so ffprobe is no longer spawned again before every video, audio, and GIF job
- **Progress without the flood**: FFmpeg progress is parsed as proper `-progress` blocks (speed and ETA are now populated) and only forwarded to the UI when the bar moves visibly; the store updates a single file per event and components subscribe to summary counters instead of rescanning the whole queue
- **History writes coalesced**: history lives in memory and is flushed in the background (temp file + rename) instead of re-reading, re-parsing, and rewriting the whole file after every completed job
- **Keep-original is instant** on filesystems that support reflinks (APFS, Btrfs, XFS, ReFS); large copies no longer block the async runtime
- **UI never freezes on folder scans** or log reads: those commands run on blocking threads; scanning uses `walkdir`, does not follow symlinks, and skips hidden directories
- **HW encoders are verified** with a one-frame encode at startup, and an encoder that fails at runtime is disabled for the session, so machines without a usable GPU no longer pay for a failed NVENC spawn per video
- **GIF conversion in one pass**: `split` + `palettegen`/`paletteuse` reads the source once and needs no temporary palette file
- **Faster native image pipeline**: PNG pixels go straight to oxipng (no intermediate encode/decode) in their native color type; AVIF encodes from zero-copy buffers and uses the RGB path for opaque images; JPEG avoids an extra pixel copy; resizing uses SIMD convolution (`fast_image_resize`); encoder threads are budgeted across concurrent image jobs; animated GIFs are re-quantized frame by frame with 0.5 dithering
- **Thumbnails** regenerate as you scroll (visible range, not just item count) and are keyed by file size/mtime so edited files refresh
- **Log viewer and history** render only the visible rows; logs are read newest-first and retained for 7 days
- **Fonts are bundled** instead of fetched from Google Fonts at launch
- Dev builds compile dependencies optimized so `tauri dev` image encoding is usable

### Bug Fixes

- The file list's **Add** dialog now accepts audio files (extension lists come from one backend source)
- Grayscale images can be encoded to WebP and AVIF
- Files in one batch share the same `{date}`/`{time}` template stamp
- The update indicator in the header reflects the automatic startup check
- Batch-level failures that happen before a job starts are surfaced on the affected files instead of leaving them queued

### Removed

- Unused custom-preset backend (never reachable from the UI) and the unused single-file IPC commands

## [1.1.2] — 2026-07-02

### Performance

- **Smarter batch ordering**: video, PDF, and audio queues now process smaller files first; image batches start with larger files to keep parallel workers busy
- **Faster PNG compression**: quick RGBA pre-encode plus lighter oxipng preset reduces PNG encode time
- **Faster AVIF/HEIC decode**: FFmpeg intermediate files use QOI instead of PNG — less disk I/O and full alpha preservation

### Bug Fixes

- **History corruption under parallel compression**: concurrent image job completions no longer race on the history file
- **History file size**: history is written as compact JSON instead of pretty-printed

## [1.1.1] — 2026-04-27

### New Features

#### Queue Controls: Pause / Resume / Cancel All
- New buttons appear next to the batch progress bar while compression is running
- **Pause**: stops dequeuing new files; in-flight items finish naturally so no work is lost
- **Resume**: continues draining the queue from where Pause stopped
- **Cancel All**: kills every in-flight FFmpeg/Ghostscript child, stops the image batch from spawning more tasks, and reverts all in-progress and queued files back to a clean queued state — files stay in the list, ready to retry
- Clear button is disabled during compression (previously, clicking Clear emptied the list while the backend kept compressing in the background)

### Bug Fixes

- **Zero-byte output marker leak**: orphaned 0-byte placeholder files left behind when an early error occurred between path-claim and encoder-spawn (e.g. AVIF decode failure, sidecar spawn error). Replaced with a self-cleaning RAII guard that removes only 0-byte markers — real output bytes are never destroyed.
- **History pollution from Cancel All**: each killed compression no longer adds a `success: false` row to history. Cancelling a 100-file batch used to leave 100 cancellation entries; now cancelled jobs are silent.
- **Misleading errors during cancel**: the per-file Error event is suppressed when the kill came from Cancel All, so the UI doesn't briefly flash "FFmpeg exited with code Some(-1)" while the file is being reset to queued.
- **Started-event index drift**: video/PDF/audio batch handlers in the frontend used insertion-order indices to map `Started` events to files, which would silently corrupt UI state if the backend ever skipped a file. Now matched by `inputPath`, consistent with the image batch handler.
- Patched [`package.json`](package.json) version (was stale at `1.0.0`).

## [1.1.0] — 2026-04-14

### New Features

#### Audio Compression
- First-class audio compression support: MP3, AAC, Opus, FLAC, WAV, and 10 additional input formats (WMA, AIFF, APE, ALAC, AC3, DTS, PCM, AMR, OGG, M4A)
- Output format selector: MP3, AAC, Opus, FLAC, WAV, or Original (preserves source format; niche formats fall back to MP3)
- Bitrate presets (64k – 320k) with custom input for lossy formats
- Sample rate control: Original, 48000, 44100, or 22050 Hz
- Animated waveform equalizer progress indicator during compression

#### HEIC / HEIF Image Support
- HEIC and HEIF files accepted as image inputs
- Thumbnails generated via FFmpeg decode → image-crate pipeline (avoids filtergraph conflicts)
- HEIC inputs compressed using image-crate pipeline (output format falls back to JPEG, as no HEIF muxer is available)

#### Performance: Batch File Probing
- Replaced unthrottled sequential probe loop with `probe_files_batch` — a semaphore-limited Tauri command that streams results via `Channel<ProbeEvent>`
- New `updateFileProbes` store action performs a single O(n) bulk state update instead of one `setState` call per file
- Eliminates UI jank when adding large folders

#### Batch Progress Bar
- Persistent progress bar visible across the full compression batch, not just per-file

### Bug Fixes

- **Virtualized list row clipping** — `virtualizer.measure()` is now called when toggling thumbnail view, invalidating cached row heights so the bottom row is no longer clipped
- **Probe tracking memory leak** — Probe state and flush timer are now cleared when the file list is cleared, preventing stale intervals from running after a reset
- **FileItem re-renders** — `FileItem` memoized with `React.memo`; unnecessary re-renders eliminated during probe streaming and store updates

## [1.0.0] — Initial Release

- Video compression (H.264, H.265, AV1) with hardware acceleration (NVENC, VideoToolbox)
- Image compression (JPEG/MozJPEG, PNG/oxipng, WebP, AVIF/ravif, GIF) with parallel processing
- PDF compression via Ghostscript
- Audio extraction from video files
- Video-to-GIF conversion
- Preset system (built-in + custom)
- Compression history and application log viewer
- Auto-updater
