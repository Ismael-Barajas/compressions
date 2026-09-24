# Compressions: Efficiency Wins, Round 2

Status: **implemented on `claude/efficiency-wins-planning-4usvvn`, except where an item says otherwise**
(see [Implementation status](#implementation-status) at the end).
Scope: everything after v1.2.0 (`docs/OPTIMIZATION_PLAN.md` covers the first pass, and all of it shipped).

The earlier pass fixed most of the in-app waste: progress coalescing, O(1) store updates, history
batching, parallel audio/PDF, and more. For this round, I read the whole app again, inspected the
**published v1.2.0 release bundle**, and measured each candidate. The measurements used FFmpeg 6.1
and a standalone Rust bench pinned to the exact crate versions in `Cargo.lock`, on a 4-core Linux sandbox.
Each item says what was verified and how. Where nothing could be measured, the item says so.

---

## Summary

| # | Win | Who it hits | Evidence | Effort |
|---|-----|-------------|----------|--------|
| 1 | Apple Silicon build runs **x86_64 FFmpeg/ffprobe under Rosetta**; macOS FFmpeg also has **no SVT-AV1** | Every M-series Mac user, every FFmpeg job | Inspected the v1.2.0 `aarch64` bundle | S–M |
| 2 | Audio compression **re-encodes embedded cover art** (MP3/FLAC → PNG, M4A → H.264, Opus → Theora) | Anyone compressing a music library | Measured: typical MP3/FLAC saves **0%**, M4A **fails** | S |
| 3 | Single-pass GIF **buffers the whole clip in RAM** | Long clips or large widths in Tools → GIF | Measured: **916 MB** (default) / **11.6 GB** (original width) for 2 min | S |
| 4 | Drain loop waits for **all** media batches, so files added mid-run sit idle | "Drop more files while compressing" workflow | Code path | M |
| 5 | Thumbnails: full-size JPEG decode, PNG intermediate for HEIC, all-or-nothing batch reply | Thumbnail view | Measured: JPEG **1.8×**, HEIC intermediate **~4×** faster | S each |
| 6 | "Keep metadata" AVIF runs libaom at its slowest default speed, ignores resize, and decodes the input twice | AVIF output with metadata kept | Measured: **63.7 s → 10.9 s** per 3 MP image, same size | S |
| 7 | Bundle size: universal `gs` in single-arch builds, full duplicate `ffprobe` | Download and every auto-update | Inspected bundle: sidecars are 222 of 247 MB | S–M |
| 8 | Smaller items: PNG buffer clone, startup HW probe, concurrency caps, probe semaphore | Various | Code path | S |

Also below: **bugs found along the way** (one leaves a file row spinning forever) and **ideas checked and rejected**.

---

## 1. macOS Apple Silicon ships x86_64 FFmpeg (Rosetta), and FFmpeg on macOS has no AV1 encoder

**What I checked.** I downloaded `Compressions_aarch64.app.tar.gz` from the v1.2.0 release and ran `file` on the sidecars:

```
Contents/MacOS/compressions  Mach-O 64-bit arm64 executable
Contents/MacOS/ffmpeg        Mach-O 64-bit x86_64 executable     ← translated by Rosetta 2
Contents/MacOS/ffprobe       Mach-O 64-bit x86_64 executable     ← translated by Rosetta 2
Contents/MacOS/gs            Mach-O universal (x86_64 + arm64)
```

`scripts/download-ffmpeg.sh` fetches from evermeet.cx, which ships Intel-only builds. The release
workflow (`release.yml` → "Download FFmpeg (macOS)") then copies that same binary to both the
`aarch64-apple-darwin` and `x86_64-apple-darwin` sidecar names. The workflow comment says
"evermeet.cx provides universal binaries", but they are x86_64 only.

**Cost.** On Apple Silicon, every video encode, audio encode, GIF conversion, AVIF/HEIC decode,
video thumbnail and ffprobe runs through Rosetta 2. Rosetta translates x86 SIMD, and the codecs
(x264/x265/libaom/dav1d/lame/opus) lean on hand-written assembly. So their native NEON paths go
unused, and every process spawn pays translation overhead. VideoToolbox encoding still runs in
hardware, but decoding, scaling and the pipeline around it are translated. *Not measured here: that
needs an M-series Mac. Measure before/after on one 1080p x264 encode and one 100-file MP3 batch.*

**Found at the same time.** The bundled build's configure line has no `--enable-libsvtav1`, and the
binary has no `libsvtav1` encoder. `build_video_args` hardcodes `libsvtav1` for AV1
(`ffmpeg/args.rs`), so **AV1 compression fails on every macOS install**. Windows and Linux use BtbN
builds, which do include it.

**Change.**
- Fetch a native arm64 static build for `aarch64-apple-darwin` and an x86_64 one for
  `x86_64-apple-darwin`, and stop copying one binary to both names. Candidates to evaluate: Martin
  Riedl's build server (arm64 and x86_64 static macOS builds) and osxexperts.net (arm64). Neither
  was reachable from this sandbox, so check each one's encoder list before switching.
- Add a CI guard after the download step, per target: check that `file` reports the expected
  architecture, and that `ffmpeg -encoders` lists `libx264 libx265 libsvtav1 libmp3lame libopus`
  and `h264_videotoolbox`. That would have caught both problems, and it keeps them from coming back.
- Optional hardening: if `libsvtav1` is missing at runtime, fall back to `libaom-av1 -cpu-used 6 -row-mt 1` rather than failing.

**Effort** S–M (CI only). **Risk** low with the encoder guard in place.

---

## 2. Audio compression re-encodes embedded album art

**Today.** `build_audio_compression_args` (`ffmpeg/args.rs`) has no `-vn` and no `-c:v`. The
comment says the input "is already an audio file, not a video", but most music files carry a cover
image as an attached-picture video stream. FFmpeg then transcodes that stream to the output
muxer's default video codec.

**Measured** with the app's exact default arguments (Original format, 192k) on a 3-minute 320k
track. The first table uses a 375 KB photo-like JPEG cover:

| Input | App output today | With the fix | Result today |
|-------|------------------|--------------|--------------|
| MP3 7.58 MB | **7.76 MB** (cover → 3 MB PNG) | 4.70 MB (`-c:v copy`) | Output > input, so keep-original returns the file unchanged: **0% saved** after a full encode |
| FLAC 9.81 MB | **12.87 MB** (cover → PNG) | 9.81 MB (`-c:v copy`) | Same: wasted encode, "Already optimized" |
| M4A (AAC) 5.32 MB | **exit code 234** | 4.71 MB (`-c:v copy`) | ipod muxer picks H.264, libx264 encodes the cover, then the muxer rejects it: **compression fails** |
| MP3 → Opus (.ogg) | Opus **+ Theora video stream** | Opus only (`-vn`) | Output grows a one-frame video track |

With the synthetic 85 KB cover the effect is smaller but has the same sign: MP3 +214 KB, FLAC +214 KB.

**Change.** In `push_audio_encoding_args` (or the compression builder), add:
- MP3, FLAC, AAC/M4A: `-c:v copy`. The cover keeps its bytes and its `attached_pic`
  disposition (verified with ffprobe on all three).
- Opus/OGG, WAV: `-vn`. Tested: `-c:v copy` fails in the ogg muxer, and WAV cannot hold images.

Add arg-builder tests like the existing ones. **Effort** S. **Risk** low. This is the cheapest high-impact item on the list.

---

## 3. Single-pass GIF keeps every frame in memory

**Today.** `build_gif_single_pass_args` (shipped in 1.2.0) uses
`split[a][b];[a]palettegen[p];[b][p]paletteuse`. `palettegen` only emits its palette at end of
stream, so every filtered frame on branch `[b]` queues in memory until the source is fully read.
Memory grows as duration × fps × width × height.

**Measured** on a 2-minute 1080p clip at 12 fps:

| Width | Single-pass (today) | Two-pass (1.1.x approach) |
|-------|---------------------|---------------------------|
| 480 (default) | **916 MB** peak, 24.2 s | 150 MB peak, 27.8 s |
| Original (1080p) | **11.6 GB** peak, 171 s | 150 MB peak, **107 s** |

At the default width, single-pass saves about 13% of wall time for 6× the memory. At large sizes
it is both slower and dangerous: a 5-minute clip at original width would need about 30 GB.

**Change.** Pick the strategy per job from the estimated buffer, `duration × fps × out_w × out_h × 4`
bytes. Duration and resolution are already known from the probe. Use single-pass under a budget
(e.g. 256 MB), otherwise two-pass with a temp palette. Both paths already existed, so this is mostly
restoring the old builder next to the new one. **Effort** S. **Risk** low.

---

## 4. The drain loop waits for the slowest media type before picking up new files

**Today.** `startCompression` (`lib/compressionController.ts`) takes a snapshot of the queued files,
starts the video/image/PDF/audio batches, and waits for `Promise.allSettled` on all four before
looking at the queue again. The app explicitly supports adding files mid-run (drag-drop "works even
during compression"). But anything added after a round starts waits until the round's slowest
batch finishes. Example: during a 20-minute video encode, the user drops in 300 photos. The image
pipeline is idle, yet the photos wait all 20 minutes.

**Change.** Run one drain loop per media type. Each loop re-reads *its own* queued files when its
batch returns, and they share the pause/cancel checks and the operation counter. `runBatch` and
`entriesFor` already work per type, so the loop body mostly moves as-is. **Effort** M (plus tests
in `tests/lib/compressionController.test.ts`). **Risk** low–medium; the "queue did not advance"
guard needs to be per type.

---

## 5. Thumbnails

### 5a. Decode JPEGs at reduced size (1.8× faster per thumbnail)
`thumbnail_image` fully decodes each image to make a 160 px thumbnail. For JPEG, the decoder
can instead decode directly at 1/2, 1/4 or 1/8 scale in the DCT domain, picking the smallest
scale whose long side still covers 160 px.

Measured on a 12 MP (4032×3024) JPEG with `jpeg-decoder`: **~206 ms → ~116 ms** per
thumbnail, with identical 160×120 output. Peak decode memory drops from about 36 MB of RGB to
under 1 MB. Anything unusual (CMYK, decode errors) falls back to the existing full decode.

`mozjpeg` (already a dependency) was faster still, at 63 ms, but it reports corrupt input by
unwinding. Under the release profile's `panic = "abort"`, one broken JPEG would kill the app, so
it is not used for decoding. **Effort** S.

### 5b. HEIC thumbnail intermediate: PNG → QOI (~4× faster)
The compress path already switched its FFmpeg intermediate to QOI in 1.1.2, but
`thumbnail_via_decode` still writes a full-size **PNG** (`compressions_thumb_{hash}.png`). Measured
on a 12 MP frame: writing the intermediate costs **3.14 s as PNG vs 0.68 s as QOI**, and reading it
back costs 290 ms vs 162 ms. That makes about **3.4 s → 0.85 s** per HEIC thumbnail. Changing the
extension is the whole fix. **Effort** S.

### 5c. Stream thumbnail results instead of returning them all at once
`generate_thumbnails_batch` collects the whole `JoinSet` before replying, so one slow item holds
back every thumbnail on screen. A slow item is a video (up to the 10 s timeout) or a HEIC (5b).
Send each result over a `Channel` as it finishes, the way `probe_files_batch` already works.
**Effort** S–M.

### 5d. (Optional) Keep the cache across sessions
The cache is deleted on app exit and on "Clear", even though entries are already keyed by
path + size + mtime. Re-opening the same folder therefore regenerates everything. Keeping it
(bounded, e.g. LRU trim to 200 MB at startup) removes that repeat work. That trades against leaving
thumbnails of user files in the temp dir, which may be why it is deleted today. **Effort** S.

---

## 6. AVIF output with "keep metadata" uses the slowest libaom speed

When `stripMetadata` is off and the output is AVIF, `compress_avif_with_ffmpeg`
(`commands/image.rs`) runs `libaom-av1 -still-picture 1` with FFmpeg's default `cpu-used` of 1,
the slowest setting. It also sets no thread budget, even though up to 8 image jobs run at once.

Measured on a 2000×1500 image with the app's exact args (quality 80 → CRF 12):

| libaom settings | Wall time | Output |
|-----------------|-----------|--------|
| App today (`cpu-used` 1) | **63.7 s** | 203.9 KB |
| `-cpu-used 6 -row-mt 1` | **10.9 s** | 203.1 KB |

That is 5.8× faster with no size penalty on this image. Scaling linearly with pixel count (not measured), a 12 MP phone photo would take about 4 minutes today on this 4-core machine.

Two related issues on the same path:
- **Resize is ignored.** The FFmpeg command has no scale filter, so a resize setting is silently
  dropped when metadata is kept.
- **Double decode.** For AVIF or HEIC *inputs*, the FFmpeg decode to a temp QOI runs before the
  branch, even though this branch never reads the temp file.

**Change.** Add `-cpu-used 6 -row-mt 1 -threads <encoder_threads>`, apply the same resize as the
native path (`-vf scale=...`), and move the temp decode into the native branch. **Effort** S.
**Risk** low; stripping metadata stays the default.

---

## 7. Bundle and update size

The installed Apple Silicon `.app` is **247 MB**. The sidecars account for 222 MB of that:
`ffmpeg` 81 MB, `ffprobe` 81 MB, `gs` 60 MB, plus 18 MB of gs resources. Every auto-update downloads
the full bundle (86 MB compressed).

- **Thin `gs` per target.** The Koch `.pkg` binary is universal: x86_64 is 30.8 MB and arm64 is
  29.5 MB. Running `lipo -thin <arch>` in the macOS download step saves about **30 MB per Mac build**.
  **Effort** S.
- **ffprobe is a second full static FFmpeg.** It is used only for duration/resolution at
  add time, a duration fallback, and AVIF/HEIC dimensions. There are two ways to cut it:
  - Parse the stream header that `ffmpeg -hide_banner -i <file>` prints. Well-trodden, but the text
    format is less stable than JSON.
  - Use a slimmer FFmpeg build limited to the codecs the app uses. The evermeet build carries
    vmaf, rubberband, zmq, bluray, avisynth and more.

  Either one saves about 81 MB (a third of the app). **Effort** M. **Risk** medium; measure the
  parse approach against a corpus first.

---

## 8. Smaller items

- **PNG encode clones the full pixel buffer** (`encode_png`, `b.as_raw().clone()` ×4). Pass the
  `DynamicImage` by value and use `into_raw()`. The time saved is negligible next to oxipng, but
  peak memory drops by one frame per job: 96 MB for a 24 MP RGBA image, times up to 8 concurrent jobs. **Effort** S.
- **HW encoder probe at every launch** (`lib.rs` → `detect_hw_encoders`). One `ffmpeg -encoders`
  run plus a sequential test encode per listed HW encoder (two on the current macOS and Windows
  builds) happen on every start, even when no video is ever compressed. On Macs today each of those spawns also goes through Rosetta. Options: run it lazily
  on the first video batch, cache the result keyed by app version + FFmpeg binary mtime, or at
  least run the verifications concurrently. **Effort** S.
- **Audio/PDF concurrency caps** are `min(cores − 1, 4)` and `min(cores − 1, 3)`. On a 10–16 core
  machine that leaves most cores idle during a big MP3 batch. Raising the audio cap to about 8 is
  worth a measured try; disk contention is the thing to watch. **Effort** S (measure first).
- **`probe_files_batch` creates a new 6-permit semaphore per call.** Three quick drops mean 18
  concurrent ffprobes. Move it into managed state like `ThumbnailSemaphore`. **Effort** S.
- **Video scale filter chains two `scale` filters.** A single
  `scale=...:force_original_aspect_ratio=decrease:force_divisible_by=2` avoids a second resample
  when the first result is odd-sized. **Effort** S.

---

## Bugs found along the way

- **An AVIF/HEIC decode failure leaves the row spinning forever.** `compress_image` sends
  `Started` (status → processing), then `decode_*_via_ffmpeg(...)?` returns `Err` without an
  `Error` event. `compress_images_batch` turns that into a result with an **empty `input_path`**,
  so `runBatch` on the frontend cannot match it, and the file never reaches a terminal state.
  Fix: replace the hand-rolled loop in `compress_images_batch` with `job::run_batch`, which keeps
  the paths, and send `Error` on that early return.
- **M4A compression fails for files with cover art** (item 2).
- **AV1 fails on macOS** (item 1).
- **Resize is ignored for AVIF with metadata kept** (item 6).
- **Audio rows show the cover art's size as "resolution".** `probe_video_info` reports the first
  video stream, which for an MP3 is the embedded picture (e.g. "1200x1200").

## Checked and not worth doing

- **Faster PNG for clipboard pastes.** Measured on a 5K screenshot: `image` 0.25's default PNG
  encoder is already its fast path (47.5 ms vs 51.0 ms, byte-identical output). No change.
- **Frontend store and render path.** After 1.2.0, progress events are O(1) in the store, rows are
  memoized, and the counters are primitives. I found nothing further worth the churn.
- **Piping FFmpeg decodes through stdout instead of a temp QOI.** QOI writes are already cheap
  (0.68 s for 12 MP, most of that decoding). Not worth giving up the simple file-based path.

---

## Suggested order

| PR | Contents | Why first |
|----|----------|-----------|
| 1 | Item 2 (cover art), item 3 (adaptive GIF), the stuck-row bug | All size S, backend-only, measurable, and they fix user-visible failures |
| 2 | Item 1 (native arm64 FFmpeg + CI encoder/arch guard), item 7 `gs` thinning | CI-only change, largest reach on Macs |
| 3 | Item 5a–5c (thumbnails) | Self-contained in `commands/thumbnail.rs` + `FileList.tsx` |
| 4 | Item 6 (AVIF metadata path) + item 8 small items | Small, independent |
| 5 | Item 4 (per-type drain loops) | Touches the controller's core loop; wants its own tests |
| later | Item 7 ffprobe removal | Largest size win, but needs a parsing corpus |

Each PR should keep `npm run test:run`, `cargo test --lib`, `cargo clippy -D warnings` and
`cargo fmt --check` green, and add arg-builder or unit tests alongside the change.

---

## Implementation status

| Item | Status |
|------|--------|
| 1. Native FFmpeg per macOS target + sidecar guard | Done. `download-ffmpeg.sh <target>` fetches native builds from Martin Riedl's build server. `check-sidecars.sh` fails the release if a sidecar has the wrong architecture or FFmpeg lacks an encoder the app uses. A new `Sidecars` workflow runs both on PRs that touch the download setup. Verified in CI on PR #14: both targets get the right architecture and every required encoder, including `libsvtav1` and VideoToolbox. The Apple Silicon sidecars dropped from 222 MB to 161 MB (FFmpeg and ffprobe 81 → 66 MB each, `gs` 60 → 29.5 MB). The optional libaom fallback for a missing `libsvtav1` is not done; the guard makes it unnecessary for shipped builds. |
| 2. Audio cover art | Done: `-c:v copy` for MP3/FLAC/M4A and `-vn` for Opus/WAV. The generated args were run through FFmpeg: MP3 7.58 → 4.70 MB, M4A now succeeds (5.32 → 4.71 MB), and cover art keeps its `attached_pic` flag. |
| 3. GIF memory | Done. Single pass under a 256 MB buffer estimate, two passes otherwise. The generated two-pass args on the 2-minute clip peak at 149 MB, with the same GIF size. During pass 1, FFmpeg reports no progress (`palettegen` only emits at the end), so the row shows its indeterminate state until 50%. |
| 4. Per-type drain loops | Done: `drainQueue` in `compressionController.ts`, with tests for mid-run additions and pause. |
| 5a–5c. Thumbnails | Done. 5d (keep the cache across sessions) is not done; it trades against leaving thumbnails of user files in the temp dir. |
| 6. AVIF metadata path | Done: `-cpu-used 6 -row-mt 1 -threads`, resize applied, and no unused temp decode. |
| 7. Bundle size | `gs` thinning done. Dropping `ffprobe` is not done; it needs a parsing corpus first. |
| 8. Small items | Done: PNG buffer move, concurrent HW encoder checks, one shared probe semaphore, single-scaler video resize. Not done: raising the audio/PDF concurrency caps, which needs measuring on a many-core machine. |
| Bugs | Stuck row on AVIF/HEIC decode failure: fixed. Audio rows showing cover-art resolution: fixed. M4A, AV1 on macOS, and AVIF resize: see items 2, 1 and 6. |
