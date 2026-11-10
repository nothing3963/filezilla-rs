#!/usr/bin/env bash
# Linux build script, the counterpart of build.cmd in the project root.
#
# Dependencies (Debian/Ubuntu example):
#   sudo apt install build-essential libssl-dev \
#     libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev patchelf
# Also requires Node.js 20.19+, pnpm, and Rust 1.88+.
#
# Usage: chmod +x build.sh && ./build.sh

set -euo pipefail

cd "$(dirname "$0")"

# Same as build.cmd: keep the temp directory and cargo cache inside the project to avoid polluting system directories.
export TMPDIR="$PWD/.appdata/tmp"
export TMP="$TMPDIR"
export TEMP="$TMPDIR"
mkdir -p "$TMPDIR"
export CARGO_HOME="$PWD/.cache/cargo-home"

VERSION="$(node scripts/get-version.mjs)"

echo "[build] project : $PWD"
echo "[build] TMP     : $TMPDIR"
echo "[build] CARGO_HOME : $CARGO_HOME"
echo "[build] version : $VERSION"

pnpm install --reporter=append-only --config.confirmModulesPurge=false
pnpm build
pnpm tauri build --bundles deb,appimage

OUT="release/$VERSION"
mkdir -p "$OUT"

cp "src-tauri/target/release/filezilla-rs" "$OUT/"

copied=0
# Only copy bundles matching the current version: the bundler does not clean
# stale bundles from previous builds, so a bare *.deb glob would copy them too.
for file in src-tauri/target/release/bundle/deb/*_${VERSION}_*.deb; do
  [ -e "$file" ] || continue
  cp "$file" "$OUT/"
  copied=$((copied + 1))
done
for file in src-tauri/target/release/bundle/appimage/*_${VERSION}_*.AppImage; do
  [ -e "$file" ] || continue
  cp "$file" "$OUT/"
  copied=$((copied + 1))
done

# Require at least one install package for this exact version; anything else
# means the bundler did not produce the artifact we just asked for.
if [ "$copied" -eq 0 ]; then
  echo "[ERROR] No .deb or .AppImage bundle for version $VERSION was produced." >&2
  exit 1
fi

echo "Build and copy succeeded!"
echo "  $OUT/filezilla-rs"
for file in "$OUT"/*.deb "$OUT"/*.AppImage; do
  [ -e "$file" ] || continue
  echo "  $file"
done
