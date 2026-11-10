#!/usr/bin/env bash
# macOS build script (aarch64 + x86_64), counterpart of build.sh (Linux).
#
# Dependencies (macOS example):
#   xcode-select --install
#   brew install node pnpm rustup openssl
# Also requires Node.js 20.19+, pnpm, and Rust 1.88+.
#
# Usage: chmod +x build-macos.sh && ./build-macos.sh

set -euo pipefail

cd "$(dirname "$0")"

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

TARGETS=("aarch64-apple-darwin" "x86_64-apple-darwin")

for TARGET in "${TARGETS[@]}"; do
  echo "[build] target : $TARGET"

  # Auto-install missing Rust targets
  if ! rustup target list --installed | grep -q "^${TARGET}$"; then
    echo "[build] installing target: $TARGET"
    rustup target add "$TARGET"
  fi

  pnpm tauri build --bundles dmg,app --target "$TARGET"

  OUT="release/$VERSION/$TARGET"
  mkdir -p "$OUT"

  BINARY_PATH="src-tauri/target/$TARGET/release/filezilla-rs"
  if [ -f "$BINARY_PATH" ]; then
    cp "$BINARY_PATH" "$OUT/"
  else
    echo "[WARN] Main binary not found at $BINARY_PATH, skipping binary copy." >&2
  fi

  copied=0

  # Only copy bundles matching the current version: the bundler does not
  # clean stale bundles from previous builds, so a bare *.dmg glob would
  # copy them too.
  for file in src-tauri/target/$TARGET/release/bundle/dmg/*_${VERSION}_*.dmg; do
    [ -e "$file" ] || continue
    cp "$file" "$OUT/"
    copied=$((copied + 1))
  done

  for file in src-tauri/target/$TARGET/release/bundle/macos/*.app; do
    [ -e "$file" ] || continue
    cp -R "$file" "$OUT/"
    copied=$((copied + 1))
  done

  if [ "$copied" -eq 0 ]; then
    echo "[ERROR] No .dmg or .app bundle for version $VERSION was produced for $TARGET." >&2
    exit 1
  fi

  echo "[build] $TARGET succeeded:"
  [ -f "$OUT/filezilla-rs" ] && echo "  $OUT/filezilla-rs"
  for file in "$OUT"/*.dmg; do
    [ -e "$file" ] || continue
    echo "  $file"
  done
  for file in "$OUT"/*.app; do
    [ -e "$file" ] || continue
    echo "  $file"
  done
done

echo "Build and copy succeeded for all targets!"