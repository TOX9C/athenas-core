# Handoff — Broadcast Bug Fix + Layout Issue

**Date:** 2026-06-01  
**Session Model:** `omniroute/glm` (OmniRoute caveman mode)  
**Note:** Vision/image input **not available** in this session — cannot view screenshots directly.

---

## 1. What Was Done

### Tauri Broadcast Race Condition — FIXED and VERIFIED

**Bug:** 3 concurrent PTY sessions emit `pty:raw` events, but only the last session's payloads reach the frontend. Panes 0 and 1 stop receiving updates after initial output.

**Root cause:** `app_handle.emit("pty:raw", &serde_json::Value)` — the `&Value` borrow in concurrent tokio tasks has a borrow-sharing race. One task overwrites the buffer before another task's emit reads it.

**Fix:** Serialize `&Value` to owned `String` via `serde_json::to_string(data)` before `emit()`. Applied to all 12 emit sites:

| File | Sites | Events |
|------|-------|--------|
| `src-tauri/src/commands/mod.rs` | 4 | `pty:raw`, `terminal:ready`, `terminal:data`, `terminal:exit` |
| `src-tauri/src/state.rs` | 8 | `athena:askUser` + 7 `wire_*_events` closures |

**Verification:**
- `cargo build` — clean
- `cargo test --workspace` — all pass
- E2E via `tauri-wd`: `node e2e-tests/test_broadcast_direct.mjs` — 3 distinct sessionIds, 16 payloads delivered, zero clobbering

**Also completed:**
- Stripped all 25 `[DIAG]` diagnostic logs across 3 files (`commands/mod.rs`, `tauri_bridge.rs`, `xterm_mount.rs`)
- Removed `LISTEN_REGISTRY_COUNTER` static + atomic import from `tauri_bridge.rs`
- Added regression test at `e2e-tests/test_broadcast_direct.mjs` with ~40-line header
- Rebuilt frontend dist in release mode

---

## 2. Screenshot Issue — COULD NOT VIEW

User shared screenshot: `CleanShot 2026-06-01 at 16.05.08@2x.png` (2788x1792, Retina).

**Attempted:**
- `look_at` tool — timed out after 120s
- `multimodal-looker` agent — produced garbage output
- `filesystem_read_media_file` — "could not be resized below inline image size limit"
- Direct model vision — "this model does not support image input"

**Result:** Unable to view the image in this session. Need a vision-capable model or user description.

---

## 3. Likely Layout Issue — Workspace Grid Not Filling Space

Based on the codebase analysis and the user's hint ("not taking the whole space"), the issue is likely in the **flex container centering** in the workspace panel.

### Primary Suspect: `WorkspacePanel` in `frontend/src/components/workspace/mod.rs`

**Line 30:**
```rust
style: "flex: 1; display: flex; align-items: center; justify-content: center; overflow: auto;",
```

The `align-items: center` prevents the `WorkspaceGrid` from stretching to fill the available height. The grid's `flex: 1` (line 81 of `terminal_grid.rs`) can't override the parent's `align-items` constraint.

### Secondary Suspect: `xterm_mount.rs` — ResizeObserver Race

The `FitAddon` uses a `requestAnimationFrame` delay (line 244) before its first `fit()`. If the grid layout hasn't finalized by that frame, the terminal canvas gets sized to a stale width. The `ResizeObserver` should fix this on subsequent resizes, but if it never fires because the container size doesn't change after mount, the terminal stays at the wrong size forever.

### Tertiary Suspect: `terminal_grid.rs` — PaneItem Styling

Line 161:
```rust
style: "border: 1px solid var(--border); border-radius: 4px; overflow: hidden; display: flex; flex-direction: column; min-height: 0; min-width: 0; flex: 1; background: var(--bg);{span_style}",
```

The `flex: 1` and `min-height: 0` are correct for grid children, but the 1px border and 2px grid padding consume space that might cause layout collapse in edge cases.

### Potential Fixes (by priority):

1. **Fix the flex container centering** — add `align-items: stretch` or remove `align-items: center` from the content container in `mod.rs:30`
2. **Ensure xterm div fills** — verify the xterm mount div (`id: pane_id`, `style: "width: 100%; height: 100%; ..."`) actually receives the full container dimensions
3. **Check grid-template sizing** — `grid-template-columns: 1fr 1fr 1fr; grid-template-rows: 1fr` should work with `min-height: 0` on grid parent

---

## 4. Uncommitted Changes

All changes are **uncommitted** (user hasn't requested commit):

- `src-tauri/src/commands/mod.rs` — emit fixes + DIAG strip
- `src-tauri/src/state.rs` — emit fixes
- `frontend/src/tauri_bridge.rs` — DIAG strip + LISTEN_REGISTRY_COUNTER removal
- `frontend/src/components/workspace/xterm_mount.rs` — DIAG strip
- `frontend/dist/` — rebuilt release WASM
- `e2e-tests/test_broadcast_direct.mjs` — NEW regression test

---

## 5. Relevant Files for Layout Fix

| File | Purpose |
|------|---------|
| `frontend/src/components/workspace/mod.rs:30` | **PRIMARY** — flex centering prevents grid stretch |
| `frontend/src/components/workspace/terminal_grid.rs:81` | Grid container with `flex: 1` |
| `frontend/src/components/workspace/terminal_grid.rs:161` | PaneItem styling — flex child constraints |
| `frontend/src/components/workspace/xterm_mount.rs:340-345` | xterm mount div — width/height 100% |
| `frontend/src/components/workspace/xterm_mount.rs:231-291` | FitAddon + ResizeObserver — delayed fitting |

---

## 6. E2E Testing Setup

```bash
# Terminal 1: Start WebDriver server
tauri-wd --port 4444

# Terminal 2: Run regression test
node e2e-tests/test_broadcast_direct.mjs
```

Frontend must be built in **RELEASE** mode (`bash frontend/build-dist.sh` — no `--debug`). Debug builds include Dioxus devtools that panic in WKWebView.

---

## 7. Next Steps

1. **Fix the layout centering** in `mod.rs:30` — change to `align-items: stretch` or remove centering
2. **Test visually** — user needs a vision-capable model to verify the screenshot issue
3. **Commit** — once layout is fixed and verified