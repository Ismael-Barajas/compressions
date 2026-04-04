#!/usr/bin/env bash
set -euo pipefail

# Download Ghostscript binary for the current platform.
# Places it in src-tauri/binaries/ with Tauri target-triple naming.
#
# macOS: Extracts universal binary from Richard Koch's .pkg
# Linux/Windows (via WSL/MSYS): Downloads Windows installer and extracts gswin64c.exe + gsdll64.dll

GS_VERSION="10.07.0"
GS_VERSION_NODOTS="10070"

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

    PKG_URL="https://pages.uoregon.edu/koch/Ghostscript-${GS_VERSION}.pkg"
    TEMP_DIR=$(mktemp -d)

    echo "Downloading Ghostscript ${GS_VERSION} for macOS..."
    curl -L "$PKG_URL" -o "$TEMP_DIR/gs.pkg"

    echo "Extracting .pkg..."
    pkgutil --expand "$TEMP_DIR/gs.pkg" "$TEMP_DIR/gs_expanded"

    # The .pkg contains a component package with a Payload (cpio archive)
    PAYLOAD=$(find "$TEMP_DIR/gs_expanded" -name "Payload" | head -1)
    if [ -z "$PAYLOAD" ]; then
      echo "Error: Could not find Payload in .pkg"
      rm -rf "$TEMP_DIR"
      exit 1
    fi

    echo "Extracting Payload..."
    mkdir -p "$TEMP_DIR/gs_payload"
    cd "$TEMP_DIR/gs_payload"
    cat "$PAYLOAD" | gunzip -c | cpio -id 2>/dev/null || true
    cd - > /dev/null

    # Find the gs binary — Koch's .pkg ships gs-noX11 (headless build)
    GS_BIN=$(find "$TEMP_DIR/gs_payload" -name "gs-noX11" -type f | head -1)
    if [ -z "$GS_BIN" ]; then
      # Fallback: try plain "gs"
      GS_BIN=$(find "$TEMP_DIR/gs_payload" -name "gs" -type f | head -1)
    fi
    if [ -z "$GS_BIN" ]; then
      echo "Error: Could not find gs binary in extracted payload"
      echo "Contents of bin/:"
      find "$TEMP_DIR/gs_payload" -path "*/bin/*" -type f
      rm -rf "$TEMP_DIR"
      exit 1
    fi

    echo "Found gs binary at: $GS_BIN"
    cp "$GS_BIN" "$BIN_DIR/gs-$TARGET"
    chmod +x "$BIN_DIR/gs-$TARGET"

    # Copy Ghostscript resource files (fonts, init scripts, ICC profiles)
    # These are required at runtime — gs_init.ps is in Resource/Init/
    GS_SHARE_DIR=$(find "$TEMP_DIR/gs_payload" -type d -name "$GS_VERSION" -path "*/share/ghostscript/*" | head -1)
    GS_RES_DIR="$BIN_DIR/gs-res"
    if [ -n "$GS_SHARE_DIR" ]; then
      rm -rf "$GS_RES_DIR"
      mkdir -p "$GS_RES_DIR"
      cp -R "$GS_SHARE_DIR/Resource" "$GS_RES_DIR/"
      cp -R "$GS_SHARE_DIR/lib" "$GS_RES_DIR/"
      cp -R "$GS_SHARE_DIR/iccprofiles" "$GS_RES_DIR/"
      echo "Ghostscript resources installed: $GS_RES_DIR ($(du -sh "$GS_RES_DIR" | cut -f1))"
    else
      echo "Warning: Could not find Ghostscript share directory"
    fi

    rm -rf "$TEMP_DIR"
    echo "Ghostscript binary installed: $BIN_DIR/gs-$TARGET"
    ;;

  Linux|MINGW*|MSYS*|CYGWIN*)
    TARGET="x86_64-pc-windows-msvc"
    INSTALLER_URL="https://github.com/ArtifexSoftware/ghostpdl-downloads/releases/download/gs${GS_VERSION_NODOTS}/gs${GS_VERSION_NODOTS}w64.exe"
    TEMP_DIR=$(mktemp -d)

    echo "Downloading Ghostscript ${GS_VERSION} Windows installer..."
    curl -L "$INSTALLER_URL" -o "$TEMP_DIR/gs_installer.exe"

    # NSIS installers can be extracted with 7z
    if ! command -v 7z &> /dev/null; then
      echo "Error: 7z is required to extract the Windows installer."
      echo "Install with: sudo apt install p7zip-full (Linux) or choco install 7zip (Windows)"
      rm -rf "$TEMP_DIR"
      exit 1
    fi

    echo "Extracting installer with 7z..."
    7z x -o"$TEMP_DIR/gs_extracted" "$TEMP_DIR/gs_installer.exe" > /dev/null

    # Find gswin64c.exe and gsdll64.dll
    GS_EXE=$(find "$TEMP_DIR/gs_extracted" -name "gswin64c.exe" | head -1)
    GS_DLL=$(find "$TEMP_DIR/gs_extracted" -name "gsdll64.dll" | head -1)

    if [ -z "$GS_EXE" ]; then
      echo "Error: Could not find gswin64c.exe in extracted installer"
      rm -rf "$TEMP_DIR"
      exit 1
    fi

    cp "$GS_EXE" "$BIN_DIR/gs-$TARGET.exe"
    if [ -n "$GS_DLL" ]; then
      cp "$GS_DLL" "$BIN_DIR/gsdll64.dll"
      echo "Copied gsdll64.dll (required alongside gs.exe)"
    fi

    # Copy Ghostscript resource files
    GS_SHARE_DIR=$(find "$TEMP_DIR/gs_extracted" -type d -name "Resource" -path "*/gs*" | head -1)
    GS_RES_DIR="$BIN_DIR/gs-res"
    if [ -n "$GS_SHARE_DIR" ]; then
      GS_PARENT=$(dirname "$GS_SHARE_DIR")
      rm -rf "$GS_RES_DIR"
      mkdir -p "$GS_RES_DIR"
      cp -R "$GS_PARENT/Resource" "$GS_RES_DIR/"
      [ -d "$GS_PARENT/lib" ] && cp -R "$GS_PARENT/lib" "$GS_RES_DIR/"
      [ -d "$GS_PARENT/iccprofiles" ] && cp -R "$GS_PARENT/iccprofiles" "$GS_RES_DIR/"
      echo "Ghostscript resources installed: $GS_RES_DIR"
    fi

    rm -rf "$TEMP_DIR"
    echo "Ghostscript binaries installed for Windows."
    ;;

  *)
    echo "Unsupported OS: $OS"
    exit 1
    ;;
esac

echo ""
ls -la "$BIN_DIR"/gs*
