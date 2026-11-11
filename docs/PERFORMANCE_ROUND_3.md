# Compressions: Performance, Round 3

Status: **implemented** on branch `perf/compression-speed`. Sections 1–10 and the work packages
are the plan as written before the work; [Results](#results) at the end records what was built,
what the re-measurements showed, and where the outcome differs from the plan.
Scope: everything after v1.2.2. `docs/OPTIMIZATION_PLAN.md` and `docs/EFFICIENCY_WINS.md` cover
rounds 1 and 2; all of that shipped and is not repeated here.

Round 2 was measured on a 4-core Linux sandbox. This round was measured on the kind of machine
the earlier rounds could not reach: Windows 11, i9-13900K (8 P-cores + 16 E-cores, 32 threads),
64 GB RAM, RTX 4090, with the bundled sidecars and the files in `test_folder/`. Almost every
finding here is about what happens on a many-core, hybrid-CPU, GPU-equipped machine.

Every number below was measured in this session unless it says otherwise. Where something could
not be measured (macOS, the app's own window in the foreground), the item says so.

---

## Summary

| # | Finding | Measured effect | Effort |
|---|---------|-----------------|--------|
| 1 | Windows runs the FFmpeg/Ghostscript sidecars (and the app itself when it is not the focused window) mostly on **efficiency cores** | x264 1440p: 105–112 fps → 153 fps. x265: 66 → 99 fps. AVIF batch at today's settings: 1.4 → 2.3 img/s | S |
| 2 | **NVENC fails on 10-bit sources** with H.264 selected, falls back to CPU, then **disables the GPU encoder for the session** | Every 10-bit clip (game capture, HDR phone video) is CPU-encoded on a machine with an RTX 4090 | S |
| 3 | Videos run one at a time even on NVENC, which is not CPU-bound | Two at once on NVENC: 173 → 334 fps (HEVC 1440p), 273 → 548 fps (H.264 1080p). Software on 32 threads: +20–47% | S–M |
| 4 | NVENC `-cq` is fed the CRF number unchanged, but the scales differ | GPU output is **~2× the size** of CPU output at the same slider value, at a much higher quality than asked for | S (needs a decision) |
| 5 | Image batch is capped at 8 workers regardless of core count | 8 → 32 workers: JPEG 33 → 70 img/s, WebP 24 → 57, PNG 9.0 → 16.9. AVIF does not gain and stays at 8 (see Results) | S–M |
| 6 | Audio capped at 4 at once, PDF at 3 | Audio 2.2 → 5.6 files/s (16 at once). PDF 1.5 → 4.7–5.3 files/s (12 at once) | S |
| 7 | **EXIF orientation is dropped without rotating the pixels** (default settings) | Photos tagged as rotated come out sideways. Reproduced | S |
| 8 | **ICC colour profile is always dropped** (JPEG, WebP), with metadata kept or stripped | Display P3 photos (every recent iPhone) lose their profile and render desaturated. Reproduced on `IMG_2229.jpg` | S |
| 9 | Bundled FFmpeg is an unpinned nightly `master` build | It **crashed 4 times in roughly 60 libx264 runs** (3 while two encodes overlapped, 1 alone). The test machine may be at fault instead (see Results) | S–M |
| 10 | Small items: two GIF builders miss `-nostdin`; AV1 has no GPU path | See below | S |

Also below: the three open PRs, ideas that were measured and rejected, and the work packages.

---

## The three open PRs

None of the three branches should be merged as they are. Their content is on this branch where it
was still needed (commit `cc47bdf`).

| PR | Verdict | Why |
|----|---------|-----|
| #13 "Fix 26 confirmed code-review findings" | **Close, nothing to bring in** | It already landed through the v1.2.1 release (#14). Of the lines it adds, 93% are in `main` verbatim; the rest were rewritten by later work (#16 queue order and real Pause, #17). Spot-checked: `cancelled_jobs`, `validate_image_options`, `OutputClaim`, `NativeJobGuard`, `-nostdin`, `sanitizePathSegment`, `truncate_at_char_boundary` are all present. GitHub marks it conflicting for the same reason. |
| #11 cargo (tauri, rand, serde_with, tar) | **Close, superseded** | The bumps are wanted (each has an open advisory), but its `Cargo.lock` predates v1.2.2: merging it would drop `jpeg-decoder` and set the package back to 1.2.0. Reproduced on the current lockfile with the same resolution Dependabot's CI tested, plus `rustls` 0.23.46 (three newer advisories it does not cover). |
| #10 npm (postcss, vite, babel, browserslist, undici) | **Close, superseded** | Its `package-lock.json` predates the pipeline view: merging it would remove `@xyflow/react`. It also stops short of what the open alerts need (vite 6.4.4, undici 7.30.1, vitest 4.1.12). Reproduced and taken further: `npm audit` now reports 0 vulnerabilities. |

One thing the cargo PR would have broken by itself: the Tauri CLI refuses to build when the `tauri`
crate and `@tauri-apps/api` are on different minor versions. The crate goes to 2.11, so
`@tauri-apps/api` and `@tauri-apps/cli` are now pinned to `~2.11` in `package.json`. **Not verified:**
a full `tauri build` on the bumped set. It is the first step of the last work package.

Checked after the bumps: `cargo fmt --check`, `cargo clippy --lib -- -D warnings`, `cargo test --lib`
(143 passed), `npx tsc --noEmit`, `npm run test:run` (105 passed), `npm run version:check`.

One advisory stays open: `glib` < 0.20 (Linux GTK3 bindings pulled in by Tauri). There is no fix
within Tauri 2.

---

## 1. Windows parks the sidecars on the efficiency cores

**What I saw.** Scaling tests stopped improving at 16 workers on a 32-thread machine. Per-core
sampling showed why: all 16 P-core threads idle and all 16 E-cores at 100%.

```
            P-core threads (0-15)                       E-cores (16-31)
21:31:39    8  -1  0 22 25  0  8  0 13  3  2 13 0 0 0 0   100 100 100 100 100 100 100 100 100 100 ...
```

Windows 11 classifies a process with no focused window as background work and, on a hybrid CPU,
steers it to the efficiency cores. The sidecars are windowless child processes, so they are always
in that class. The app's own process falls into it whenever the user switches to another window,
which is what people do during a long batch.

A process can opt out with one call: `SetProcessInformation(ProcessPowerThrottling)` with
`ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED` and `StateMask = 0`.

**Measured**, same commands, default scheduling vs the opt-out applied to the process:

| Workload | Default | Opted out |
|----------|---------|-----------|
| FFmpeg libx264 `-preset fast`, 1440p60 10-bit source | 105–112 fps (P 15%, E 93% busy) | 153–154 fps |
| FFmpeg libx264 `-preset fast`, 1080p60 8-bit source | 199–240 fps | 256 fps |
| FFmpeg libx265 `-preset fast`, 1440p60 | 60–66 fps | 99 fps |
| FFmpeg libsvtav1 preset 7, 1440p60 | 94 fps | 100 fps |
| PNG batch, today's 8 workers × 4 threads (native) | 6.3 img/s | 9.0 img/s |
| AVIF batch, today's 8 workers × 4 threads (native) | 1.41 img/s | 2.30 img/s |
| One 7 MP AVIF, 4 threads (native) | 3.2 s | 1.85 s |
| JPEG batch, 32 workers (native) | 39 img/s | 70 img/s |

The demotion is intermittent: a fresh process sometimes gets all cores for its first seconds.
With the opt-out it got all cores in every run. GPU encodes are unaffected (173 fps either way).

**Not verified.** These runs were started from a background shell, which is the same situation as
a sidecar but not the same as the app's window in the foreground. The work package starts by
confirming it in the real app (encode fps from the logs, window focused vs minimized).
Setting the opt-out on the parent only gave a partial gain (123–138 fps), so set it on each child.

**Change.**
- At startup, opt the app's own process out (covers native image encodes).
- After each sidecar spawn, opt the child out by PID (`CommandChild::pid()`): `OpenProcess(PROCESS_SET_INFORMATION)`,
  `SetProcessInformation`, `CloseHandle`. One helper, called from `run_sidecar` (`commands/job.rs`),
  `run_ffmpeg_helper` (`commands/image.rs`), and the thumbnail and probe spawns.
- `#[cfg(windows)]` only; a no-op elsewhere. Log a warning if the call fails and carry on.
- macOS has the same P/E split on Apple Silicon. Not measured; do not change anything there without a Mac.

**Effort** S. **Risk** low. It only tells the scheduler this work is not background work.

---

## 2. NVENC fails on 10-bit input and takes the GPU encoder down with it

**Reproduced** with the app's exact arguments on `test_folder/DayZ … .mp4` (HEVC Main 10, 1440p60):

```
ffmpeg -i clip.mp4 -c:v h264_nvenc -rc vbr -cq 23 -b:v 0 -preset p5 …
[h264_nvenc] No capable devices found          exit 127
```

H.264 NVENC has no 10-bit mode, and `build_video_args` only adds `-pix_fmt yuv420p` on the software
path (`ffmpeg/args.rs:83`). What happens next, in `commands/video.rs`:

1. The GPU attempt fails.
2. The file is retried in software, and succeeds.
3. Because software succeeded, `disable_hw_encoder` removes `h264_nvenc` **for the rest of the session**.

So one 10-bit file silently turns GPU encoding off for every later file. The existing output in
`test_folder/compressed/` confirms it: its encoder tag is `libx264`.

Adding `-pix_fmt yuv420p` makes the same command succeed (exit 0). `hevc_nvenc` and `av1_nvenc`
accept the 10-bit input as it is.

**Change.** Add `-pix_fmt yuv420p` for H.264 hardware encoders too (`h264_nvenc`, `h264_videotoolbox`).
Leave HEVC alone so 10-bit sources stay 10-bit. Add an arg-builder test. **Effort** S. **Risk** low.

**Related, not in scope:** that clip is HDR (BT.2020, PQ). Converting it to 8-bit H.264 without tone
mapping gives washed-out colours on both the CPU and GPU paths. Worth its own decision later
(for example, suggest H.265 when the source is HDR).

---

## 3. Two NVENC encodes at once double throughput

`compress_videos_batch` runs one file at a time because "libx264/x265/SVT-AV1 already saturate
every core". That is right for software and wrong for the GPU.

| Encoder, source | 1 at once | 2 at once | 3 at once | 4 at once |
|-----------------|-----------|-----------|-----------|-----------|
| hevc_nvenc, 1440p60 | 173 fps | **334–342 fps** | 343 fps | 351 fps |
| h264_nvenc, 1440p60 | 172 fps | **316–324 fps** | 344 fps | — |
| h264_nvenc, 1080p60 | 273 fps | **548 fps** | 581 fps | 625 fps |
| libx264 fast, 1440p60, all cores (item 1 applied) | 153 fps | 189 fps (+23%) | — | — |
| libx264 fast, 1080p60, all cores | 256 fps | 377 fps (+47%) | — | — |
| libx265 fast, 1440p60, all cores | 99 fps | 119 fps (+20%) | — | — |
| libx264 fast, 1440p60, today's scheduling | 115 fps | 114 fps | 121 fps | — |
| libsvtav1 p7, 1440p60, today's scheduling | 107 fps | 77 fps | — | — |

The GPU doubles: it has two encoder chips and the CPU is mostly idle. Software gains a little on
this 32-thread machine once item 1 lets it use every core, because x264 and x265 do not fill 32
threads alone; SVT-AV1 gets worse.

**Change.** Run 2 videos at once when a hardware encoder will be used for the batch's codec, 1
otherwise. `run_batch` already takes the concurrency as a parameter and starts files in input order.
Optional follow-up, only after item 1 and only with a measurement on a smaller machine: 2 at once
for libx264/libx265 when there are 16 or more logical cores.

Things to handle:
- If the GPU encoder gets disabled mid-batch (a real failure), the second slot now runs a second
  software encode. Acceptable; it only costs contention until the batch ends.
- Cards with one encoder chip will not double. This RTX 4090 has two. Expect no loss there, not a gain.
- VideoToolbox: not measured. Keep macOS at 1 until someone measures on a Mac.
- The pipeline view labels the video lane "FFmpeg · one at a time" (`PipelineView.tsx`); update it.

**Effort** S–M. **Risk** low–medium (ordering and cancel paths; `run_batch` already handles N > 1 for audio).

---

## 4. NVENC output is twice the size it needs to be

The quality slider's CRF value is passed to NVENC as `-cq` unchanged. The two scales are not the
same: at the same number NVENC targets a much higher quality and spends twice the bits.

Measured with VMAF (higher is better) on two clips, default settings (`crf 23`):

| Encode | 1080p60 game clip | 720×1280 phone clip |
|--------|-------------------|---------------------|
| libx264 fast, crf 23 | 11.9 MB, VMAF 91.4 | 1356 KB, VMAF 96.5 |
| h264_nvenc, cq 23 (**today**) | 24.5 MB, VMAF 98.2 | 2579 KB, VMAF 97.9 |
| h264_nvenc, cq 28 | 14.3 MB, VMAF 94.9 | 1440 KB, VMAF 96.7 |
| h264_nvenc, cq 30 | 11.3 MB, VMAF 92.9 | 1161 KB, VMAF 96.0 |
| libx265 fast, crf 23 | 7.3 MB, VMAF 89.7 | 872 KB, VMAF 96.1 |
| hevc_nvenc, cq 23 (**today**) | 20.7 MB, VMAF 97.4 | 2139 KB, VMAF 97.8 |
| hevc_nvenc, cq 30 | 9.2 MB, VMAF 92.9 | 943 KB, VMAF 96.1 |
| hevc_nvenc, cq 33 | 6.3 MB, VMAF 89.4 | 670 KB, VMAF 94.0 |

**Proposed mapping:** `cq = crf + 5` for H.264 NVENC and `cq = crf + 7` for HEVC NVENC (clamped to 51).
On both clips that still scores at or above the software encoder at the same slider value, and the
files come out **42–44% smaller (H.264) and 56% smaller (HEVC)** than today's GPU output.

**This needs a decision** because it changes what existing GPU users get: smaller files at lower
(but still software-equivalent) quality for the same slider position. Two clips is a small sample;
the work package repeats the VMAF check on two more before settling the constants.
VideoToolbox's `-q:v` mapping was not measured.

**Effort** S. **Risk** low technically; it is a product choice.

---

## 5. Image batches use 8 workers on any machine

`compress_images_batch` caps workers at `min(cores, 8)` and splits threads `cores / workers`.
On 32 threads that is 8 jobs × 4 encoder threads, which leaves most of the machine idle for
JPEG and WebP (single-threaded encoders) and is slower than more, narrower jobs for PNG and AVIF.

Measured on `IMG_2229.jpg` (7 MP) and `IMG_3172.png`, full jobs (decode, encode, write), with all
cores available (item 1 applied to the test process, so this gain is on top of item 1):

| Format | Today (8 workers) | 16 workers | 32 workers, 1 thread each | Peak memory per job |
|--------|-------------------|------------|---------------------------|---------------------|
| JPEG q80 | 33.1 img/s | 51.9 img/s | **70.3 img/s** (2.1×) | ~64 MB (~9 bytes/pixel) |
| WebP q80 | 23.5 img/s | 39.2 img/s | **56.7 img/s** (2.4×) | similar to JPEG |
| PNG | 9.0 img/s (8 × 4 threads) | 13.7 img/s (16 × 2) | **16.9 img/s** (1.9×) | ~36 MB (~9.5 bytes/pixel) |
| AVIF q80 | 2.30 img/s (8 × 4 threads) | 2.65 img/s (16 × 2) | **3.52 img/s** (1.5×) | ~219 MB (~31 bytes/pixel) |

A single file still benefits from threads: one PNG takes 1010 ms on 1 thread and 822 ms on 4; one
7 MP AVIF takes 5.1 s on 1 thread, 1.85 s on 4 and 1.0 s on 32. So the existing rule (threads =
cores ÷ jobs in flight) is right; only the worker cap is wrong.

**Change.**
- Workers = logical cores, bounded by memory rather than by a constant. The frontend already sends
  each file's resolution. Use the largest image in the batch and a per-format cost
  (10 bytes/pixel; 32 for AVIF) against a fixed budget of about 4 GiB; assume 24 MP when the
  resolution is unknown; never fewer than 2 workers. A 48 MP JPEG batch then gets ~9 workers
  instead of 32, and a low-memory machine is safer than it is today (today 8 × 48 MP AVIF can
  reach ~12 GB).
- Encoder threads stay `cores / in-flight jobs`, so a batch of two AVIFs still gets 16 threads each.
- One pure function decides `(workers, threads)`, with unit tests next to the existing
  `image_encoder_threads` tests.

**Effort** S–M. **Risk** medium: memory. The budget above is the guard; verify peak memory with a
batch of the largest images available.

---

## 6. Audio and PDF caps

Round 2 left this open ("needs measuring on a many-core machine"). Measured with the app's exact
arguments: 32 four-minute MP3s (320k → 192k) and 24 PDFs (`/ebook`).

| At once | Audio | | At once | PDF |
|---------|-------|-|---------|-----|
| 4 (**today**) | 2.20 files/s | | 3 (**today**) | 1.50–1.64 files/s |
| 8 | 3.73 files/s | | 6 | 3.01–3.24 files/s |
| 16 | **5.57 files/s** (2.5×) | | 12 | **4.65–5.29 files/s** (3.1×) |
| 31 | 5.51 files/s | | 24 | 4.62–5.38 files/s |

Both stop improving at half the logical cores, with or without item 1.

**Change.** In `single_threaded_batch_concurrency`, replace the fixed caps with half the logical
cores: audio `cap = (cores / 2).clamp(4, 16)`, PDF `cap = (cores / 2).clamp(3, 12)`, still limited
to `cores - 1`. Machines with 8 threads or fewer behave exactly as today. Ghostscript memory on
very large PDFs is the reason PDF tops out lower. **Effort** S. **Risk** low.

---

## 7. Rotated photos come out sideways

Nothing in `src-tauri/src` reads EXIF orientation. With "strip metadata" on (the default) the tag
is removed and the pixels are left as they were, so a photo that relied on the tag is now sideways.

**Reproduced** with a 400×200 JPEG tagged Orientation 6 (viewers show it 200×400):

```
strip_metadata=true  (default): output pixels 400x200, exif kept=false   ← displays rotated
strip_metadata=false          : output pixels 400x200, exif kept=true    ← displays correctly
```

Thumbnails take the same path (`commands/thumbnail.rs` decodes without orientation and writes a
JPEG with no EXIF). Not reproduced separately; same code pattern.

**Change.** `image` 0.25.10 (already in the lockfile) has `ImageDecoder::orientation()` and
`DynamicImage::apply_orientation()`. Apply the orientation whenever the output will not carry the
tag: always for PNG, AVIF and thumbnails; for JPEG and WebP when metadata is stripped. When EXIF is
copied across, leave the pixels alone (as today), or the image would be rotated twice.
Add tests for both modes. **Effort** S. **Risk** low.

---

## 8. Colour profiles are dropped

`IMG_2229.jpg` carries a 536-byte ICC profile. None of the outputs do:

```
photo -> jpg,  strip_metadata=true : icc profile kept=false
photo -> jpg,  strip_metadata=false: icc profile kept=false
photo -> webp, strip_metadata=true : icc profile kept=false
photo -> webp, strip_metadata=false: icc profile kept=false
```

Without the profile a Display P3 photo is read as sRGB and looks flatter. "Keep metadata" only
copies EXIF (`copy_exif_jpeg`, `copy_exif_webp`).

**Change.** Read the profile from the decoder (`ImageDecoder::icc_profile()`) and write it to JPEG
and WebP with `img-parts` (`set_icc_profile`), and to PNG with oxipng. Keep it even when metadata
is stripped: it is colour data, not personal data, and it is small. Native AVIF (`ravif`) cannot
embed one; leave that documented.

Do it together with item 7, and take EXIF from the decoder too (`exif_metadata()`). That removes
the second full read of the input file and two full copies of the output that `copy_exif_*` make
today. **Effort** S. **Risk** low. Costs nothing measurable.

---

## 9. The bundled FFmpeg is a nightly build, and it crashed

`scripts/download-ffmpeg.ps1` fetches `ffmpeg-master-latest-win64-gpl.zip`: whatever `master` was
that day (this checkout has `N-123884`, 2026-04-09). Each release therefore ships a different,
untested FFmpeg.

While benchmarking, that build crashed (segmentation fault / access violation) **4 times in
roughly 60 libx264 runs**: three while two encodes overlapped and one running alone. Same inputs
and arguments as the runs that succeeded. No crash was seen with NVENC, libx265 or SVT-AV1. The
app reports a crash as a failed file and removes the output, so on this build a software H.264
encode can fail at random. The release you have installed may carry a different nightly.

**Change.** Pin all three platforms to a release-branch build (BtbN publishes them per branch) and
record the version in the script. Then run the overlap test 30 times, and 30 solo encodes, and expect 0 crashes:

```bash
for r in $(seq 30); do for i in 1 2; do ffmpeg -nostdin -y -v error -i clip.mp4 -c:v libx264 -preset fast -pix_fmt yuv420p -crf 23 -c:a aac out_$i.mp4 & done; wait; done
```

`check-sidecars.sh` already verifies the encoder list, so a missing encoder in the pinned build
fails CI. **Effort** S–M. **Risk** medium: a different FFmpeg needs the manual checks in `PLAN.md` rerun.
Needs the maintainer, because it downloads binaries.

---

## 10. Small items

- **`-nostdin` is missing** from `build_gif_palette_args` and `build_gif_paletteuse_args`
  (`ffmpeg/args.rs:316`, `:335`). Every other spawn passes it, and CLAUDE.md says all do. Add it and a test.
- **AV1 has no GPU path.** `av1_nvenc` works on this card: 171 fps against 67–100 fps for SVT-AV1
  preset 7, on the 1440p clip. Its `-cq` scale was not calibrated (item 4 method applies). Optional;
  do it only with a VMAF-checked mapping.
- **Startup race, existing:** hardware detection runs in the background at launch, so a video
  started in the first second uses software. Left alone.

---

## Measured and not worth doing

- **GPU decode (`-hwaccel cuda`).** 162 → 164 fps (H.264), 178 → 184 fps (HEVC) on the 1440p clip, and
  no change at 1080p. Keeping frames on the GPU with `scale_cuda` was slower (110 fps). Not worth the
  fallback handling.
- **Frontend store and render path.** `updateProgress` on a 500-file store takes 85 µs, and the
  backend sends at most ~4 events per second per job. Same conclusion as round 2.
- **Faster JPEG settings.** Turning off mozjpeg's scan optimization: 197 → 124 ms per 7 MP image for
  +2.0% file size. It is a trade, not a win; left as is. The "fastest" profile is 4–20× quicker but
  45–60% larger.
- **WebP threads or method.** `thread_level 1`: 316 vs 320 ms. Method 3: same size, same time. Method 2: twice as fast, +5.6% size.
- **More than two videos at once.** NVENC is flat from 2 to 4, and SVT-AV1 loses 28% at 2 (table in item 3).

---

## Work packages

Sized for one agent each. They are grouped by the files they own so two agents never edit the same
file. Packages A, B and C can run in parallel; D and E are independent of all three; F is last.

Every package: keep `cargo fmt --check`, `cargo clippy --lib -- -D warnings`, `cargo test --lib`,
`npx tsc --noEmit` and `npm run test:run` green, add tests beside the change, no `Co-Authored-By`
lines, and report the before/after number it was asked for. No new abstractions beyond what the
package names.

### A. Scheduler opt-out and sidecar caps (items 1, 6)
Owns: a new small `src-tauri/src/` module for the Windows call, `commands/job.rs`, `commands/audio.rs`,
`commands/pdf.rs`, `commands/probe.rs`, `lib.rs`.
1. First confirm item 1 in the real app: compress the 1440p clip with H.264 while the window is
   focused and again while minimized; compare fps from the logs. Report both.
2. Add the opt-out helper (`#[cfg(windows)]`, no-op elsewhere), call it for the app process at
   startup and from `run_sidecar` after spawn. Expose it for package B and C's spawn sites, or call
   it from the shared spawn helpers they already use.
3. New caps in `single_threaded_batch_concurrency` with unit tests for 4, 8, 16 and 32 cores.
Done when: the minimized-window x264 run is within 10% of the focused run and near 150 fps on the
1440p clip, and a 32-file MP3 batch is at least 2× faster than before on this machine.

### B. Video (items 2, 3, 4, 10)
Owns: `ffmpeg/args.rs` (all of it, including the two GIF builders), `ffmpeg/probe.rs`,
`commands/video.rs`, `commands/gif.rs`, the video lane label in `src/components/pipeline/PipelineView.tsx`.
1. `-pix_fmt yuv420p` for H.264 hardware encoders; test with the 10-bit clip that the GPU encoder
   stays enabled afterwards.
2. `-nostdin` on the two GIF builders.
3. Concurrency 2 for NVENC batches.
4. Only if the maintainer approved item 4: the `cq` offsets, after repeating the VMAF check on two more clips.
5. Optional, same condition: `av1_nvenc`.
Done when: the 10-bit clip encodes on the GPU with H.264 selected and the GPU encoder is still
enabled afterwards, and four 1440p clips with H.265 finish in about half the wall time.

### C. Native image pipeline (items 5, 7, 8)
Owns: `compression/image.rs`, `commands/image.rs`, `commands/thumbnail.rs`.
1. Orientation and ICC first (correctness), with EXIF taken from the decoder.
2. Worker and thread plan with the memory budget.
3. Rerun `cargo run --release --example perf_probe -- all …` and report the `batch` and `orient` sections.
Done when: the orientation and ICC lines in `perf_probe orient` read correctly in both modes, and
on this machine JPEG and WebP batches reach at least 1.8× today's throughput, PNG 1.6× and AVIF 1.3×,
with peak memory under the budget. Measure with the process opted out of power throttling (item 1),
or the numbers will be the efficiency-core ones.

### D. Pin FFmpeg (item 9)
Owns: `scripts/download-ffmpeg.ps1`, `scripts/download-ffmpeg.sh`, `scripts/check-sidecars.sh`,
`.github/workflows/`. Needs the maintainer to run the downloads. Done when the 30-round overlap
test has 0 crashes and the `Sidecars` workflow passes on all targets.

### E. Close out the PRs
Close #10, #11 and #13 with a pointer to this branch. Maintainer action.

### F. Integration (last)
1. `npm run tauri build` on the bumped dependencies (the Tauri 2.11 pairing has not had a full build).
2. Run the manual checks in `.claude/PLAN.md` that touch video, images and cancel.
3. Update `CHANGELOG.md`, the status in `.claude/CLAUDE.md` and `.claude/PLAN.md`, the two
   convention lines this changes (video is no longer always sequential; image cap is no longer 8),
   and fill in the status table below.

---

## How the numbers were produced

- Images: `src-tauri/examples/perf_probe.rs` (committed with this plan), release profile, on
  `test_folder/IMG_2229.jpg` and `IMG_3172.png`. Per-phase scaling used a standalone copy of
  `compression/image.rs` pinned to this `Cargo.lock`.
- Video: the bundled FFmpeg with the argument lists `build_video_args` produces, on a 20-second cut
  of the DayZ clip (1440p60 HEVC 10-bit) and an 8-bit 1080p60 transcode of it. 1200 frames each.
- Quality: FFmpeg's `libvmaf` filter against the encode's own source.
- Audio and PDF: the bundled FFmpeg and Ghostscript with the app's argument lists, N processes at once.
- Core usage: Windows performance counters, one sample per second per logical processor.

Anything launched from a background shell on this machine is subject to item 1. The tables say
which scheduling each number was taken under; the first image-batch run of this session was
unknowingly on efficiency cores only and was redone. Run-to-run variance was a few percent.

## Results

Every item is implemented on this branch. Checks on the final tree: `cargo fmt --check`,
`cargo clippy -- -D warnings`, `cargo test --lib` (169 passed, 1 ignored), `npx tsc --noEmit`,
`npm run test:run` (105 passed), and `npm run tauri build` (the app, MSI and NSIS installers
build on Tauri 2.11; only the updater signature step fails locally, for lack of the private key).

### Checked in the built app

The release build was driven through its real UI (files dropped, Compress and Cancel All
clicked) on the test machine: i9-13900K, RTX 4090, bundled FFmpeg 9.0.2.

| Check | Result |
|-------|--------|
| Startup | Log shows the power-throttling opt-out and `Usable HW encoders {h264_nvenc, hevc_nvenc, av1_nvenc}` |
| 4 × 20 s 1440p60 10-bit clips, H.264 | `encoder=h264_nvenc concurrency=2`, two FFmpeg processes throughout, 16.5 s for all four (about 154 fps each). Output is 8-bit H.264; the GPU encoder stayed enabled |
| Same four clips, AV1 | `encoder=av1_nvenc concurrency=2`, 19.5 s. Output is 10-bit AV1 |
| Cancel All with two AV1 encodes in flight | Both stopped, nothing else started, no FFmpeg process left, no software retry, partial outputs removed; the next batch still used `av1_nvenc` |
| 65 × 7 MP JPEGs | Plan `workers=32 encoder_threads=1`; all 65 done in 1.0 s; peak memory 2.05 GB |
| 16 × 48 MP JPEGs | Plan `workers=8 encoder_threads=4`; done in 2.8 s; peak memory 3.4 GB (budget 4 GiB) |
| Photo tagged Orientation 6 | Output is 200×400, upright |
| `IMG_2229.jpg` | Output keeps the 536-byte ICC profile |
| 32 MP3s and 24 PDFs together | 16 audio and 12 PDF jobs at once: 4.8 audio files/s and 3.2 PDFs/s while sharing the machine (2.2 and 1.5 before, each measured alone) |
| Pipeline view | Video lane reads "FFmpeg · up to 2 at a time" and shows "2 running" |

### What changed from the plan above

- **Item 1, speed-up not reproduced after the change.** The opt-out is in place for the app
  process and for every encode, image-helper and thumbnail spawn (not the short ffprobe and
  encoder-detection spawns). Reading a child's state back shows it applied
  (`ControlMask=1, StateMask=0`). But on the day of implementation Windows was not demoting the
  test processes at all: libx264 on the 1440p clip ran 163–164 fps with and without the opt-out,
  three rounds each. The 105–112 fps figure above is from an earlier session. So the mechanism is
  verified and harmless, and the size of the gain in the real app is still unmeasured.
- **Item 4, mapping confirmed on two more clips and adopted.** `-cq = CRF + 5` for H.264 and
  `CRF + 7` for HEVC. On two further clips at CRF 18, 23 and 28, H.264 NVENC scored 0.1 to 1.4
  VMAF above libx264 at 0.76× to 1.32× its size; HEVC NVENC scored 0.3 to 0.6 below libx265 at
  0.99× to 1.53× its size. Against the old `-cq = CRF`, output is 29–35% smaller for H.264 and
  48–50% for HEVC on these clips (42–56% on the first two). Both new clips are cuts of the same
  game capture, so the sample is still narrow.
- **Item 5, AVIF keeps 8 workers.** Re-measured with 38 files: 8 workers × 4 threads 2.19 img/s,
  16 × 2 2.17, 19 × 2 2.11, 32 × 1 2.02, and 19 × 1 (what the 4 GiB budget picked) 1.39. The
  3.52 img/s figure in the item 5 table came from a batch of exactly 32 files on 32 workers, which
  hides the cost of the last files running on one thread each, and 32 workers needs about 7 GB at
  7 MP. JPEG, WebP and PNG gains held on the re-run: 34.0 → 69.5, 23.7 → 58.6 and 9.1 → 17.3 img/s.
- **Item 6, 8-thread machines.** PDF runs 4 at once there, not 3 as before: the cap is half the
  logical cores. Audio is unchanged up to 8 threads.
- **Item 7, pixels are always upright.** Keeping metadata no longer leaves the pixels unrotated:
  the pixels are rotated in every mode and the orientation tag in the copied EXIF is reset to 1,
  because many viewers ignore EXIF orientation in WebP. Thumbnails are upright too.
- **Item 8.** The ICC profile is kept for JPEG, WebP and PNG in both strip modes (536 bytes on
  `IMG_2229.jpg`; outputs grew by 554 and 562 bytes). A profile whose colour space does not match
  the output pixels (CMYK, or gray on RGB output) is dropped. EXIF now also survives a format
  change (JPEG to WebP) when metadata is kept. PNG output carries no EXIF.
- **Item 9, pinned, but the crash is not an FFmpeg-nightly problem.** Windows and Linux now fetch
  BtbN's `n9.0` release-branch build (9.0.2 on the day), which passes `check-sidecars.sh`. The
  done-when test failed: the pinned build also crashed in **2 of 90** libx264 encodes (exit 139,
  both while two encodes overlapped; 30 solo encodes were clean), the same rate as the master
  build. During implementation rustc also crashed five times on this machine under load (illegal
  instruction, LLVM errors, an impossible const-eval failure). Two FFmpeg builds seven months
  apart failing alike, plus random faults in an unrelated program, point at the test machine
  (an i9-13900K) and not at FFmpeg. Rerun the overlap test on a second machine to settle it. The
  pin stays because it makes releases reproducible. The `Sidecars` workflow has not run on this
  branch yet; it runs in CI on the PR.
- **Item 10, `av1_nvenc` added.** Calibrated on four clips against SVT-AV1 preset 7:
  `-cq = CRF + 5` scores within −0.5 to +1.0 VMAF at CRF 18 to 28 (it falls 1 to 1.7 behind at
  CRF 35). One encode runs 183 fps against 67–100 for SVT-AV1 on the 1440p clip; two at once
  270–304 fps. Used only when the startup verification encode passes (RTX 40-series and newer).

### Known gaps, not fixed here

- The gain from the scheduler opt-out in the real app (window focused against minimized, before
  and after this branch) is unmeasured.
- VideoToolbox: nothing here was run on a Mac. Its `-q:v` mapping is uncalibrated and it stays at
  one video at a time.
- AV1 at a given slider value is larger than H.264 at the same value, in software and now on the
  GPU alike, because SVT-AV1's CRF scale runs 0–63. The mapping preserves that; it does not fix it.
- AVIF and HEIC inputs are decoded by FFmpeg through a temporary QOI file, so their orientation,
  ICC profile and EXIF do not reach the output.
- Native AVIF output (`ravif`) carries no ICC profile.
- A 10-bit HDR source encoded to 8-bit H.264 keeps its HDR colour tags and looks washed out; this
  was already true on the software path.
- `benches/compression_bench.rs` fails `cargo clippy --all-targets` (it did before this branch).
- One Cargo advisory stays open: `glib` < 0.20, pulled in by Tauri 2 on Linux.

## Implementation status

| Item | Status |
|------|--------|
| Dependency bumps (replaces #10, #11) | Done, `cc47bdf` |
| 1. Scheduler opt-out | Done, `1db68b9`, `7e98325`. Gain in the app unmeasured |
| 2. NVENC 10-bit | Done, `1eb5548`. Confirmed in the app |
| 3. NVENC two at once | Done, `aaf5131`. Confirmed in the app |
| 4. NVENC quality mapping | Done, `a9a6715` |
| 5. Image workers | Done, `275967a`, `ff2b7c8`. AVIF stays at 8 |
| 6. Audio/PDF caps | Done, `1db68b9`. Confirmed in the app |
| 7. Orientation | Done, `bd3d3e7`, `44d7bc7` |
| 8. ICC profile | Done, `bd3d3e7` |
| 9. Pin FFmpeg | Done, `5026d22`. Does not cure the crash |
| 10. Small items | Done: `-nostdin` `1eb5548`, `av1_nvenc` `a9a6715` |
| E. PRs #10, #11, #13 | Closed |
| F. Integration | Done: full build, in-app checks, CHANGELOG, `.claude/CLAUDE.md`, `PLAN.md` |
