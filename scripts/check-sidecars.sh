#!/usr/bin/env bash
set -euo pipefail

# Fail if a downloaded sidecar is the wrong architecture for its target or FFmpeg
# lacks an encoder the app calls. Both have shipped before: the Apple Silicon build
# carried an x86_64 FFmpeg (every encode under Rosetta) without libsvtav1 (AV1
# failed on every Mac), and nothing caught it.
#
# Usage: check-sidecars.sh <target-triple>

TARGET="${1:?usage: check-sidecars.sh <target-triple>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../src-tauri/binaries"

# Encoders referenced in src-tauri/src (ffmpeg/args.rs, commands/image.rs).
ENCODERS=(libx264 libx265 libsvtav1 libaom-av1 libmp3lame libopus aac flac)
EXE=""
case "$TARGET" in
  aarch64-apple-darwin)
    WANT_ARCH="arm64"
    ENCODERS+=(h264_videotoolbox hevc_videotoolbox)
    ;;
  x86_64-apple-darwin)
    WANT_ARCH="x86_64"
    ENCODERS+=(h264_videotoolbox hevc_videotoolbox)
    ;;
  x86_64-unknown-linux-gnu)
    WANT_ARCH="x86-64"
    ;;
  x86_64-pc-windows-msvc)
    WANT_ARCH="x86-64"
    EXE=".exe"
    ;;
  *)
    echo "Unknown target: $TARGET"
    exit 1
    ;;
esac

failed=0

arch_of() {
  if command -v lipo >/dev/null 2>&1 && [[ "$TARGET" == *apple-darwin ]]; then
    lipo -archs "$1"
  elif command -v file >/dev/null 2>&1; then
    file -b "$1"
  else
    echo "unknown"
  fi
}

for bin in ffmpeg ffprobe gs; do
  path="$BIN_DIR/$bin-$TARGET$EXE"
  if [ ! -s "$path" ]; then
    echo "::error::missing sidecar $path"
    failed=1
    continue
  fi
  arch="$(arch_of "$path")"
  if [ "$arch" = "unknown" ]; then
    echo "note: cannot inspect the architecture of $bin here; skipping that check"
  elif ! grep -qw -- "$WANT_ARCH" <<<"$arch"; then
    echo "::error::$bin-$TARGET is built for '${arch:0:70}', expected $WANT_ARCH"
    failed=1
  else
    echo "ok: $bin-$TARGET is ${arch:0:70}"
  fi
done

ffmpeg="$BIN_DIR/ffmpeg-$TARGET$EXE"
if [ -s "$ffmpeg" ]; then
  # Ask the binary when this runner can execute it; otherwise read the encoder
  # names from the binary itself (e.g. an x86_64 build checked on an arm64 runner).
  if listing="$("$ffmpeg" -hide_banner -encoders 2>/dev/null)"; then
    source_desc="ffmpeg -encoders"
  else
    listing="$(strings -a "$ffmpeg")"
    source_desc="strings"
  fi
  for enc in "${ENCODERS[@]}"; do
    if grep -Eq "(^|[[:space:]])${enc}([[:space:]]|\$)" <<<"$listing"; then
      echo "ok: encoder $enc ($source_desc)"
    else
      echo "::error::ffmpeg-$TARGET has no $enc encoder"
      failed=1
    fi
  done
fi

exit "$failed"
