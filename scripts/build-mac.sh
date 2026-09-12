#!/usr/bin/env bash
set -euo pipefail

# build-mac.sh — Build a .dmg package for Maolan DAW on macOS.
#
# Usage:
#   ./scripts/build-mac.sh [OPTIONS]
#
# Options:
#   -s, --source-dir DIR     Path to maolan source directory (default: parent of this script)
#   -o, --output-dir DIR     Where to write the .dmg file (default: ./dist)
#   -v, --version VERSION    Override package version (default: read from Cargo.toml)
#   -t, --target-dir DIR     Local target directory (useful when source is on NFS)
#   -c, --codesign IDENTITY  Codesigning identity (default: ad-hoc "-")
#   -h, --help               Show this help message
#
# The script ensures the Xcode Command Line Tools are installed, installs Rust
# via rustup if missing, builds the release binaries, assembles a Maolan.app
# bundle, signs it, and produces a .dmg disk image in the output directory.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$SOURCE_DIR/dist"
OVERRIDE_VERSION=""
TARGET_DIR=""
CODESIGN_IDENTITY="-"

usage() {
    sed -n '4,18p' "$0" | sed 's/^# //'
    exit 0
}

while [[ $# -gt 0 ]]; do
    case "$1" in
        -s|--source-dir)
            SOURCE_DIR="$(realpath "$2")"
            shift 2
            ;;
        -o|--output-dir)
            OUTPUT_DIR="$(realpath "$2")"
            shift 2
            ;;
        -v|--version)
            OVERRIDE_VERSION="$2"
            shift 2
            ;;
        -t|--target-dir)
            TARGET_DIR="$(realpath "$2")"
            shift 2
            ;;
        -c|--codesign)
            CODESIGN_IDENTITY="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

if [[ "$(uname -s)" != "Darwin" ]]; then
    echo "Error: This script must run on macOS." >&2
    exit 1
fi

CARGO_TOML="$SOURCE_DIR/Cargo.toml"
if [[ ! -f "$CARGO_TOML" ]]; then
    echo "Error: Cargo.toml not found at $CARGO_TOML" >&2
    exit 1
fi

# Extract version from Cargo.toml or use override
if [[ -n "$OVERRIDE_VERSION" ]]; then
    PKG_VERSION="$OVERRIDE_VERSION"
else
    PKG_VERSION="$(grep -m1 '^version' "$CARGO_TOML" | sed 's/.*= *"\(.*\)".*/\1/')"
fi

MAC_ARCH="$(uname -m)"
PKG_NAME="maolan"
APP_NAME="Maolan.app"
DMG_NAME="${PKG_NAME}-${PKG_VERSION}-macos.${MAC_ARCH}.dmg"

echo "========================================"
echo "Building Maolan .dmg package"
echo "Version: $PKG_VERSION"
echo "Architecture: $MAC_ARCH"
echo "Source: $SOURCE_DIR"
echo "Output: $OUTPUT_DIR/$DMG_NAME"
echo "Codesign identity: $CODESIGN_IDENTITY"
echo "========================================"

# ---------------------------------------------------------------------------
# 1. Ensure Xcode Command Line Tools
# ---------------------------------------------------------------------------
echo ""
echo "[1/6] Checking Xcode Command Line Tools..."
if ! xcode-select -p &>/dev/null; then
    echo "Xcode Command Line Tools not found."
    echo "Running 'xcode-select --install' — complete the dialog, then re-run this script."
    xcode-select --install
    exit 1
fi
echo "Xcode Command Line Tools found: $(xcode-select -p)"

# ---------------------------------------------------------------------------
# 2. Install Rust if missing
# ---------------------------------------------------------------------------
echo ""
echo "[2/6] Checking Rust toolchain..."
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing via rustup..."
    export RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
    export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    source "$CARGO_HOME/env"
else
    echo "Rust already installed: $(rustc --version)"
fi

# Ensure cargo is in PATH for the rest of the script
if [[ -f "${CARGO_HOME:-$HOME/.cargo}/env" ]]; then
    source "${CARGO_HOME:-$HOME/.cargo}/env"
fi

# ---------------------------------------------------------------------------
# 3. Build release binaries
# ---------------------------------------------------------------------------
echo ""
echo "[3/6] Building release binaries..."
cd "$SOURCE_DIR"

CARGO_ARGS=("--release")
CARGO_ARGS+=("--workspace")
if [[ -n "$TARGET_DIR" ]]; then
    mkdir -p "$TARGET_DIR"
    CARGO_ARGS+=("--target-dir" "$TARGET_DIR")
    echo "Using local target directory: $TARGET_DIR"
fi

cargo build "${CARGO_ARGS[@]}"

# Determine where binaries ended up
if [[ -n "$TARGET_DIR" ]]; then
    BIN_DIR="$TARGET_DIR/release"
else
    BIN_DIR="$SOURCE_DIR/target/release"
fi

# Verify binaries exist
for bin in maolan maolan-cli maolan-osc maolan-plugin-host; do
    if [[ ! -f "$BIN_DIR/$bin" ]]; then
        echo "Error: Binary '$BIN_DIR/$bin' not found after build" >&2
        exit 1
    fi
done

echo "Build completed successfully."

# ---------------------------------------------------------------------------
# 4. Prepare Maolan.app bundle
# ---------------------------------------------------------------------------
echo ""
echo "[4/6] Preparing $APP_NAME bundle..."

STAGING_DIR="$(mktemp -d)"
trap "rm -rf '$STAGING_DIR'" EXIT

APP_DIR="$STAGING_DIR/$APP_NAME"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Binaries (the DAW locates maolan-plugin-host next to its own executable)
cp "$BIN_DIR/maolan"     "$APP_DIR/Contents/MacOS/"
cp "$BIN_DIR/maolan-cli" "$APP_DIR/Contents/MacOS/"
cp "$BIN_DIR/maolan-osc" "$APP_DIR/Contents/MacOS/"
cp "$BIN_DIR/maolan-plugin-host" "$APP_DIR/Contents/MacOS/"
chmod 755 "$APP_DIR/Contents/MacOS/"*

# Info.plist
cat > "$APP_DIR/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Maolan</string>
    <key>CFBundleDisplayName</key>
    <string>Maolan</string>
    <key>CFBundleExecutable</key>
    <string>maolan</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.maolan.maolan</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleVersion</key>
    <string>$PKG_VERSION</string>
    <key>CFBundleShortVersionString</key>
    <string>$PKG_VERSION</string>
    <key>CFBundleIconFile</key>
    <string>icon</string>
    <key>LSMinimumSystemVersion</key>
    <string>11.0</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.music</string>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSMicrophoneUsageDescription</key>
    <string>Maolan needs microphone access for audio recording.</string>
</dict>
</plist>
EOF

# Icon: build an .icns from assets/images/icon.png
ICON_SRC="$SOURCE_DIR/assets/images/icon.png"
ICONSET_DIR="$STAGING_DIR/icon.iconset"
mkdir -p "$ICONSET_DIR"
if [[ -f "$ICON_SRC" ]]; then
    sips -z 16 16     "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16.png"     >/dev/null
    sips -z 32 32     "$ICON_SRC" --out "$ICONSET_DIR/icon_16x16@2x.png"  >/dev/null
    sips -z 32 32     "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32.png"     >/dev/null
    sips -z 64 64     "$ICON_SRC" --out "$ICONSET_DIR/icon_32x32@2x.png"  >/dev/null
    sips -z 128 128   "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128.png"   >/dev/null
    sips -z 256 256   "$ICON_SRC" --out "$ICONSET_DIR/icon_128x128@2x.png" >/dev/null
    sips -z 256 256   "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256.png"   >/dev/null
    sips -z 512 512   "$ICON_SRC" --out "$ICONSET_DIR/icon_256x256@2x.png" >/dev/null
    sips -z 512 512   "$ICON_SRC" --out "$ICONSET_DIR/icon_512x512.png"   >/dev/null
    sips -z 1024 1024 "$ICON_SRC" --out "$ICONSET_DIR/icon_512x512@2x.png" >/dev/null
    iconutil -c icns "$ICONSET_DIR" -o "$APP_DIR/Contents/Resources/icon.icns"
else
    echo "Warning: $ICON_SRC not found; building without an icon" >&2
fi

# Strip local symbols from the binaries before signing
for bin in maolan maolan-cli maolan-osc maolan-plugin-host; do
    strip "$APP_DIR/Contents/MacOS/$bin"
done

# Documentation
cp "$SOURCE_DIR/README.md" "$APP_DIR/Contents/Resources/"
cp "$SOURCE_DIR/LICENSE"   "$APP_DIR/Contents/Resources/"

# ---------------------------------------------------------------------------
# 5. Sign the app bundle
# ---------------------------------------------------------------------------
echo ""
echo "[5/6] Signing $APP_NAME..."
for bin in maolan maolan-cli maolan-osc maolan-plugin-host; do
    codesign --force --sign "$CODESIGN_IDENTITY" "$APP_DIR/Contents/MacOS/$bin"
done
codesign --force --sign "$CODESIGN_IDENTITY" "$APP_DIR"

# ---------------------------------------------------------------------------
# 6. Build the .dmg
# ---------------------------------------------------------------------------
echo ""
echo "[6/6] Building .dmg..."

# DMG layout: the app bundle and a link to /Applications
DMG_STAGING="$STAGING_DIR/dmg-root"
mkdir -p "$DMG_STAGING"
cp -R "$APP_DIR" "$DMG_STAGING/"
ln -s /Applications "$DMG_STAGING/Applications"

mkdir -p "$OUTPUT_DIR"
DMG_PATH="$OUTPUT_DIR/$DMG_NAME"
rm -f "$DMG_PATH"

hdiutil create \
    -volname "Maolan" \
    -srcfolder "$DMG_STAGING" \
    -format UDZO \
    -imagekey zlib-level=9 \
    "$DMG_PATH"

# Verify the package
echo ""
echo "Verifying package..."
codesign --verify --deep --strict "$APP_DIR" 2>/dev/null || \
    echo "Warning: codesign verification reported issues (ad-hoc signed builds may show this)" >&2
hdiutil verify "$DMG_PATH"
ls -lh "$DMG_PATH"

echo ""
echo "========================================"
echo "Package built successfully:"
echo "  $DMG_PATH"
echo "========================================"
if [[ "$CODESIGN_IDENTITY" == "-" ]]; then
    echo ""
    echo "Note: The app is ad-hoc signed. For distribution outside this Mac,"
    echo "re-run with -c \"Developer ID Application: <name>\" and notarize the DMG."
fi
