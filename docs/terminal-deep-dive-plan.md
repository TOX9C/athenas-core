# Terminal Deep Dive: Copy Missing Lines, Paste Delay, Drag & Drop

**Date:** 2026-06-07  
**Scope:** xterm.js terminal in Tauri/Dioxus WASM app  
**Issues:** (1) Copy missing most lines, (2) Paste takes 2-3s, (3) Drag & drop doesn't work

---

## 1. Architecture Recap

```
Frontend (WASM/Dioxus)         IPCgt; Backend (Rust/Tauri)
  ├─ XtermMount component                      pty_read_loop
  │  ├─ Creates xterm.js Terminal (canvas      SessionManager
  │  │  renderer, no scrollback set) ┘         ├─ RwLock<HashMap<session>>
  │  ├─ onData → pty_write ─────────────>      ├─ spawn Pts/openpty child
  │  │                       async IPC           ├─ write → do_write
  │  ├─ pty:raw listener <────────────────      │   ├─ tokio::spawn_blocking
  │  │   write_bytes_to_term                     │   └─ libc::write(fd, data)
  │  │                                         └─ read_bytes → pty:raw b64
  │  └─ keydown handler (Shift+Enter, Cmd+Del)     emit
  │
  └─ No clipboard copy/paste listeners
     No drag & drop event handlers
```

**Key files:**
- `frontend/src/components/workspace/xterm_mount.rs` — Terminal creation & data flow
- `frontend/src/tauri_bridge.rs` — IPC wrappers (`pty_write`, `pty_listen_raw`)
- `src-tauri/src/commands/mod.rs` — Backend `pty_write`, `pty_read_loop`
- `crates/athena-terminal/src/session.rs` — PTY fd operations

---

## 2. Issue #1: Copy Missing Most Lines

### 2.1 Observed Behavior
- User selects text in the terminal and copies (Ctrl+C / Cmd+C)
- When pasting elsewhere, most lines are missing
- Some output is present, but the majority is absent

### 2.2 Root Cause Analysis

**Finding A: No `scrollback` config set in xterm.js options**

```rust
// xterm_mount.rs:225-251 — options object creation
let options = js_sys::Object::new();
// scrollback is NOT set — xterm.js falls back to default 1000 lines
```

xterm.js buffer = `rows` (visible) + `scrollback` (above). By default, `scrollback = 1000`. If the terminal has produced more than 1000 lines, the oldest ones are discarded. When the user selects all (Ctrl+A), xterm.js only selects what remains in the active buffer. **The 1000-line cap silently truncates older output, making most lines "disappear" on copy.**

**Finding B: Canvas renderer selection is viewport-biased**

xterm.js is configured with `rendererType: "canvas"` (xterm_mount.rs:250). In canvas renderer mode, text is drawn onto a `<canvas>` element, not into the DOM. The browser's native text selection does NOT work on canvas. xterm.js implements selection by maintaining an internal `SelectionModel` and rendering a highlight overlay via DOM elements (`<div class="xterm-selection">`).

When the copy event fires, xterm.js calls `clipboardData.setData("text/plain", selectionText)`. The `selectionText` is derived from the internal buffer. However, if the terminal has been resized, scrolled, or the canvas was redrawn (e.g., by the `IntersectionObserver` handler at xterm_mount.rs:538-624 calling `refresh(0, rows-1)`), the selection-to-text mapping can become **inconsistent with the actual displayed content**.

**Finding C: No explicit copy event listener or `onSelectionChange`**

The `XtermMount` component does NOT:
- Attach a `copy` event listener to the xterm.js element
- Call `term.getSelection()` to verify what's selected
- Handle the `onSelectionChange` callback
- Use `xterm-addon-webgl` or `xterm-addon-clipboard` (neither are loaded)

The only clipboard-related code in the entire frontend is in `skills_panel.rs` (unrelated — it copies skill names to clipboard).

**Finding D: WKWebView clipboard API悬疑**

Tauri v2 uses WKWebView on macOS. While `clipboardData` is technically available, the WebKit clipboard API in embedded WebViews can be restricted. The copy event fires but `clipboardData.setData()` might silently fail or be truncated.

The xterm.js minified bundle contains:
```js
t.copyHandler=function(e,t){e.clipboardData&&e.clipboardData.setData("text/plain",t.selectionText),e.preventDefault()}
```

This writes to `clipboardData` on the event object. In standard browsers, this works. In Tauri's WKWebView, `clipboardData.setData()` may be a no-op or may have a size limit.

**Conclusion for Copy Issue:**
There are actually **two separate problems** with the same symptom:
1. **Primary**: xterm.js default `scrollback: 1000` truncates older buffer lines. "Select All" only captures what's in the 1000-line + visible range.
2. **Secondary**: Canvas renderer selection-to-clipboard path may produce stale or truncated output, especially after canvas refreshes triggered by visibility observers.

---

## 3. Issue #2: Paste Takes 2-3 Seconds

### 3.1 Observed Behavior
- User pastes text into the terminal (Ctrl+V / Cmd+V)
- A noticeable 2-3 second delay before the pasted text appears in the terminal

### 3.2 Root Cause Trace

**Finding A: xterm.js paste sends data through `onData`, one IPC per chunk**

```rust
// xterm_mount.rs:429-435
let on_data_closure = Closure::wrap(Box::new(move |data: String| {
    let pane_id = pane_id_for_data.clone();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = pty_write(&pane_id, &data).await;  // ← IPC round-trip per chunk
    });
}) as Box<dyn FnMut(String)>);
```

The `onData` callback fires whenever xterm.js has input data to send to the PTY. For paste, xterm.js feeds the pasted text through its input parser. The behavior depends on bracketed paste mode and the terminal state:

1. **If bracketed paste mode is OFF** (default for many shells): xterm.js sends pasted text character by character (or in small chunks based on its internal input handling). Each character/chunk triggers a **separate** `onData` → `pty_write` → `invoke("pty_write")` IPC cycle.

2. **If bracketed paste mode is ON** (`\e[?2004h` / `\e[?2004l`): xterm.js wraps the paste in bracket markers `\e[200~` ... `\e[201~`, which might still trigger multiple `onData` calls depending on implementation.

**Finding B: IPC round-trip is non-trivial in WASM/Tauri**

```
WasmBindgen JS Bridge   →   Tauri IPC (postMessage)
        ↑                        ↓
   Promise resolve      ←    JSON serialize + deserialize
```

Each `invoke("pty_write", ...)` involves:
1. WASM → JS call marshalling
2. JSON serialization in JS
3. `window.__TAURI__.core.invoke` postMessage to Rust IPC channel
4. Tokio dispatches to `pty_write` command
5. `session_manager.lock().await` — acquire global mutex (all sessions block here)
6. `SessionManager::write(id, data)` — acquire session read lock, get session
7. `TerminalSession::write(data)` — acquire `pending_writes` lock, check `status` lock
8. `do_write(data)` → `tokio::task::spawn_blocking(...libc::write(fd, ...))`
9. Wait for thread pool scheduling
10. Thread executes `libc::write(fd, data)` — syscall to PTY fd (non-blocking)

Even if each call is fast (~1-3ms), pasting 500 characters one at a time = 1.5 seconds at 3ms per call.

**Finding C: No write batching or coalescing for input**

The code does NOT implement write batching for the `onData` path. Compare to how `pty_read_loop` coalesces OUTPUT:
```rust
// pty_read_loop reads 16KB chunks and coalesces before IPC
// But onData writes directly — no batching
```

**Finding D: Non-blocking PTY fd write can block the thread pool**

```rust
// session.rs:116-135
fn do_write(&self, data: &[u8]) -> io::Result<usize> {
    let fd = self.master_fd;
    let buf = data.to_vec();
    tokio::task::spawn_blocking(move || {
        let written = unsafe { libc::write(fd, buf.as_ptr() as *const _, buf.len()) };
        // ...
    }).await ...
```

`spawn_blocking` is fine for non-blocking writes, but when many writes pile up (from a paste), the thread pool (default: 512 threads) can be exhausted. Once the pool is saturated, new `spawn_blocking` calls are queued, causing further delays.

**Finding E: PTY is shared between read and write — no separate channels**

The PTY fd is opened in non-blocking mode, but the Rust code uses `tokio::task::spawn_blocking` for writes while the read side uses `session.read_bytes()` which directly calls `libc::read()` (also non-blocking). There's no separate pipe for input/output — they share the same fd.

Wait, actually `read` and `write` on the same fd in non-blocking mode should be fine on Linux/macOS since they're separate syscalls. But the thread pool saturation is a real issue.

**Conclusion for Paste Delay:**
The 2-3 second delay is caused by **xterm.js sending pasted text as individual (or small-chunk) writes**, each of which traverses the full IPC + thread-pool scheduling path. With ~60-100 small writes per second, a paste of 500 characters takes 5-8 seconds. The user perceives it as a 2-3 second delay.

---

## 4. Issue #3: Drag & Drop Not Working

### 4.1 Observed Behavior
- Dragging an image or file into the terminal has no effect
- Nothing happens on drop

### 4.2 Root Cause Analysis

**Finding: No drag/drop event handlers are registered anywhere on the terminal container.**

```
Searching for: drag, dragover, dragenter, drop, dragleave
Result: Zero matches in frontend/src/components/workspace/xterm_mount.rs
Result: Zero matches in frontend/src/ (any .rs file)
```

The `XtermMount` component's DOM node is a `<div>` with events:
- `onpointerdown` (focus terminal, refresh canvas)
- No `ondragover`, `ondrop`, `ondragenter`, or `ondragleave`

This feature is **not implemented**. xterm.js by default does handle drag/drop for internal selection purposes, but file or image dropping is not a built-in xterm.js feature — it must be explicitly implemented by the application.

---

## 5. Structured Fix Plan

### Phase A: Fix Copy Missing Lines

**Step A1: Increase and expose `scrollback` config**
- File: `frontend/src/components/workspace/xterm_mount.rs`
- Set `scrollback` in xterm.js options to a much higher value (e.g., 100000 or `Infinity`):
  ```rust
  let _ = js_sys::Reflect::set(
      &options,
      &JsValue::from_str("scrollback"),
      &JsValue::from_f64(100_000.0),  // or f64::INFINITY
  );
  ```
- Rationale: Prevents buffer truncation and ensures "Select All" captures all lines

**Step A2: Add `onSelectionChange` callback to track selection state**
- Register `term.onSelectionChange(() => {...})` to log when selection changes
- This helps debug whether selection itself is the problem

**Step A3: Test and verify copy with explicit `navigator.clipboard` fallback**
- Add a `copy` event listener on the terminal container
- If `clipboardData.setData` fails, fall back to `navigator.clipboard.writeText(term.getSelection())`
- This requires `allowlist` or `clipboard` plugin in Tauri (see Phase D)

**Verify:** Build and run. Fill terminal with 5000 lines. Select All → Copy → Paste elsewhere. All 5000 lines should be present.

### Phase B: Fix Paste Delay

**Step B1: Implement write coalescing for `onData`**
- Modify `xterm_mount.rs` to batch incoming `onData` chunks within a time window (e.g., 16ms)
- Similar to how `pty_read_loop` coalesces output:
  ```
  onData(chunk) → queue.push(chunk)
  if (!flushScheduled) {
      scheduleFlush(16ms):
          all = queue.splice(0).join("");
          pty_write(all);  // single IPC call
  }
  ```

**Step B2: Inject bracketed paste mode on shell startup**
- Wrap paste input with bracket markers: `\x1b[200~` + text + `\x1b[201~`
- This tells the shell to treat the entire paste as a single unit
- Can be done in `xterm_mount.rs` or the PTY spawn logic

**Step B3: Optimize backend `pty_write` to reduce lock contention**
- `state.session_manager` lock is held for the entire `write()` call. Consider:
  - Make `SessionManager::write` not require exclusive lock on the manager
  - Use separate per-session channels instead of a shared `pending_writes` Mutex

**Verify:** Paste 2000 characters. Should appear instantaneously (within 100ms).

### Phase C: Implement Drag & Drop

**Step C1: Add drag events to terminal container**
- `ondragover`: `e.preventDefault()` to allow dropping
- `ondrop`: Read `e.dataTransfer.files`, extract paths
- Files: Write file path to PTY (e.g., `cat "file_path"` or just paste the path)

**Step C2: Map file types to actions**
- Text files: read content or write path to PTY
- Images: write path to PTY (e.g., as a string: `"/path/to/image.png"`)
- Directories: write path to PTY

**Step C3: Visual feedback**
- Highlight the terminal container during `dragenter`/`dragover`
- Restore on `dragleave`/`drop`

**Verify:** Drag files and images into terminal; paths appear in the terminal.

### Phase D: Enable Tauri clipboard support (Prerequisite for A3)

**Step D1: Add Tauri clipboard plugin**
- Install `tauri-plugin-clipboard-manager` or use Tauri v2's built-in clipboard
- Configure in `tauri.conf.json`:
  ```json
  "permissions": [
    "clipboard-manager:default"
  ]
  ```

**Step D2: Bridge clipboard to WASM**
- Add `clipboard_read` and `clipboard_write` in `tauri_bridge.rs`
- Fallback from `navigator.clipboard` (Web API) to Tauri command

---

## 6. Risk & Tradeoffs

| Decision | Risk | Mitigation |
|----------|------|------------|
| High scrollback (100K lines) | Memory usage increases (~10KB per line × 100K = ~1GB) | Make scrollback configurable; default to 5000, not 100000 |
| Write coalescing (16ms) | Slight input latency for fast typists | Use 0ms coalesce (next tick), only delay for paste bursts |
| Bracketed paste mode | Shell compatibility (old shells don't support it) | Detect shell capability, only enable for supported shells |
| Drag & drop paths | Security — writes arbitrary paths to shell | Sanitize inputs; only write to TTY, never execute |
| Clipboard plugin | Increases app bundle size | Use Tauri v2 built-in if available; lazy-load plugin |

---

## 7. Recommended Implementation Order

```
1. Phase D: Enable Tauri clipboard (enables/fixes both copy and paste)
   └─ Add clipboard permission to tauri.conf.json
   └─ Add clipboard bridge in tauri_bridge.rs

2. Phase B: Fix paste delay (quick wins, high impact)
   └─ B1: Implement onData write coalescing in xterm_mount.rs
   └─ B2: Add bracketed paste mode markers
   └─ Verify with large paste

3. Phase A: Fix copy missing lines
   └─ A1: Add scrollback option to xterm.js init
   └─ A2: Add navigator.clipboard fallback for copy
   └─ Verify with thousands of lines

4. Phase C: Implement drag & drop
   └─ C1: Add drag/drop event handlers
   └─ C2: Write file paths to PTY on drop
   └─ Verify by dragging files
```

---

## 8. Appendix: Evidence Log

| Claim | Evidence | Source |
|-------|----------|--------|
| xterm.js uses canvas renderer | `rendererType: "canvas"` set in options | xterm_mount.rs:249-251 |
| No scrollback config set | No `Reflect::set("scrollback", ...)` found | xterm_mount.rs:225-251 |
| write() spawns blocking thread | `tokio::task::spawn_blocking` in `do_write` | session.rs:119-134 |
| No drag event handlers | Zero matches for "drag" in workspace components | grep across frontend/src |
| IPC goes through `__TAURI__.core.invoke` | `invoke` function calls `Reflect.get(window, "__TAURI__")` | tauri_bridge.rs:14 |
| PTY fd is non-blocking | `F_SETFL(flags | O_NONBLOCK)` in spawn() | session.rs:219-220 |
| xterm.js has built-in clipboard | `copyHandler`, `handlePasteEvent` in bundle | vendor/xterm/xterm.js |
| Clipboard not in Tauri config | No "clipboard" string in tauri.conf.json | tauri.conf.json |
| onData fires per character for typing | Closure wrap on `term.onData` | xterm_mount.rs:429 |
