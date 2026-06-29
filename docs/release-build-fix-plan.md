# Release Build Not Reflecting Frontend Changes — Root Cause & Fix Plan

## Symptom
- `cargo tauri dev` → new CSS/frontend changes are visible ✓
- `cargo tauri build` → old CSS/frontend changes still shown ✗ ("never updates")

---

## Investigation Summary

### 1. Build Pipeline Analysis

**`tauri.conf.json`:**
- `frontendDist` points to `../frontend/dist/` — the modern Tauri v2 way to serve frontend
- `bundle.resources` ALSO lists the same files from `../frontend/dist/` to be copied into Resources
- `beforeBuildCommand` runs `frontend/build-dist.sh` which correctly rebuilds and copies to `dist/`

**`build-dist.sh`:**
- Runs `dx build --release` (or `--debug` for dev)
- Copies output from `target/dx/.../public/` to `frontend/dist/`
- Replaces `index.html` with custom version (WASM fixes + console capture)
- This script WORKS — the `dist/` directory IS being updated (verified via timestamps and content)

### 2. Root Causes Identified

#### ROOT CAUSE #1: Redundant `bundle.resources` (CRITICAL)

The `bundle.resources` in `tauri.conf.json` copies the same frontend files that are already in `frontendDist`:

```json
"bundle": {
    "resources": {
      "../frontend/dist/index.html": "./index.html",
      "../frontend/dist/styles.css": "./styles.css",
      "../frontend/dist/assets/": "./assets/",
      "../frontend/dist/vendor/": "./vendor/"
    }
}
```

**Why this breaks the build:**
- `frontendDist` is the modern Tauri v2 way — files are embedded into the binary and served via `tauri://localhost`
- `bundle.resources` is for ADDITIONAL runtime files (libs, configs, data files) — NOT frontend assets
- When both point to the same files, Tauri bundles TWO copies:
  1. **Embedded copy** via `frontendDist` (used by the WebView)
  2. **Resources directory copy** via `bundle.resources` (accessible via API)
- In release mode, the WebView can load from EITHER location. If the Resources copy is used, it shows a STALE version from a previous build.
- macOS bundles these into `.app/Contents/Resources/`, and the OS/WebView can cache these aggressively.

#### ROOT CAUSE #2: Aggressive WebView Caching (HIGH)

macOS WKWebView caches files aggressively in release mode:
- No `Cache-Control` headers are set for `tauri://localhost` requests
- The WebView may serve old cached content even if files ARE updated
- Dev mode works because the dev server sends `no-cache` headers
- Release mode uses `tauri://localhost` custom protocol — no cache busting by default

#### ROOT CAUSE #3: `dx build` Cache Not Fully Invalidated (MEDIUM)

The `build-dist.sh` script partially cleans `dx` cache but doesn't catch all stale artifacts:
```bash
# Only cleans specific hashed files:
find "$BUILD_DIR/assets" -maxdepth 1 \
  \( -name 'athena-frontend_bg-dx*.wasm' -o -name 'athena-frontend-dx*.js' \) -delete
rm -f "$BUILD_DIR/styles.css"
```
Other files (unhashed JS, wasm directory, etc.) might be skipped if `dx` decides to reuse them.

---

## Fix Plan

### Fix #1: Remove Redundant `bundle.resources` (CRITICAL)

**File:** `src-tauri/tauri.conf.json`
**Action:** Delete the frontend file entries from `bundle.resources`. Keep `bundle.resources` only for true runtime resources (if any).

```json
"bundle": {
    "active": true,
    "targets": "all"
    // REMOVE: resources block for frontend files
}
```

**Why:** These files are already handled by `frontendDist`. Having them in both places causes Tauri to bundle duplicates, and the WebView may load the wrong (stale) copy.

### Fix #2: Add Cache-Busting to Prevent WebView Caching (HIGH)

**File:** `frontend/index.html`
**Action:** Add cache-control meta tags and cache-busting query strings:

```html
<!-- Add in <head> -->
<meta http-equiv="Cache-Control" content="no-cache, no-store, must-revalidate">
<meta http-equiv="Pragma" content="no-cache">
<meta http-equiv="Expires" content="0">

<!-- Cache-bust CSS -->
<link rel="stylesheet" href="./styles.css?v=BUILD_TIMESTAMP" />
```

The `build-dist.sh` script should inject a fresh timestamp (e.g., current epoch seconds) into the query string every build.

**Options for CSS cache-busting:**
1. Append build timestamp as query string (e.g., `styles.css?v=1700000000`)
2. Hash-based renaming (e.g., `styles.abc123.css`)
3. Use a `<link>` tag with data-version attribute

### Fix #3: Add Content-Hash to `styles.css` in `build-dist.sh` (HIGH)

**File:** `frontend/build-dist.sh`
**Action:** After copying `styles.css`, compute its hash and also copy a hashed version. Update `index.html` to reference the hashed version.

This ensures that every CSS change gets a different filename, completely defeating any caching.

### Fix #4: Clean `dx` Cache More Aggressively (MEDIUM)

**File:** `frontend/build-dist.sh`
**Action:** Add a more thorough cache clean before building:

```bash
# Clean ALL of dx's cached build artifacts - not just specific files
rm -rf "$BUILD_DIR"
rm -rf "$PROJECT_ROOT/target/dx/athena-frontend/release"  # or debug
```

This forces a completely fresh build every time, at the cost of longer build times.

### Fix #5: Add Cache-Control Headers for Custom Protocol (MEDIUM)

**File:** `src-tauri/src/main.rs` (or appropriate handler)
**Action:** Configure Tauri's custom protocol to send `Cache-Control: no-cache` headers for all asset responses.

This tells the WebView to NEVER cache `tauri://localhost` resources, ensuring changes are always visible.

---

## Implementation Priority

| Priority | Fix | Effort | Impact |
|----------|-----|--------|--------|
| P0 | Remove redundant `bundle.resources` | 2 min | CRITICAL — likely the main cause |
| P1 | Add cache-busting query strings | 5 min | HIGH — prevents WebView caching |
| P1 | Content-hash `styles.css` in build script | 10 min | HIGH — bulletproof CSS freshness |
0｜P2｜Thorough dx cache clean | 5 min | MEDIUM — eliminates dx stale artifacts |
| P2 | Cache-Control headers for custom protocol | 10 min | MEDIUM — prevents ALL future caching |

---

## How to Verify the Fix

1. **Make a visible CSS change** (e.g., change a background color to bright red)
2. **Run `cargo tauri build`**
3. **Open the built app** (from `src-tauri/target/release/...` or `.dmg`)
4. **Check if the CSS change is visible**
5. **Repeat with another color** to verify it updates consistently

If the CSS change is visible after the first build, the fix works. If not, investigate deeper caching issues.
