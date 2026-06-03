# Terminal Grid Resize Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make workspace pane resizing update both the visual grid allocation and each terminal's real PTY/xterm dimensions.

**Architecture:** Fix the parent flex container so the CSS grid owns the full workspace viewport, then make xterm fitting authoritative and observable. Column/row drag should update CSS grid fractions, ResizeObserver should call FitAddon after layout settles, and xterm's `onResize` event should call `pty_resize` with the actual calculated cols/rows.

**Tech Stack:** Dioxus frontend, Tauri IPC, xterm.js, FitAddon, ResizeObserver, Rust PTY backend.

---

## Root Cause

The visual symptoms come from two coupled frontend layout issues:

1. `frontend/src/components/workspace/mod.rs:30` wraps `WorkspaceGrid` in a flex container with `align-items: center; justify-content: center; overflow: auto;`. In a column flex container, `align-items: center` disables cross-axis stretch, so the child grid can be visually centered instead of forced to consume the full available width. This explains the blank, unusable area and why the bottom full-row pane can appear to occupy only part of the row.

2. `frontend/src/components/workspace/xterm_mount.rs:231-291` calls `FitAddon.fit()` on initial `requestAnimationFrame` and on ResizeObserver callbacks, but the callback does not debounce to the post-layout frame and does not log/verify the `cols`/`rows` propagated through xterm `onResize`. When the grid fractions change, the pane boxes move visually, but xterm can keep stale geometry long enough for text clipping or unused terminal cells. The backend resize path itself is present and correct: `frontend/src/tauri_bridge.rs:440`, `src-tauri/src/commands/mod.rs:679`, and `crates/athena-terminal/src/session.rs:241`.

Secondary issue to guard against: `frontend/src/components/workspace/terminal_grid.rs:447-448` clamps each adjacent column independently with `.max(0.1)`, which can increase the total fraction sum during extreme drags. That does not cause the initial blank area, but it can make divider position drift and produce layouts that no longer match the drag delta exactly.

## File Structure

- Modify: `frontend/src/components/workspace/mod.rs` — make the active workspace content area stretch, not center, when a grid is mounted.
- Modify: `frontend/src/components/workspace/terminal_grid.rs` — keep drag fraction totals stable and add testable pure helpers for divider math.
- Modify: `frontend/src/components/workspace/xterm_mount.rs` — refit after layout settles and log/propagate actual xterm cols/rows.
- Create: `e2e-tests/test_workspace_resize_geometry.mjs` — regression test that inspects pane rectangles and terminal cols after dragging/resizing.

---

### Task 1: Stretch Workspace Grid Container

**Files:**
- Modify: `frontend/src/components/workspace/mod.rs:29`

- [ ] **Step 1: Change active content alignment**

Replace the content wrapper style in `WorkspacePanel` with a stretch-safe layout:

```rust
style: "flex: 1; display: flex; align-items: stretch; justify-content: stretch; overflow: hidden; min-width: 0; min-height: 0;",
```

- [ ] **Step 2: Preserve empty-state centering**

Wrap only the empty state in a centered child so the no-workspace screen remains centered:

```rust
div {
    style: "flex: 1; display: flex; align-items: center; justify-content: center; text-align: center; color: var(--textDim);",
    div {
        div {
            style: "width: 40px; height: 40px; border-radius: 8px; background: var(--bgTertiary); display: flex; align-items: center; justify-content: center; margin: 0 auto; opacity: 0.4;",
            span { style: "font-size: 16px; font-weight: 700; color: var(--textMuted);", "W" }
        }
        span { style: "font-size: 12px; margin-top: 8px; display: block;", "Create a workspace to get started" }
        button {
            style: "margin-top: 12px; padding: 8px 16px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px;",
            "+ New Space"
        }
    }
}
```

- [ ] **Step 3: Verify grid receives full parent box**

Run the app and inspect the active grid element. Expected: `WorkspaceGrid` bounding rect equals the content wrapper rect minus only intentional padding/borders.

---

### Task 2: Stabilize Split Drag Fractions

**Files:**
- Modify: `frontend/src/components/workspace/terminal_grid.rs:434`

- [ ] **Step 1: Add helper for paired fraction resizing**

Add a pure helper near `DragInfo`:

```rust
fn resize_pair_preserving_total(values: &[f64], idx: usize, delta: f64, min: f64) -> Vec<f64> {
    let mut next = values.to_vec();
    if idx + 1 >= next.len() {
        return next;
    }

    let pair_total = next[idx] + next[idx + 1];
    let requested_left = next[idx] + delta;
    let left = requested_left.clamp(min, pair_total - min);
    let right = pair_total - left;

    next[idx] = left;
    next[idx + 1] = right;
    next
}
```

- [ ] **Step 2: Use helper for column drags**

Replace:

```rust
let mut new_widths = initial_cols.clone();
new_widths[idx] = (new_widths[idx] + delta_fr).max(0.1);
new_widths[idx + 1] = (new_widths[idx + 1] - delta_fr).max(0.1);
col_widths.set(new_widths);
```

with:

```rust
let new_widths = resize_pair_preserving_total(&initial_cols, idx, delta_fr, 0.1);
col_widths.set(new_widths);
```

- [ ] **Step 3: Use helper for row drags**

Replace:

```rust
let mut new_heights = initial_rows.clone();
new_heights[idx] = (new_heights[idx] + delta_fr).max(0.1);
new_heights[idx + 1] = (new_heights[idx + 1] - delta_fr).max(0.1);
row_heights.set(new_heights);
```

with:

```rust
let new_heights = resize_pair_preserving_total(&initial_rows, idx, delta_fr, 0.1);
row_heights.set(new_heights);
```

- [ ] **Step 4: Add focused unit tests if frontend tests compile**

Add tests in the same file or an adjacent test module:

```rust
#[test]
fn resize_pair_preserves_total() {
    let next = resize_pair_preserving_total(&[1.0, 1.0], 0, 0.5, 0.1);
    assert_eq!(next, vec![1.5, 0.5]);
    assert!((next.iter().sum::<f64>() - 2.0).abs() < f64::EPSILON);
}

#[test]
fn resize_pair_clamps_without_growing_total() {
    let next = resize_pair_preserving_total(&[1.0, 1.0], 0, 5.0, 0.1);
    assert_eq!(next, vec![1.9, 0.1]);
    assert!((next.iter().sum::<f64>() - 2.0).abs() < f64::EPSILON);
}
```

---

### Task 3: Make Xterm Fit Authoritative

**Files:**
- Modify: `frontend/src/components/workspace/xterm_mount.rs:230`

- [ ] **Step 1: Add post-layout fit helper**

Add a helper that schedules fit after the browser applies the new grid layout:

```rust
fn schedule_fit(window: &web_sys::Window, fit_instance: &JsValue) {
    let fit_for_raf = fit_instance.clone();
    let raf_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
        call_fit(&fit_for_raf);
    }) as Box<dyn FnMut()>);
    let _ = window.request_animation_frame(raf_closure.as_ref().unchecked_ref());
    raf_closure.forget();
}
```

- [ ] **Step 2: Use scheduled fit for initial fit**

Replace the inline `requestAnimationFrame` block at `frontend/src/components/workspace/xterm_mount.rs:240-247` with:

```rust
schedule_fit(&window, &fit_instance);
```

- [ ] **Step 3: Use scheduled fit for ResizeObserver**

Replace the ResizeObserver callback body:

```rust
call_fit(&fit_for_ro);
```

with:

```rust
if let Some(window) = web_sys::window() {
    schedule_fit(&window, &fit_for_ro);
}
```

- [ ] **Step 4: Add resize propagation logging during verification**

Temporarily log actual xterm resize values inside the `onResize` closure:

```rust
web_sys::console::log_1(
    &format!("XtermMount[{pane_id}]: xterm resized to {cols}x{rows}").into(),
);
```

Remove or gate this log after the regression test is passing.

---

### Task 4: Add Geometry Regression Test

**Files:**
- Create: `e2e-tests/test_workspace_resize_geometry.mjs`

- [ ] **Step 1: Create test script**

```javascript
import { remote } from 'webdriverio';

const browser = await remote({
  hostname: '127.0.0.1',
  port: 4444,
  path: '/',
  capabilities: {
    browserName: 'safari',
    'tauri:options': { application: '../src-tauri/target/release/athenas-core' },
  },
});

try {
  await browser.pause(3000);

  const geometry = await browser.execute(() => {
    const grid = document.querySelector('[style*="display: grid"][style*="grid-template-columns"]');
    const panes = Array.from(grid?.children ?? []).filter((el) => {
      const style = getComputedStyle(el);
      return style.display === 'flex' && style.flexDirection === 'column';
    });
    const gridRect = grid?.getBoundingClientRect();
    const paneRects = panes.map((el) => {
      const rect = el.getBoundingClientRect();
      return { x: rect.x, y: rect.y, width: rect.width, height: rect.height };
    });
    const terminals = Array.from(document.querySelectorAll('.xterm-mount .xterm')).map((el) => {
      const rect = el.getBoundingClientRect();
      return { width: rect.width, height: rect.height, text: el.textContent ?? '' };
    });
    return { gridRect, paneRects, terminals };
  });

  console.log(JSON.stringify(geometry, null, 2));

  if (!geometry.gridRect || geometry.gridRect.width < 1000) {
    throw new Error('Workspace grid did not stretch to available width');
  }

  const bottomPane = geometry.paneRects.at(2);
  if (!bottomPane || bottomPane.width < geometry.gridRect.width * 0.9) {
    throw new Error('Bottom pane does not span the full grid row');
  }

  const clippedTerminal = geometry.terminals.find((terminal) => terminal.width < 100);
  if (clippedTerminal) {
    throw new Error('At least one xterm mount has collapsed/clipped width');
  }
} finally {
  await browser.deleteSession();
}
```

- [ ] **Step 2: Run regression**

Run:

```bash
node e2e-tests/test_workspace_resize_geometry.mjs
```

Expected: exits with status `0` and prints pane geometry where the grid fills the workspace and the bottom pane spans almost the full grid width.

---

### Task 5: Verify End-to-End Behavior

**Files:**
- Modify only if verification exposes a missed root cause.

- [ ] **Step 1: Build release frontend**

Run:

```bash
bash frontend/build-dist.sh
```

Expected: frontend build completes and writes release assets without Dioxus devtools panic.

- [ ] **Step 2: Run app-level resize test**

Run:

```bash
node e2e-tests/test_workspace_resize_geometry.mjs
```

Expected: geometry assertions pass.

- [ ] **Step 3: Manually verify screenshot scenario**

Open a 3-pane `2x2` workspace, drag the vertical splitter, then verify:

- Top-left pane expands visually and accepts terminal input across the expanded width.
- Top-right pane shrinks visually and prompt text wraps within the smaller PTY width instead of being clipped outside stale columns.
- Bottom pane spans the entire row with no blank unusable half.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/workspace/mod.rs frontend/src/components/workspace/terminal_grid.rs frontend/src/components/workspace/xterm_mount.rs e2e-tests/test_workspace_resize_geometry.mjs docs/superpowers/plans/2026-06-02-terminal-grid-resize-fix.md
git commit -m "fix: synchronize workspace terminal resizing"
```

---

## Self-Review

- Spec coverage: Covers the visual blank area, stale terminal dimensions, top-right clipping, and bottom-row half-width symptom.
- Placeholder scan: No placeholders remain; all code changes and test commands are concrete.
- Type consistency: New helper accepts `&[f64]`, returns `Vec<f64>`, and matches existing drag state types.
