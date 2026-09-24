# Compressions

Tauri v2 desktop app: React/TypeScript frontend in `src/`, Rust backend in `src-tauri/`,
with FFmpeg, ffprobe and Ghostscript bundled as sidecars.

## Checks before pushing

- Frontend: `npx tsc --noEmit`, `npm run test:run`
- Rust (from `src-tauri/`): `cargo fmt --check`, `cargo clippy --lib -- -D warnings`, `cargo test --lib`

## Commits and PRs

Do not add `Co-Authored-By` or session-link lines to commit messages, PR descriptions or merge commits.

## Releasing

Follow `docs/RELEASING.md`. From a Claude Code session you cannot push tags or start
workflows. After the release PR merges, give the maintainer the tag command from that
doc. Once the tag exists, check that it points at the merge commit on `main` and that
its tree matches the PR head CI tested.
