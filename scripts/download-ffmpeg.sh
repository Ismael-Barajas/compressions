#!/usr/bin/env bash
set -euo pipefail

# Download static FFmpeg and ffprobe binaries for the current platform.
# Places them in src-tauri/binaries/ with Tauri target-triple naming.

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BIN_DIR="$SCRIPT_DIR/../src-tauri/binaries"
mkdir -p "$BIN_DIR"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      x86_64) TARGET="x86_64-apple-darwin" ;;
      arm64)  TARGET="aarch64-apple-darwin" ;;
      *)      echo "Unsupported macOS architecture: $ARCH"; exit 1 ;;
    esac
    FFMPEG_URL="https://evermeet.cx/ffmpeg/getrelease/zip"
    FFPROBE_URL="https://evermeet.cx/ffmpeg/getrelease/ffprobe/zip"

    echo "Downloading FFmpeg for macOS ($ARCH)..."
    curl -L "$FFMPEG_URL" -o /tmp/ffmpeg.zip
    unzip -o /tmp/ffmpeg.zip -d /tmp/ffmpeg_extract
    cp /tmp/ffmpeg_extract/ffmpeg "$BIN_DIR/ffmpeg-$TARGET"
    chmod +x "$BIN_DIR/ffmpeg-$TARGET"
    rm -rf /tmp/ffmpeg.zip /tmp/ffmpeg_extract

    echo "Downloading ffprobe for macOS ($ARCH)..."
    curl -L "$FFPROBE_URL" -o /tmp/ffprobe.zip
    unzip -o /tmp/ffprobe.zip -d /tmp/ffprobe_extract
    cp /tmp/ffprobe_extract/ffprobe "$BIN_DIR/ffprobe-$TARGET"
    chmod +x "$BIN_DIR/ffprobe-$TARGET"
    rm -rf /tmp/ffprobe.zip /tmp/ffprobe_extract
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
