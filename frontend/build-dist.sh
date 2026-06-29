#!/bin/bash
# Build the Dioxus frontend and copy to dist with path fixes for Tauri
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Always build the frontend in release mode. Dioxus debug builds enable
# devtools that attempt a WebSocket connection for hot-reload, which panics
# in WKWebView (SecurityError). See E2E notes in CLAUDE.md.
PROFILE="release"
DX_FLAG="--release"

BUILD_DIR="$PROJECT_ROOT/target/dx/athena-frontend/$PROFILE/web/public"
DIST_DIR="$SCRIPT_DIR/dist"

# Save our custom index.html before building
CUSTOM_HTML="$SCRIPT_DIR/index.html"

echo "Building Dioxus frontend ($PROFILE)..."
cd "$SCRIPT_DIR"

# Clean stale hashed assets from the build cache BEFORE building so dx doesn't accumulate
# old WASM/JS from prior builds alongside the new ones.
if [ -d "$BUILD_DIR/assets" ]; then
  find "$BUILD_DIR/assets" -maxdepth 1 \( -name 'athena-frontend_bg-dx*.wasm' -o -name 'athena-frontend-dx*.js' \) -delete 2>/dev/null || true
fi

~/.cargo/bin/dx build $DX_FLAG

# Optimize WASM with wasm-opt if available.
# wasm-opt typically shaves 20-30% off release WASM size.
# Fallback: if wasm-opt is not installed, log a warning and continue.
wasm_opt_run() {
  local input_wasm="$1"
  if command -v wasm-opt &>/dev/null; then
    tmpfile=$(mktemp)
    if wasm-opt -Oz "$input_wasm" -o "$tmpfile" 2>/dev/null; then
      mv "$tmpfile" "$input_wasm"
      echo "wasm-opt: optimized $(basename "$input_wasm")"
    else
      rm -f "$tmpfile"
      echo "wasm-opt: optimization failed for $(basename "$input_wasm")"
    fi
  fi
}

echo "Copying build output to dist..."
rm -rf "$DIST_DIR"
cp -r "$BUILD_DIR" "$DIST_DIR"

# Optimize the main WASM file before creating stable aliases
for wasm in "$DIST_DIR"/assets/athena-frontend_bg-dx*.wasm; do
  [ -f "$wasm" ] && wasm_opt_run "$wasm"
done
for wasm in "$DIST_DIR"/wasm/athena-frontend_bg-dx*.wasm; do
  [ -f "$wasm" ] && wasm_opt_run "$wasm"
done

# Tauri serves dist/ as static files, so vendored assets must be copied into it.
VENDOR_DIR="$SCRIPT_DIR/vendor"

if [ -d "$VENDOR_DIR" ]; then
  cp -r "$VENDOR_DIR" "$DIST_DIR/vendor"
  VENDOR_FILES=$(find "$DIST_DIR/vendor" -type f | wc -l | tr -d ' ')
  echo "Vendored assets copied: $VENDOR_FILES files in dist/vendor/"
fi

# Create stable-name aliases for hashed filenames BEFORE checking entry path.
# Dioxus release builds output to assets/ with hashes; debug builds to wasm/ without.
# Use hard copies (not symlinks) so Tauri's asset embedder picks them up for release bundles.
for wasm in "$DIST_DIR"/assets/athena-frontend_bg-dx*.wasm; do
  [ -f "$wasm" ] && cp -f "$wasm" "$DIST_DIR/assets/athena-frontend_bg.wasm"
done
for js in "$DIST_DIR"/assets/athena-frontend-dx*.js; do
  [ -f "$js" ] && cp -f "$js" "$DIST_DIR/assets/athena-frontend.js"
done
for wasm in "$DIST_DIR"/wasm/athena-frontend_bg-dx*.wasm; do
  [ -f "$wasm" ] && cp -f "$wasm" "$DIST_DIR/wasm/athena-frontend_bg.wasm"
done
for js in "$DIST_DIR"/wasm/athena-frontend-dx*.js; do
  [ -f "$js" ] && cp -f "$js" "$DIST_DIR/wasm/athena-frontend.js"
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
