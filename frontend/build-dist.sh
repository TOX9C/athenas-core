#!/bin/bash
# Build the Dioxus frontend and copy to dist with path fixes for Tauri
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Always build the frontend in release mode. Dioxus debug builds enable
# devtools that attempt a WebSocket connection for hot-reload, which panics
# in WKWebView (SecurityError). See the release E2E configuration for details.
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

# Prefer the cargo bin dx: CI installs dioxus-cli there too (ci.yml), and a
# Homebrew binary also named `dx` (unrelated tool) can shadow it in PATH.
DX_BIN="$HOME/.cargo/bin/dx"
if [ ! -x "$DX_BIN" ]; then
  DX_BIN="$(command -v dx || true)"
fi
if [ -z "$DX_BIN" ] || [ ! -x "$DX_BIN" ]; then
  echo "error: dioxus-cli (`dx`) not found; install with: cargo install dioxus-cli" >&2
  exit 1
fi
"$DX_BIN" build $DX_FLAG

# wasm-opt is handled by dx itself: [web.wasm_opt] level "z" in Dioxus.toml
# runs its own downloaded (current) binaryen during `dx build --release`.
# The former manual pass here was removed — it depended on whatever stale
# system binaryen was in PATH (Ubuntu apt ships v116, which cannot parse
# wasm produced by the current rustc toolchain) and only saved ~0.4% on
# top of dx's own optimization.

rm -rf "$DIST_DIR"
cp -r "$BUILD_DIR" "$DIST_DIR"
# dx copies public/ verbatim into the build output; drop the legacy art
# archive before anything else looks at dist (perf#5, see comment below).
rm -rf "$DIST_DIR/art"

# (wasm optimization loop removed — dx handles it; see comment above)

# Tauri serves dist/ as static files, so vendored assets must be copied into it.
VENDOR_DIR="$SCRIPT_DIR/vendor"

if [ -d "$VENDOR_DIR" ]; then
  cp -r "$VENDOR_DIR" "$DIST_DIR/vendor"
  VENDOR_FILES=$(find "$DIST_DIR/vendor" -type f | wc -l | tr -d ' ')
  echo "Vendored assets copied: $VENDOR_FILES files in dist/vendor/"
fi

# Copy mobile PWA shell assets into dist alongside the Dioxus bundle.
cp -f "$SCRIPT_DIR/public/mobile.css" "$DIST_DIR/mobile.css"
cp -f "$SCRIPT_DIR/public/manifest.webmanifest" "$DIST_DIR/manifest.webmanifest"
cp -f "$SCRIPT_DIR/public/sw.js" "$DIST_DIR/sw.js"
rm -rf "$DIST_DIR/icons"
cp -r "$SCRIPT_DIR/public/icons" "$DIST_DIR/icons"

# Stable entry-script names: the custom index.html/mobile.html reference
# `athena-frontend.js` (unhashed), and the wasm-bindgen bootstrap inside that
# script fetches the *hashed* `-dx*.wasm` directly. So the JS entry keeps its
# unhashed alias (HTML points at it), but the WASM does NOT get one — nothing
# fetches the unhashed name at runtime (the `new URL("athena-frontend_bg.wasm",
# import.meta.url)` fallback only fires when init() is called with no path,
# which the bootstrap never does), so we drop it and save ~2.2 MB per copy.
for js in "$DIST_DIR"/assets/athena-frontend-dx*.js; do
  [ -f "$js" ] && cp -f "$js" "$DIST_DIR/assets/athena-frontend.js"
done
for js in "$DIST_DIR"/wasm/athena-frontend-dx*.js; do
  [ -f "$js" ] && cp -f "$js" "$DIST_DIR/wasm/athena-frontend.js"
done
# perf#5: remove the duplicates nothing fetches. In assets/ (release layout)
# that is: the unhashed WASM copy AND the hashed JS entry (the HTML loads the
# stable `athena-frontend.js` alias; the hashed JS name has no consumer once
# the alias exists). Keep the hashed WASM — the bootstrap embeds that exact
# name.
rm -f "$DIST_DIR"/assets/athena-frontend_bg.wasm \
      "$DIST_DIR"/assets/athena-frontend-dx*.js

# Cache-bust the kitty addon: WKWebView caches subresources by URL across
# app restarts, so without a fresh token the pane keeps running a stale
# addon build after every frontend rebuild.
KITTY_V="v$(date +%s)"
perl -pi -e "s|\./vendor/xterm/addon-kitty-graphics\.js(\?v=\w+)?|./vendor/xterm/addon-kitty-graphics.js?v=$KITTY_V|g" "$DIST_DIR/index.html"
echo "Kitty addon cache token: $KITTY_V"

# Replace Dioxus-generated entry documents with our custom ones.
# index.html keeps the desktop diagnostics; mobile.html mounts the same WASM
# bundle in companion mode for the installable PWA.
if [ -f "$CUSTOM_HTML" ]; then
  cp "$CUSTOM_HTML" "$DIST_DIR/index.html"
  if [ -f "$SCRIPT_DIR/public/mobile.html" ]; then
    cp "$SCRIPT_DIR/public/mobile.html" "$DIST_DIR/mobile.html"
  fi

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
  # Use perl instead of `sed -i` so this works on both macOS (BSD sed, needs
  # `sed -i ''`) and Linux runners (GNU sed, where `-i ''` misparses the script
  # as a filename). Perl is already a dependency for the path fixes below.
  perl -pi -e "s|__FRONTEND_ENTRY__|$ENTRY_PATH|g" "$DIST_DIR/index.html"
  if [ -f "$DIST_DIR/mobile.html" ]; then
    perl -pi -e "s|__FRONTEND_ENTRY__|$ENTRY_PATH|g" "$DIST_DIR/mobile.html"
  fi
  echo "Replaced entry documents with custom desktop + mobile versions"
fi

# Fix /./ paths that Dioxus generates — Tauri's custom protocol can't resolve
# absolute paths like /./wasm/... or /./assets/...
# These must become relative paths: ./wasm/... ./assets/...
perl -pi -e 's|href="/\./|href="./|g; s|src="/\./|src="./|g' "$DIST_DIR/index.html"

# Fix the same pattern inside the JS entry bundle. Only the entry alias is
# rewritten: it contains the wasm-bindgen bootstrap fetch URLs. Rewriting
# every *.js would silently corrupt unrelated bundles whose string literals
# legitimately contain "/./assets/..."
for js in "$DIST_DIR/assets/athena-frontend.js" "$DIST_DIR/wasm/athena-frontend.js"; do
  [ -f "$js" ] && perl -pi -e 's|"/\./|"./|g; s|/\./assets/|./assets/|g; s|/\./wasm/|./wasm/|g' "$js"
done

echo "Done. Files in $DIST_DIR:"
find "$DIST_DIR" -type f -o -type l | sort
