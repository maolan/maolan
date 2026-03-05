#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

TARGET_BIN="target/release/maolan"
if [[ ! -x "$TARGET_BIN" ]]; then
  echo "release binary not found: $TARGET_BIN"
  echo "run: cargo build --release"
  exit 1
fi

TARGET_TRIPLE="$(rustc -vV | awk '/host: / {print $2}')"
STAMP="$(date +%Y%m%d_%H%M%S)"
PKG_NAME="maolan-${STAMP}-${TARGET_TRIPLE}"
DIST_DIR="${ROOT_DIR}/dist/${PKG_NAME}"

mkdir -p "$DIST_DIR"
cp "$TARGET_BIN" "$DIST_DIR/"
cp README.md LICENSE TASKS_ROADMAP.md "$DIST_DIR/"

cat > "${DIST_DIR}/RELEASE_NOTES.txt" <<EOF
Maolan release bundle

Build target: ${TARGET_TRIPLE}
Build time: $(date -u +"%Y-%m-%d %H:%M:%S UTC")

Contents:
- maolan (release binary)
- README.md
- LICENSE
- TASKS_ROADMAP.md
EOF

TARBALL="${ROOT_DIR}/dist/${PKG_NAME}.tar.gz"
tar -C "${ROOT_DIR}/dist" -czf "$TARBALL" "$PKG_NAME"

echo "Created release artifact:"
echo "  $TARBALL"
