#!/bin/bash
# Build the Dioxus WASM frontend and copy to frontend/dist, then launch Tauri dev.
set -e

REPO_ROOT="$(cd "$(dirname "$0")" && pwd)"
DIST_DIR="$REPO_ROOT/frontend/dist"
DX_OUTPUT="$REPO_ROOT/target/dx/athena-frontend/debug/web/public"

echo "Building Dioxus frontend..."
~/.cargo/bin/dx build --package athena-frontend

echo "Copying build output to $DIST_DIR..."
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"
cp -r "$DX_OUTPUT"/* "$DIST_DIR"/

echo "Launching Tauri dev..."
cd "$REPO_ROOT"
cargo tauri dev
