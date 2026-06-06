# Terminal Content Disappears When Expanding Sidebar — Fix Applied

## Bug Description
When expanding the left sidebar (from compact icon strip to full panel), terminal content in the workspace area randomly disappears. Content reappears after pressing Enter, scrolling, or clicking the terminal. The bug is intermittent — it happens sometimes, not always.

**Key distinction**: This happens when expanding/collapsing the sidebar while staying on the Workspace panel. Not related to switching between panels (Workspace ↔ Editor).

---

## Root Cause

When the sidebar expands, the main workspace area contracts via CSS flex layout. This triggers xterm.js's `ResizeObserver` → `fit()` call, which races with the CSS layout transition. xterm.js's `<canvas>` element can end up with incorrect dimensions during this race, appearing blank.

---

## Fix Applied (3 changes in `xterm_mount.rs`)

### 1. ✅ Debounced `fit()` in `ResizeObserver` (lines 440–444, 777–791)

**Before:** `ResizeObserver` fired → immediate `schedule_fit()` → `fit()` during CSS transition → wrong dimensions → blank canvas

**After:** `ResizeObserver` fires → `debounced_fit()` waits 150ms for CSS transition to settle → `fit()` with correct dimensions

```rust
fn debounced_fit(window: &web_sys::Window, fit_instance: &JsValue, container: &web_sys::Element) {
    let fit_for_cb = fit_instance.clone();
    let container_for_cb = container.clone();
    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
        call_fit(&fit_for_cb, &container_for_cb);
    }) as Box<dyn FnMut()>);
    let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
        closure.as_ref().unchecked_ref(),
        150,
    );
    closure.forget();
}
```

The `ResizeObserver` callback now calls `debounced_fit()`:

```rust
let ro_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
    if let Some(w) = web_sys::window() {
        debounced_fit(&w, &fit_for_ro, &container_for_ro);
    }
}) as Box<dyn FnMut()>);
```

### 2. ✅ `IntersectionObserver` for Visibility Changes (lines 477–536)

When the terminal container becomes visible after being hidden (e.g., by a `display: none` toggle), the browser discards the `<canvas>` backing store. The `IntersectionObserver` detects when the container becomes visible and forces a full redraw:

```rust
let vis_closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |entries: JsValue| {
    let Ok(arr) = entries.dyn_into::<js_sys::Array>() else { return; };
    for entry in arr.iter() {
        let is_intersecting = js_sys::Reflect::get(&entry, &JsValue::from_str("isIntersecting"))
            .ok().and_then(|v| v.as_bool()).unwrap_or(false);
        if is_intersecting {
            let rows = /* ... get rows ... */;
            if rows > 0 {
                let _ = refresh_fn.call2(&term, &0.0, &(&rows - 1));
            }
            // Also refit after becoming visible again:
            // ...setTimeout(50ms) → fit()...
        }
    }
}))
```

### 3. ✅ `refresh()` on Click (lines 651–663)

When the user clicks on the terminal, it forces a full refresh in case the canvas was blanked. This serves as a quick, safe bail-out:

```rust
if let Some(term) = term_ref() {
    // Focus the terminal
    if let Ok(focus_val) = js_sys::Reflect::get(&term, &JsValue::from_str("focus")) { ... }
    // Safety net: force full redraw if canvas was blanked
    if let Ok(rows_val) = js_sys::Reflect::get(&term, &JsValue::from_str("rows")) {
        if let Some(rows) = rows_val.as_f64() {
            if let Ok(refresh_val) = js_sys::Reflect::get(&term, &JsValue::from_str("refresh")) {
                if let Ok(refresh_fn) = refresh_val.dyn_into::<js_sys::Function>() {
                    let _ = refresh_fn.call2(&term, &JsValue::from_f64(0.0),
                        &JsValue::from_f64(rows - 1.0));
                }
            }
        }
    }
}
```

---

## Files Modified

| File | Change |
|------|--------|
| `frontend/src/components/workspace/xterm_mount.rs` | Added `debounced_fit()`, `IntersectionObserver` visibility handler, `refresh()` on click. Updated `XtermCleanup` struct with observer fields. Updated `use_drop` to disconnect `IntersectionObserver`. |

---

## Testing

1. Open the app, create a workspace with a terminal
2. Expand/collapse the left sidebar multiple times
3. The terminal content should remain visible without pressing Enter
4. If content ever appears blank, clicking the terminal should restore it

---

## Build Status

- ✅ Backend: `cargo check --manifest-path src-tauri/Cargo.toml` — passes
- ✅ Frontend: `cargo check --manifest-path frontend/Cargo.toml` — passes
- ✅ Tests: `cargo test --workspace` — all pass
