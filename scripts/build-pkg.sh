#!/usr/bin/env bash
set -euo pipefail

# build-pkg.sh — Build a .pkg.tar.zst package for Maolan DAW on Arch/Manjaro.
#
# Usage:
#   ./scripts/build-pkg.sh [OPTIONS]
#
# Options:
#   -s, --source-dir DIR     Path to maolan source directory (default: parent of this script)
#   -o, --output-dir DIR     Where to write the package (default: ./dist)
#   -v, --version VERSION    Override package version (default: read from Cargo.toml)
#   -t, --target-dir DIR     Local target directory (useful when source is on NFS)
#   -h, --help               Show this help message
#
# The script installs build dependencies via pacman, ensures Rust is installed,
# builds the release binaries, and produces a .pkg.tar.zst package.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SOURCE_DIR="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="$SOURCE_DIR/dist"
OVERRIDE_VERSION=""
TARGET_DIR=""

usage() {
    sed -n '2,14p' "$0" | sed 's/^# //'
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
        -h|--help)
            usage
            ;;
        *)
            echo "Unknown option: $1" >&2
            exit 1
            ;;
    esac
done

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

ARCH="$(uname -m)"
PKG_NAME="maolan"
PKG_FILE="${PKG_NAME}-${PKG_VERSION}-1-${ARCH}.pkg.tar.zst"

echo "========================================"
echo "Building Maolan .pkg.tar.zst package"
echo "Version: $PKG_VERSION"
echo "Architecture: $ARCH"
echo "Source: $SOURCE_DIR"
echo "Output: $OUTPUT_DIR/$PKG_FILE"
echo "========================================"

# ---------------------------------------------------------------------------
# 1. Install system build dependencies
# ---------------------------------------------------------------------------
echo ""
echo "[1/6] Installing build dependencies..."
sudo pacman -S --needed --noconfirm \
    base-devel \
    pkgconf \
    jack2 \
    alsa-lib \
    lilv \
    suil \
    gtk2 \
    rubberband \
    ffmpeg \
    llvm \
    clang \
    cmake \
    protobuf \
    git \
    rust \
    curl \
    ca-certificates \
    libarchive \
    zstd

# ---------------------------------------------------------------------------
# 2. Ensure Rust is installed
# ---------------------------------------------------------------------------
echo ""
echo "[2/6] Checking Rust toolchain..."
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing from distribution packages..."
    sudo pacman -S --needed --noconfirm rust cargo
else
    echo "Rust already installed: $(rustc --version)"
fi

# ---------------------------------------------------------------------------
# 3. Set LIBCLANG_PATH if needed
# ---------------------------------------------------------------------------
echo ""
echo "[3/6] Configuring build environment..."
if command -v llvm-config &>/dev/null; then
    export LIBCLANG_PATH="$(llvm-config --libdir)"
    echo "LIBCLANG_PATH set to: $LIBCLANG_PATH"
fi

# ---------------------------------------------------------------------------
# 4. Build release binaries
# ---------------------------------------------------------------------------
echo ""
echo "[4/6] Building release binaries..."
cd "$SOURCE_DIR"

CARGO_ARGS=("--release")
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
for bin in maolan maolan-cli maolan-osc; do
    if [[ ! -f "$BIN_DIR/$bin" ]]; then
        echo "Error: Binary '$BIN_DIR/$bin' not found after build" >&2
        exit 1
    fi
done

echo "Build completed successfully."

# ---------------------------------------------------------------------------
# 5. Prepare package staging area
# ---------------------------------------------------------------------------
echo ""
echo "[5/6] Preparing package structure..."

STAGING_DIR="$(mktemp -d)"
trap "rm -rf '$STAGING_DIR'" EXIT

mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$STAGING_DIR/usr/share/doc/$PKG_NAME"

# Binaries
cp "$BIN_DIR/maolan"     "$STAGING_DIR/usr/bin/"
cp "$BIN_DIR/maolan-cli" "$STAGING_DIR/usr/bin/"
cp "$BIN_DIR/maolan-osc" "$STAGING_DIR/usr/bin/"
strip "$STAGING_DIR/usr/bin/"*
chmod 755 "$STAGING_DIR/usr/bin/"*

# Desktop entry
cp "$SOURCE_DIR/desktop/maolan-linux.desktop" "$STAGING_DIR/usr/share/applications/maolan.desktop"
chmod 644 "$STAGING_DIR/usr/share/applications/maolan.desktop"

# Icon
cp "$SOURCE_DIR/images/maolan-icon.svg" "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-icon.svg"
chmod 644 "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps/maolan-icon.svg"

# Documentation
cp "$SOURCE_DIR/README.md" "$STAGING_DIR/usr/share/doc/$PKG_NAME/"
cp "$SOURCE_DIR/LICENSE"   "$STAGING_DIR/usr/share/doc/$PKG_NAME/"

# Calculate installed size
SIZE="$(du -sb "$STAGING_DIR" | cut -f1)"
BUILDDATE="$(date +%s)"

# Generate .PKGINFO
cat > "$STAGING_DIR/.PKGINFO" <<EOF
pkgname = $PKG_NAME
pkgbase = $PKG_NAME
pkgver = $PKG_VERSION-1
pkgdesc = Rust Digital Audio Workstation
url = https://github.com/maolan/maolan
builddate = $BUILDDATE
packager = Maolan Team <maolan@github.io>
size = $SIZE
arch = $ARCH
license = BSD-2-Clause
depend = jack2
depend = alsa-lib
depend = lilv
depend = suil
depend = gtk2
depend = rubberband
depend = ffmpeg
EOF

# ---------------------------------------------------------------------------
# 6. Build the .pkg.tar.zst package
# ---------------------------------------------------------------------------
echo ""
echo "[6/6] Building .pkg.tar.zst package..."
mkdir -p "$OUTPUT_DIR"

if command -v bsdtar &>/dev/null; then
    bsdtar -cf "$OUTPUT_DIR/$PKG_FILE" -C "$STAGING_DIR" --options zstd:compression-level=19 .
else
    tar --zstd -cf "$OUTPUT_DIR/$PKG_FILE" -C "$STAGING_DIR" .
fi

echo ""
echo "========================================"
echo "Package built successfully:"
echo "  $OUTPUT_DIR/$PKG_FILE"
echo "========================================"
