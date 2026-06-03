#!/bin/bash
# Build the Dioxus frontend and copy to dist with path fixes for Tauri
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Support --debug for dev builds
if [[ "${1:-}" == "--debug" ]]; then
  PROFILE="debug"
  DX_FLAG=""
else
  PROFILE="release"
  DX_FLAG="--release"
fi

BUILD_DIR="$PROJECT_ROOT/target/dx/athena-frontend/$PROFILE/web/public"
DIST_DIR="$SCRIPT_DIR/dist"

# Save our custom index.html before building
CUSTOM_HTML="$SCRIPT_DIR/index.html"

echo "Building Dioxus frontend ($PROFILE)..."
cd "$SCRIPT_DIR"
~/.cargo/bin/dx build $DX_FLAG

echo "Copying build output to dist..."
rm -rf "$DIST_DIR"
cp -r "$BUILD_DIR" "$DIST_DIR"

# Tauri serves dist/ as static files, so vendored assets must be copied into it.
VENDOR_DIR="$SCRIPT_DIR/vendor"
if [ -d "$VENDOR_DIR" ]; then
  cp -r "$VENDOR_DIR" "$DIST_DIR/vendor"
  VENDOR_FILES=$(find "$DIST_DIR/vendor" -type f | wc -l | tr -d ' ')
  echo "Vendored assets copied: $VENDOR_FILES files in dist/vendor/"
fi

# Create symlinks for hashed filenames BEFORE checking entry path.
# Dioxus release builds output to assets/ with hashes; debug builds to wasm/ without.
for wasm in "$DIST_DIR"/assets/athena-frontend_bg-dx*.wasm; do
  [ -f "$wasm" ] && ln -sf "$(basename "$wasm")" "$DIST_DIR/assets/athena-frontend_bg.wasm"
done
for js in "$DIST_DIR"/assets/athena-frontend-dx*.js; do
  [ -f "$js" ] && ln -sf "$(basename "$js")" "$DIST_DIR/assets/athena-frontend.js"
done
for wasm in "$DIST_DIR"/wasm/athena-frontend_bg-dx*.wasm; do
  [ -f "$wasm" ] && ln -sf "$(basename "$wasm")" "$DIST_DIR/wasm/athena-frontend_bg.wasm"
done
for js in "$DIST_DIR"/wasm/athena-frontend-dx*.js; do
  [ -f "$js" ] && ln -sf "$(basename "$js")" "$DIST_DIR/wasm/athena-frontend.js"
done

# Replace Dioxus-generated index.html with our custom one (has WASM fixes + console capture)
if [ -f "$CUSTOM_HTML" ]; then
  cp "$CUSTOM_HTML" "$DIST_DIR/index.html"

  # Detect whether Dioxus output uses wasm/ or assets/ directory and set the entry path
  if [ -d "$DIST_DIR/wasm" ] && [ -f "$DIST_DIR/wasm/athena-frontend.js" ]; then
    ENTRY_PATH="./wasm/athena-frontend.js"
  elif [ -d "$DIST_DIR/assets" ] && [ -f "$DIST_DIR/assets/athena-frontend.js" ]; then
    ENTRY_PATH="./assets/athena-frontend.js"
  else
    echo "ERROR: Cannot find athena-frontend.js in wasm/ or assets/" >&2
    exit 1
  fi
  echo "Frontend entry point: $ENTRY_PATH"
  sed -i '' "s|__FRONTEND_ENTRY__|$ENTRY_PATH|g" "$DIST_DIR/index.html"
  echo "Replaced index.html with custom version (WASM fixes + console capture)"
fi

# Fix /./ paths that Dioxus generates — Tauri's custom protocol can't resolve
# absolute paths like /./wasm/... or /./assets/...
# These must become relative paths: ./wasm/... ./assets/...
perl -pi -e 's|href="/\./|href="./|g; s|src="/\./|src="./|g' "$DIST_DIR/index.html"

# Fix the same pattern inside any JS bundles
find "$DIST_DIR" -name '*.js' -exec perl -pi -e 's|"/\./|"./|g; s|/\./assets/|./assets/|g; s|/\./wasm/|./wasm/|g' {} +

echo "Done. Files in $DIST_DIR:"
find "$DIST_DIR" -type f -o -type l | sort
