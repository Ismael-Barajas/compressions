# Releasing

How a version goes from a PR to users, and how to check the tag before trusting it.

## Steps

1. **Bump the version on the release PR's branch**, not on `main`:
   ```
   npm run version:patch      # or version:minor / version:major
   ```
   This updates `package.json`, `package-lock.json`, `src-tauri/tauri.conf.json`,
   `src-tauri/Cargo.toml` and `src-tauri/Cargo.lock`.

2. **Rename the CHANGELOG section** from `## [Unreleased]` to `## [X.Y.Z] — YYYY-MM-DD`.
   The Release workflow takes its notes from the section whose heading matches the version.
   If the heading still says `Unreleased`, the notes step fails. Check both:
   ```
   npm run version:check
   node scripts/release-notes.mjs    # must print the new section
   ```

3. **Merge the PR once CI is green on its head.** Note the head SHA; you need it in step 5.
   PRs are squash-merged, so the merge commit on `main` has a different SHA from the PR head
   but should have the same tree.

4. **Tag the merge commit on `main`**, never the PR branch, with the change notes in the tag
   message. Run this from an up-to-date `main` checkout, because the notes script reads
   `CHANGELOG.md` and `package.json` from disk:
   ```
   git checkout main && git pull
   git tag -a --cleanup=whitespace vX.Y.Z -m "Compressions vX.Y.Z" -m "$(node scripts/release-notes.mjs)"
   git push origin vX.Y.Z
   ```
   Pushing a `v*` tag starts `.github/workflows/release.yml`. The workflow can also be started
   by hand from `main` (Actions → Release → Run workflow). Push the tag **once**: re-pushing
   or moving a `v*` tag starts the Release workflow again.

   The GitHub release body gets the same notes independently: the workflow's
   "Read release notes" step runs `scripts/release-notes.mjs` and fails if the section is
   missing or empty.

5. **Check that the tag includes the changes** before publishing:
   ```
   git fetch origin --tags --force
   git rev-parse vX.Y.Z^{commit}          # should equal the merge commit on main
   git rev-parse origin/main
   git diff --stat <PR head SHA> vX.Y.Z   # empty = tag tree is exactly what CI tested
   git show vX.Y.Z:package.json | grep '"version"'
   git show vX.Y.Z:CHANGELOG.md | grep -m1 '^## \['
   git tag -l --format='%(contents)' vX.Y.Z   # title plus the change notes
   ```

6. **Publish.** The workflow builds macOS (arm64 and x86_64), Windows and Linux. Before
   building, it runs `scripts/check-sidecars.sh`, which fails the release if FFmpeg,
   ffprobe or Ghostscript has the wrong architecture or FFmpeg lacks an encoder the app
   uses. The result is a **draft** release. Nothing reaches users until the draft is
   published on the Releases page; after that, the in-app updater offers the new version.

## Worked example: v1.2.1

| Check | Result |
|-------|--------|
| PR | #14, CI green on head `ccefee0` |
| Merge commit | `e44254e` on `main` (squash) |
| Tag | annotated `v1.2.1` → `e44254e`; its message is only the title (created before step 4 included the notes) |
| Release notes | `release-notes.mjs` on the tagged tree prints the full `[1.2.1]` section; the "Read release notes" step passed on all four builds |
| `git diff --stat ccefee0 v1.2.1` | empty: identical to the tested tree |
| Versions at tag | 1.2.1 in all manifests; CHANGELOG `## [1.2.1] — 2026-09-24` |
| Release run | #9, triggered by the tag push |

## Claude Code sessions

A Claude Code web session can push only its own working branch. It **cannot push tags**
(the push fails with HTTP 403) and **cannot start workflows** (the dispatch API returns
"Resource not accessible by integration"). Steps 1–3 and 5 can be done from a session;
step 4 (pushing the tag) and publishing the draft are done by a maintainer. The session
should hand over the exact tag command and then run the step 5 checks once the tag exists.
