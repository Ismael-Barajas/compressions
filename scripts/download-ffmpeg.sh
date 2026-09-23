#!/usr/bin/env bash
set -euo pipefail

# Download static FFmpeg and ffprobe binaries.
# Places them in src-tauri/binaries/ with Tauri target-triple naming.
#
# Usage: download-ffmpeg.sh [target-triple]
#   macOS: the target defaults to the host; pass aarch64-apple-darwin or
#          x86_64-apple-darwin to fetch the other architecture (CI builds both on
#          one runner). Each target gets a native build: running an x86_64 FFmpeg on
#          Apple Silicon puts every encode through Rosetta.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../src-tauri/binaries"
mkdir -p "$BIN_DIR"

OS="$(uname -s)"
ARCH="$(uname -m)"
TARGET="${1:-}"

case "$OS" in
  Darwin)
    if [ -z "$TARGET" ]; then
      case "$ARCH" in
        x86_64) TARGET="x86_64-apple-darwin" ;;
        arm64)  TARGET="aarch64-apple-darwin" ;;
        *)      echo "Unsupported macOS architecture: $ARCH"; exit 1 ;;
      esac
    fi
    # Martin Riedl's build server publishes static macOS builds per architecture,
    # including SVT-AV1 (the evermeet.cx builds used before were x86_64-only and
    # had no libsvtav1, so AV1 failed on every Mac).
    case "$TARGET" in
      aarch64-apple-darwin) BUILD_ARCH="arm64" ;;
      x86_64-apple-darwin)  BUILD_ARCH="amd64" ;;
      *) echo "Unsupported macOS target: $TARGET"; exit 1 ;;
    esac
    BASE_URL="https://ffmpeg.martin-riedl.de/redirect/latest/macos/$BUILD_ARCH/release"

    TEMP_DIR=$(mktemp -d)
    for bin in ffmpeg ffprobe; do
      echo "Downloading $bin for $TARGET..."
      curl -fL "$BASE_URL/$bin.zip" -o "$TEMP_DIR/$bin.zip"
      unzip -o -q "$TEMP_DIR/$bin.zip" -d "$TEMP_DIR/$bin"
      SRC=$(find "$TEMP_DIR/$bin" -type f -name "$bin" | head -1)
      if [ -z "$SRC" ]; then
        echo "Error: $bin not found in the downloaded archive"
        rm -rf "$TEMP_DIR"
        exit 1
      fi
      cp "$SRC" "$BIN_DIR/$bin-$TARGET"
      chmod +x "$BIN_DIR/$bin-$TARGET"
    done
    rm -rf "$TEMP_DIR"
    ;;

  Linux|MINGW*|MSYS*|CYGWIN*)
    TARGET="x86_64-pc-windows-msvc"
    FFMPEG_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip"

    echo "Downloading FFmpeg for Windows..."
    curl -L "$FFMPEG_URL" -o /tmp/ffmpeg-win.zip
    unzip -o /tmp/ffmpeg-win.zip -d /tmp/ffmpeg_win_extract

    EXTRACT_DIR=$(find /tmp/ffmpeg_win_extract -maxdepth 1 -type d -name "ffmpeg-*" | head -1)
    cp "$EXTRACT_DIR/bin/ffmpeg.exe" "$BIN_DIR/ffmpeg-$TARGET.exe"
    cp "$EXTRACT_DIR/bin/ffprobe.exe" "$BIN_DIR/ffprobe-$TARGET.exe"
    rm -rf /tmp/ffmpeg-win.zip /tmp/ffmpeg_win_extract
    ;;

  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

echo "FFmpeg binaries installed to $BIN_DIR"
ls -la "$BIN_DIR"
