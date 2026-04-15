# Changelog

All notable changes to Compressions are documented here.

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
