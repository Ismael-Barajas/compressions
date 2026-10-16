# Changelog

All notable changes to Compressions are documented here.

## [1.2.2] — 2026-10-15

### New

- **Pipeline view**: a List | Pipeline toggle in the file list toolbar shows the batch as a live diagram: the queue, the video, image, PDF and audio lanes running side by side, the size check, and how much was saved
- **Pause actually pauses**: the file that is running finishes, and the next one waits until you press Resume (before, the whole batch kept going)

### Bug Fixes

- **Converted files are kept, even when larger**: converting PNG to JPEG or MP3 to WAV no longer silently gives you back the original file in the old format, and those files are no longer labelled "Already optimized". Only same-format outputs fall back to the original when compression doesn't help
- **WebM, MOV, AVI, WMV and M4V sources**: when the source container can't hold the chosen codecs (for example a WebM with H.264), the output is written as MP4 instead of failing or producing a broken file
- **Hardware encoding stays on after a bad file**: a corrupt or empty video no longer turns off NVENC (or other hardware encoders) for the rest of the session; it is only turned off when the software retry succeeds
- **Corrupt PDFs are reported as failed** instead of "complete" with an empty output
- **Readable errors**: FFmpeg failures say what went wrong (for example "Could not read the file: it is corrupt or not a supported format") instead of a raw exit code
- **Queue order is kept**: files start in the order the scheduler picks (videos smallest first, images largest first) instead of whichever task woke up first
- **Adding the same folder again** no longer queues the files the app already wrote there (`photo_compressed.jpg` → `photo_compressed_compressed.jpg`)
- A file removed and added again gets its details (size, resolution, duration) read again
- Extract audio and Convert to GIF outputs no longer count towards "saved" totals
- Output names: characters Windows doesn't allow (`: * ? " < > |`) are replaced, `..` and path separators can't move output outside the chosen folder, an empty subfolder name falls back to `compressed`, and `{date}` uses your local time
- Stopping one file no longer counts as an encoder failure: it doesn't retry in software, turn off hardware encoding, or add a failed entry to history
- Stop and Cancel All can no longer miss a job that was just starting, and Extract audio and Convert to GIF work again after a Cancel All
- When the original is kept because the output was larger, it is copied safely (a failed copy keeps the compressed file), and an output name is never reused after `_999`
- Image jobs that fail clean up their partial output and show the error instead of leaving the row stuck
- Pasting files from the clipboard on Windows reads the real file list, and temporary clipboard images are cleaned up
- Image quality and resize values are validated; folder scans stop at 32 levels deep and 10,000 files
- The update notice keeps the update installable after it hides itself, the Thumbnail preset shows its resize box ticked, the Opus option shows its `.ogg` extension, and a batch where every file failed shows the failure count
- Space no longer starts a batch while a menu, button or dialog has focus, or when the key is held down

## [1.2.1] — 2026-09-24


### Performance

- **Native FFmpeg on Apple Silicon**: the macOS builds shipped an Intel-only FFmpeg and ffprobe, so every video, audio, and GIF job on an M-series Mac ran under Rosetta. Each macOS build now bundles FFmpeg for its own architecture, and the release checks every sidecar's architecture and required encoders before building
- **Smaller macOS app**: Ghostscript ships only the slice for the build's architecture, and the native FFmpeg builds are smaller too. Bundled tools on Apple Silicon went from 222 MB to 161 MB
- **Files added mid-run start right away**: each media type picks up new files as soon as its own batch finishes, so photos dropped in during a long video encode no longer wait for the video
- **GIF conversion memory is bounded**: short clips still convert in one pass; longer or larger ones use two passes instead of buffering every frame (a 2-minute 1080p clip at original width peaked at 11.6 GB, now about 150 MB)
- **AVIF with metadata kept is about 6× faster** (libaom now runs at `cpu-used` 6 with row threading and a per-job thread budget)
- **Faster thumbnails**: JPEGs decode at reduced size (about 1.8× faster, far less memory), HEIC thumbnails use a QOI intermediate instead of PNG (about 4× faster), and thumbnails appear one by one instead of waiting for the slowest in the request
- PNG encoding no longer copies the full pixel buffer; HW encoder checks at startup run concurrently; all file probes share one concurrency limit; video resizing rounds to even dimensions in a single scaler

### Bug Fixes

- **Audio compression and cover art**: embedded album art is kept as-is (MP3, FLAC, M4A) or dropped (Opus, WAV) instead of being re-encoded. Previously M4A files with cover art failed to compress, MP3/FLAC outputs were often larger than the input (so nothing was saved), and Opus outputs gained a video track
- **AV1 on macOS**: the bundled FFmpeg had no SVT-AV1 encoder, so AV1 compression failed on every Mac
- **AVIF with metadata kept ignored the resize setting**
- An AVIF/HEIC file that failed to decode left its row spinning forever; it is now marked failed
- Audio files no longer show their cover art's dimensions as a resolution

## [1.2.0] — 2026-09-07

### New App Icon

- New mark across every platform icon, the favicon, and the in-app header: a chevron bearing down on a slab, pressing it thinner. Same gold-on-black palette as before, now on a rounded tile so it sits naturally in the macOS Dock and Windows taskbar

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
