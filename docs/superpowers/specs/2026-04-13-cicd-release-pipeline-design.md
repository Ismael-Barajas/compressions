# CI/CD Release Pipeline & Auto-Updater Design

## Context

Compressions is a desktop media compression app (Tauri v2 + React + Rust) that has completed all feature phases (1-6). It needs a CI/CD pipeline to build and distribute releases across Windows, macOS, and Linux. This is Phase 7 from PLAN.md.

The app currently has a CI workflow (`test.yml`) for tests/linting but no release automation, no code signing, and no auto-update capability. The goal is to ship version 1.0.0 as the first release.

## Decisions

- **Approach:** Official `tauri-apps/tauri-action@v0` for build + release
- **Platforms:** Windows (x86_64), macOS (ARM + x86_64), Linux (x86_64)
- **Trigger:** Git tag push (`v*`) + manual workflow dispatch
- **Code signing:** Deferred (no Apple Developer or Windows cert yet)
- **Auto-updater:** Yes, using `tauri-plugin-updater` with GitHub Releases
- **Update UX:** Toast notification on startup + manual check in header
- **Version:** Bump to 1.0.0 for first release
- **Update endpoint:** `https://github.com/Ismael-Barajas/compressions/releases/latest/download/latest.json`

## 1. Release Workflow (`.github/workflows/release.yml`)

### Trigger

```yaml
on:
  push:
    tags:
      - 'v*'
  workflow_dispatch:
```

Tag push auto-triggers. Manual dispatch builds from current main and creates a release using the version in `tauri.conf.json`.

**Test assumption:** The release workflow does not re-run tests. Tests run via `test.yml` on every push to main. You should only tag commits that have passed CI on main.

### Platform Matrix

| Runner | Target triple | Sidecar script | Extra deps |
|--------|--------------|----------------|------------|
| `windows-latest` | `x86_64-pc-windows-msvc` | `scripts/download-ffmpeg.ps1` + `scripts/download-gs.ps1` | NASM via choco |
| `macos-latest` | `aarch64-apple-darwin` | `scripts/download-ffmpeg.sh` | NASM via brew |
| `macos-latest` | `x86_64-apple-darwin` | `scripts/download-ffmpeg.sh` | NASM via brew |
| `ubuntu-22.04` | `x86_64-unknown-linux-gnu` | `scripts/download-ffmpeg.sh` + `scripts/download-gs.sh` | NASM, libwebkit2gtk-4.1-dev, libappindicator3-dev, librsvg2-dev, patchelf |

### Job Steps (per matrix entry)

1. `actions/checkout@v5`
2. `actions/setup-node@v5` (node 20, npm cache)
3. `dtolnay/rust-toolchain@stable` (with macOS cross-compile targets if needed)
4. `swatinem/rust-cache@v2` (workspaces: `src-tauri -> target`)
5. Install platform deps (NASM, Linux GTK/WebKit libs)
6. Download real sidecar binaries (FFmpeg + Ghostscript via existing scripts)
7. `npm ci`
8. `tauri-apps/tauri-action@v0` with:
   - `tagName: v__VERSION__`
   - `releaseName: Compressions v__VERSION__`
   - `releaseBody: See assets to download and install.`
   - `releaseDraft: true` (review before publishing)
   - `prerelease: false`
   - `uploadUpdaterJson: true`
   - `args: ${{ matrix.args }}` (target triple for macOS)
   - env: `GITHUB_TOKEN`, `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`

### Timeout

`timeout-minutes: 60` per job. Expected: 15-25 min cached, 30-45 min cold.

### Concurrency

```yaml
concurrency:
  group: release-${{ github.ref }}
  cancel-in-progress: false
```

No cancel-in-progress for releases — don't abort a build mid-flight.

### macOS Sidecar Considerations

- FFmpeg download script (`download-ffmpeg.sh`) detects macOS via `uname` and downloads universal binaries from evermeet.cx
- For x86_64 macOS target: the sidecar binary is universal, so same binary works for both architectures
- Ghostscript macOS: downloads universal `.pkg` from Richard Koch's build
- The download scripts place binaries with the correct target-triple suffix (`ffmpeg-aarch64-apple-darwin`, etc.)

### Sidecar download for cross-compilation

When building macOS x86_64 on an ARM runner (`macos-latest` is ARM), the download script detects the host arch. We may need to set `TARGET_TRIPLE` environment variable or rename binaries after download so Tauri finds the correct sidecar for the target. This needs to be verified during implementation — the existing scripts may need a small adjustment to accept a target triple parameter.

## 2. Updater Plugin Integration

### 2a. Dependencies

**Rust** (`src-tauri/Cargo.toml`):
```toml
[target.'cfg(not(any(target_os = "android", target_os = "ios")))'.dependencies]
tauri-plugin-updater = "2"
tauri-plugin-process = "2"
```

**JavaScript** (`package.json`):
```
@tauri-apps/plugin-updater
@tauri-apps/plugin-process
```

### 2b. Tauri Configuration (`src-tauri/tauri.conf.json`)

Add to `bundle`:
```json
"createUpdaterArtifacts": true
```

Add new top-level section:
```json
"plugins": {
  "updater": {
    "pubkey": "<GENERATED_PUBLIC_KEY>",
    "endpoints": [
      "https://github.com/Ismael-Barajas/compressions/releases/latest/download/latest.json"
    ],
    "windows": {
      "installMode": "passive"
    }
  }
}
```

### 2c. Plugin Registration (`src-tauri/src/lib.rs`)

Add after existing plugins in the builder chain:
```rust
.plugin(tauri_plugin_updater::Builder::new().build())
.plugin(tauri_plugin_process::init())
```

These are desktop-only but since the app is desktop-only (no mobile targets), no `#[cfg]` guard needed.

### 2d. Capabilities (`src-tauri/capabilities/default.json`)

Add to permissions array:
```json
"updater:default",
"process:allow-restart"
```

## 3. Frontend Update UI

### 3a. Update Hook (`src/hooks/useUpdateCheck.ts`)

A custom hook that:
- On mount (app startup), calls `check()` from `@tauri-apps/plugin-updater`
- Returns `{ updateAvailable, updateVersion, updateNotes, checking, downloading, downloadProgress, checkForUpdate, installUpdate }`
- `checkForUpdate()` — manual trigger for the header button
- `installUpdate()` — calls `downloadAndInstall()` then `relaunch()` from `@tauri-apps/plugin-process`
- Catches errors gracefully (network offline, etc.) — fails silently on startup, shows error on manual check

### 3b. Toast Notification (`src/components/update/UpdateToast.tsx`)

- Appears at the bottom-right of the app when an update is detected on startup
- Shows: "Compressions v{version} is available" + "Update" button + dismiss "X"
- Clicking "Update" triggers download with a progress indicator, then relaunches
- Auto-dismisses after ~15 seconds if not interacted with
- Uses existing app CSS variables for consistent theming

### 3c. Header Update Button (`src/components/layout/Header.tsx`)

- Add a small button in the header's right button group (next to theme/history/logs)
- Default state: shows current version as subtle text (e.g., "v1.0.0")
- When update available: icon changes to indicate update, accent-colored dot or highlight
- Click opens a small dropdown/popover: "Update available: v{version}" + "Install update" button
- Also supports "Check for updates" when no update is pending (manual check)

## 4. Version Bump to 1.0.0

Update version in all 3 files:
- `package.json`: `"version": "1.0.0"`
- `src-tauri/Cargo.toml`: `version = "1.0.0"`
- `src-tauri/tauri.conf.json`: `"version": "1.0.0"`

## 5. GitHub Secrets Setup (Manual — Before First Release)

### Generate signing keys (run locally once):
```bash
npm run tauri signer generate -- -w ~/.tauri/compressions.key
```

### Add to GitHub repo secrets (Settings > Secrets and variables > Actions):
| Secret name | Value |
|-------------|-------|
| `TAURI_SIGNING_PRIVATE_KEY` | Content of the generated private key file |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Password set during generation (or empty) |

## 6. First Release Checklist

1. Generate Tauri signing key pair locally
2. Add secrets to GitHub repo
3. Implement all code changes (workflow, updater plugin, UI, version bump)
4. Test locally: `npm run tauri build` succeeds
5. Push to main
6. Create and push tag: `git tag v1.0.0 && git push origin v1.0.0`
7. Watch the workflow run in GitHub Actions
8. Review the draft release — verify all platform artifacts are present
9. Publish the release
10. Verify `latest.json` is accessible at the endpoint URL

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/release.yml` | **New** — release build + upload workflow |
| `src-tauri/tauri.conf.json` | Add `createUpdaterArtifacts`, `plugins.updater` config |
| `src-tauri/Cargo.toml` | Add `tauri-plugin-updater` + `tauri-plugin-process` dependencies, bump version to 1.0.0 |
| `src-tauri/src/lib.rs` | Register updater + process plugins |
| `src-tauri/capabilities/default.json` | Add `updater:default`, `process:allow-restart` |
| `package.json` | Add `@tauri-apps/plugin-updater`, `@tauri-apps/plugin-process`, bump version |
| `src/hooks/useUpdateCheck.ts` | **New** — update check hook |
| `src/components/update/UpdateToast.tsx` | **New** — toast notification component |
| `src/components/layout/Header.tsx` | Add version/update button |

## Verification

1. **Rust compiles:** `cargo check --manifest-path src-tauri/Cargo.toml`
2. **TypeScript compiles:** `npx tsc --noEmit`
3. **Rust lint:** `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`
4. **Rust format:** `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`
5. **Local build:** `npm run tauri build` completes without error
6. **UI test:** Run `npm run tauri dev`, verify update check runs on startup (will show "no update" or fail gracefully since no release exists yet)
7. **Workflow test:** Push a tag, verify the workflow runs and creates a draft release with artifacts for all platforms
8. **Updater test:** After publishing the first release, build a v1.0.1-dev locally and verify it detects the v1.0.0 release as an "update" (or test with the current version and a manually created release)
