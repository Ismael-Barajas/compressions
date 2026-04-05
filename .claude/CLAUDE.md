# Compressions — Claude Code Context

## Project

Desktop media compression app. Tauri v2 + React + TypeScript frontend, Rust backend, FFmpeg/Ghostscript sidecars.

## Current Status

**See [PLAN.md](PLAN.md) for full implementation plan and progress tracking.**

Phase 12 (Testing & Performance Benchmarking) is complete. Resume from **Phase 13: Final Polish**.

## Stack

- **Frontend:** React 19, TypeScript, Tailwind CSS, Zustand
- **Backend:** Rust (Tauri v2), FFmpeg/FFprobe sidecars, Ghostscript (system-installed)
- **Image encoding:** mozjpeg, oxipng, webp, ravif, gif crates (native Rust)
- **State:** Zustand store (`src/stores/compressionStore.ts`)
- **Progress:** Tauri `Channel<ProgressEvent>` from Rust → frontend

## Key Conventions

- Every new Tauri command must be registered in 3 places: `commands/mod.rs`, `lib.rs`, and `src/lib/commands.ts`
- Videos compress sequentially (FFmpeg sidecar, CPU-bound); images compress in parallel (native Rust)
- Audio extraction uses FFmpeg sidecar (`-vn` flag); triggered via right-click context menu on video files
- Output path logic lives in `src/hooks/useCompression.ts` (`getOutputDirForFile`)
- File name template logic lives in `src/lib/fileUtils.ts` (`getOutputFileName`)
- PDF compression uses Ghostscript sidecar (sequential, indeterminate progress)
- `create_dir_all` is called in both `commands/image.rs` and `commands/video.rs` before writing output

## Dev Setup

```bash
# Install dependencies
npm install

# Download FFmpeg sidecars (Windows)
powershell scripts/download-ffmpeg.ps1

# Download FFmpeg sidecars (macOS/Linux)
bash scripts/download-ffmpeg.sh

# Download Ghostscript sidecar (Windows)
powershell scripts/download-gs.ps1

# Download Ghostscript sidecar (macOS/Linux)
bash scripts/download-gs.sh

# Run dev server
npm run tauri dev

# Build
npm run tauri build
```

Prerequisites: Node 18+, Rust (rustup), NASM (`choco install nasm` / `brew install nasm`)

## Grug Rules (Engineering Philosophy)

These rules are drawn from the grug-brained developer philosophy. Take them seriously.

**Complexity is the enemy.** Every abstraction, every layer, every indirection is a place for demons to hide. When complexity grows, things break in ways nobody understands. Fight it always.

**Say no.** The best code is code not written. Before adding a feature, pattern, or abstraction, ask if it's actually needed. If forced to say yes, deliver 80% of the value with 20% of the code.

**Don't abstract early.** Let the shape of the system emerge before cutting interfaces. Wrong abstractions made too early cause more pain than duplication. Sit with the duplication a while.

**Keep refactors small.** Don't venture far from shore. Large rewrites fail. Small, incremental changes survive. If a refactor is getting big, stop and ship what you have.

**Understand before touching.** Before removing or rewriting existing code, understand why it exists. Ugly code that works has earned its place. Don't tear it out willy nilly.

**Integration tests over unit tests.** Unit tests break on every refactor and miss inter-component bugs. Integration tests catch real problems. Don't mock deeply — mocking hides what's actually broken.

**Log everything important.** Log major branches, errors, and state transitions. A good log is worth more than a fancy debugger when things go wrong in production.

**Name intermediate results.** Break complex conditionals into named variables. Readability and debuggability beat cleverness every time.

**DRY is not a law.** Three similar lines is often better than a premature abstraction with five callbacks. Duplication you can see beats abstraction you can't understand.

**Profile before optimizing.** Never optimize without a real performance profile. Network calls cost more than nested loops. Measure first.

**Keep it where it's used.** Code that lives near where it's called is easier to understand than code split across files by type. Locality of behavior beats separation of concerns in small codebases.

**Say "I don't understand this" out loud.** Admitting complexity is confusing is not weakness — it keeps complexity demons from winning. If the code is hard to explain, that's the code's fault.

## Phase Completion Checklist

After completing every phase sub-task (e.g. 10.1, 10.2, 11.1):

1. **Verify TypeScript:** run `npx tsc --noEmit` — must pass with zero errors
2. **Verify Rust:** run `cargo check --manifest-path src-tauri/Cargo.toml` — must pass with zero errors
3. **Update CLAUDE.md:** change `Current Status` to reflect what just completed and what is next
4. **Update PLAN.md:** mark the completed phase row as ✅ Complete in the Implementation Status table
