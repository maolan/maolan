#!/usr/bin/env bash
set -euo pipefail

# build-fedora.sh — Build a .rpm package for Maolan DAW on Fedora.
#
# Usage:
#   ./scripts/build-fedora.sh [OPTIONS]
#
# Options:
#   -s, --source-dir DIR     Path to maolan source directory (default: parent of this script)
#   -o, --output-dir DIR     Where to write the .rpm file (default: ./dist)
#   -v, --version VERSION    Override package version (default: read from Cargo.toml)
#   -t, --target-dir DIR     Local target directory (useful when source is on NFS)
#   -h, --help               Show this help message
#
# The script installs build dependencies via dnf, installs Rust via rustup if missing,
# builds the release binaries, and produces a .rpm package using rpmbuild.

. /etc/os-release

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

RPM_ARCH="$(uname -m)"
PKG_NAME="maolan"
RPM_NAME="${PKG_NAME}-${PKG_VERSION}-1.fc${VERSION_ID}.${RPM_ARCH}.rpm"

echo "========================================"
echo "Building Maolan .rpm package"
echo "Version: $PKG_VERSION"
echo "Architecture: $RPM_ARCH"
echo "Source: $SOURCE_DIR"
echo "Output: $OUTPUT_DIR/$RPM_NAME"
echo "========================================"

# ---------------------------------------------------------------------------
# 1. Install system build dependencies
# ---------------------------------------------------------------------------
echo ""
echo "[1/6] Installing build dependencies..."
sudo dnf install -y \
    pkgconf-pkg-config \
    gcc \
    gcc-c++ \
    jack-audio-connection-kit-devel \
    alsa-lib-devel \
    lilv-devel \
    suil-devel \
    gtk2-devel \
    ffmpeg-free-devel \
    llvm-devel \
    clang-devel \
    cmake \
    protobuf-compiler \
    protobuf-devel \
    git \
    rpm-build \
    curl \
    ca-certificates

# ---------------------------------------------------------------------------
# 2. Ensure Rust is installed
# ---------------------------------------------------------------------------
echo ""
echo "[2/6] Checking Rust toolchain..."
if ! command -v cargo &>/dev/null; then
    echo "Rust not found. Installing from distribution packages..."
    sudo dnf install -y rust cargo
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
# 5. Prepare RPM package staging area
# ---------------------------------------------------------------------------
echo ""
echo "[5/6] Preparing RPM package structure..."

SPEC_DIR="$(mktemp -d)"
trap "rm -rf '$SPEC_DIR'" EXIT

mkdir -p "$SPEC_DIR"/{BUILD,RPMS,SOURCES,SPECS,SRPMS}

STAGING_DIR="$SPEC_DIR/staging"
mkdir -p "$STAGING_DIR/usr/bin"
mkdir -p "$STAGING_DIR/usr/share/applications"
mkdir -p "$STAGING_DIR/usr/share/icons/hicolor/scalable/apps"
mkdir -p "$STAGING_DIR/usr/share/doc/$PKG_NAME"

# Binaries
cp "$BIN_DIR/maolan"     "$STAGING_DIR/usr/bin/"
cp "$BIN_DIR/maolan-cli" "$STAGING_DIR/usr/bin/"
cp "$BIN_DIR/maolan-osc" "$STAGING_DIR/usr/bin/"
cp "$BIN_DIR/maolan-plugin-host" "$STAGING_DIR/usr/bin/"
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

# Create tarball for rpmbuild
cd "$STAGING_DIR"
tar czf "$SPEC_DIR/SOURCES/maolan-files.tar.gz" .

# Generate spec file
cat > "$SPEC_DIR/SPECS/maolan.spec" <<EOF
Name:           $PKG_NAME
Version:        $PKG_VERSION
Release:        1.fedora
Summary:        Rust Digital Audio Workstation
License:        BSD-2-Clause
URL:            https://github.com/maolan/maolan
Source0:        maolan-files.tar.gz
BuildArch:      $RPM_ARCH

Requires:       jack-audio-connection-kit, alsa-lib, lilv-libs, suil, gtk2, ffmpeg-free

%description
Maolan is a Rust DAW focused on recording, editing, routing,
automation, export, and plugin hosting.
It supports CLAP, VST3, and LV2 plugins on Unix.

%prep
# No source preparation needed for binary build

%build
# No build needed — binaries are already built

%install
mkdir -p %{buildroot}
cd %{buildroot}
tar xzf %{SOURCE0}

%files
%defattr(-,root,root,-)
/usr/bin/maolan
/usr/bin/maolan-cli
/usr/bin/maolan-osc
/usr/bin/maolan-plugin-host
/usr/share/applications/maolan.desktop
/usr/share/icons/hicolor/scalable/apps/maolan-icon.svg
%doc /usr/share/doc/maolan/README.md
%license /usr/share/doc/maolan/LICENSE

%changelog
* Sun May 10 2026 Maolan Team <meka@sys.it.com> - $PKG_VERSION-1
- Initial RPM package.
EOF

# ---------------------------------------------------------------------------
# 6. Build the .rpm package
# ---------------------------------------------------------------------------
echo ""
echo "[6/6] Building .rpm package..."
cd "$SPEC_DIR"
rpmbuild --define "_topdir $SPEC_DIR" --bb "$SPEC_DIR/SPECS/maolan.spec"

# ---------------------------------------------------------------------------
# 7. Copy result to output directory
# ---------------------------------------------------------------------------
mkdir -p "$OUTPUT_DIR"

# rpmbuild expands Release, so find the actual file name
BUILT_RPM="$(ls "$SPEC_DIR/RPMS/$RPM_ARCH/"*.rpm | head -n1)"
cp "$BUILT_RPM" "$OUTPUT_DIR/$RPM_NAME"

BUILT_RPM_BASENAME="$(basename "$BUILT_RPM")"

echo ""
echo "========================================"
echo "Package built successfully:"
echo "  $OUTPUT_DIR/$RPM_NAME"
echo "========================================"
