# Frontend Core Audit — Dioxus Frontend Analysis

## Scope

This audit covers the core frontend source files of the Athena's Core Tauri 2 desktop application, including the root `App` component, Tauri bridge, and all Dioxus signal-based stores.

### Files Audited

| File | Lines | Role |
|------|-------|------|
| `frontend/src/lib.rs` | ~1,200 | Root `App` component, store providers, global keybindings, UI layout |
| `frontend/src/tauri_bridge.rs` | ~700 | Tauri IPC wrapper, typed command wrappers, event listener infrastructure |
| `frontend/src/main.rs` | ~4 | Application entry point |
| `frontend/src/stores/ui.rs` | ~123 | UI theme, panel, sidebar state |
| `frontend/src/stores/terminal.rs` | ~150 | Terminal session state (xterm.js backed) |
| `frontend/src/stores/terminal_blocks.rs` | ~134 | Terminal block/command history state |
| `frontend/src/stores/athena.rs` | ~520 | Athena chat/orchestrator state |
| `frontend/src/stores/swarm.rs` | ~195 | Swarm coordination state |
| `frontend/src/stores/workspace.rs` | ~180 | Workspace/pane/grid state |
| `frontend/src/stores/editor.rs` | ~95 | Editor/file state |
| `frontend/src/stores/session.rs` | ~155 | Session management state |
| `frontend/src/stores/notification.rs` | ~115 | Notifications state |
| `frontend/src/stores/command.rs` | ~260 | Command palette state |
| `frontend/src/stores/agent_output.rs` | ~250| Agent output tracking state |
| `frontend/src/stores/agent_status.rs` | ~225 | Agent status tracking state |
| `frontend/src/stores/panel_manager.rs` | ~170 | Panel manager state |
| `frontend/src/stores/task.rs` | ~120 | Kanban task state |

## Summary of Findings

| Severity | Count |
|----------|-------|
| Critical | 3 |
| High | 5 |
| Medium | 8 |
| Low | 6 |

---

## Critical Issues

### C1: Unbounded Memory Growth in Agent Output Buffer (`agent_output.rs`)

- **Severity**: Critical
- **File**: `frontend/src/stores/agent_output.rs`
- **Line**: `MAX_LINES_PER_BUFFER: usize = 5000` (line ~33)
- **Category**: Resource leak / Memory
- **Description**: The `MAX_LINES_PER_BUFFER` constant and `trim_lines()` function only trim lines per-buffer, but the buffers themselves (identified by `pane_id`) are never pruned. When a pane is closed, `unregister_pane()` is called which removes both the agent info and buffer. **However**, if the component that calls `unregister_pane` is unmounted without proper cleanup, or if there are multiple `register_pane` calls for the same pane (component re-mount), the old Vec is simply replaced (the key exists check passes), but the *lines* Vec from before replacement is dropped. The real issue: `AgentOutputState::append_line` and `set_lines` create new buffers if the pane_id doesn't exist, and `register_pane` also creates a buffer if it doesn't exist. In a long-running app with many pane creations/destructions, if the `pane_id` values are not unique (e.g., reused IDs), the old buffer could be orphaned. More critically, `OutputLine` stores the full `text: String`, and 5000 lines of arbitrary length can be unbounded in memory.
- **Impact**: Memory leak during long sessions with many terminal panes. Could lead to WASM out-of-memory crashes.
- **Suggested Fix**: Add a periodic garbage collection pass that removes `AgentOutputInfo` entries older than a threshold and limits the total number of tracked panes. Also consider capping `OutputLine.text` length.

### C2: `Default` on `WorkspaceState` Bypasses `save()` on Mutations (`workspace.rs`)

- **Severity**: Critical
- **File**: `frontend/src/stores/workspace.rs`
- **Line**: Lines 65–128 (all mutators)
- **Category**: Signal/Reactive state / Logic error
- **Description**: All mutation methods (`set_active_space`, `add_space`, `remove_space`, `add_pane_to_space`, `remove_pane_from_space`) call `self.save()` which spawns a `wasm_bindgen_futures::spawn_local(async move { ... })` to persist. **However**, the `save()` method creates a `clone` of `self` (via `serde_json::to_string(self)`) in a `spawn_local`, but because the store is modified *after* `save()` is called and `save()` immediately returns, there's a race condition. More critically, `set_spaces()` does **not** call `save()`, and `update_space()` calls `save()` without mutating (closure may not actually mutate). Additionally, `save()` spawns an unawaited async task, meaning if the user quickly performs several operations, multiple overlapping save tasks race. Worse, if `save()` fails (network/IPC error), the state diverges from persistence. But the **critical** bug is: `save()` takes `&self`, clones it to JSON, then moves that JSON into the async block. Since `save()` is called synchronously *during* a `store.write()` block in the component, the state inside the write block might still be mid-mutation. If the async save happens to execute its serialization *after* another write, the saved state might be newer than intended, but this is mostly harmless. The real critical issue: if `save()` is called multiple times in rapid succession (e.g., `remove_space` followed by `add_space`), the second `save()` could start before the first finishes, and because `store_set` writes to a single key, the first (now stale) save could overwrite the second. This is a **write ordering / last-write-wins** race condition.
- **Impact**: Workspace state corruption or loss, especially during rapid pane operations.
- **Suggested Fix**: Implement a debounced/single-pending save mechanism. Use a `use_effect` in the component that watches the workspace signal and triggers `save()` after all mutations settle, or add a `dirty: bool` flag with a `use_effect` that saves only when necessary.

### C3: `Closure.forget()` in `pty_listen_binary` Creates Undying JS References (`tauri_bridge.rs`)

- **Severity**: Critical
- **File**: `frontend/src/tauri_bridge.rs`
- **Line**: ~542 (in `pty_listen_binary`)
- **Category**: WASM-specific / Memory leak
- **Description**: In `pty_listen_binary`, after registering the `onmessage` closure on the channel and calling `invoke`, the code calls `onmessage.forget()`. This permanently leaks the `Closure` because it is never dropped and can never be garbage-collected by Rust. The `Closure` is owned by JS as a function reference and will live forever. There is no way to stop the channel or unlisten. Every new terminal pane calls this, so creating and destroying many terminal sessions will leak memory per session.
- **Impact**: Unbounded memory growth. Each terminal session permanently leaks a `Closure`. WASM heap will grow until the app crashes on long-running sessions.
- **Suggested Fix**: Remove `.forget()` and instead store the `Closure` in a `Rc<RefCell<Option<Closure>>>` or similar, returning it alongside an unlisten function from `pty_listen_binary`. Alternatively, redesign `pty_listen_binary` to return a `Drop` guard that calls the JS unlisten and drops the closure on the Rust side when the component unmounts.

---

## High Issues

### H1: No Cleanup of Tauri Event Listeners in Components (`tauri_bridge.rs`)

- **Severity**: High
- **File**: `frontend/src/tauri_bridge.rs`
- **Line**: `listen()` function (lines ~560–650)
- **Category**: Event handling / Resource leak
- **Description**: The `listen()` function returns a `Box<dyn FnOnce()>` unlisten function, but the audit of the codebase shows no calls to this unlisten function anywhere (a grep for `.unlisten` or the return value being used returned nothing). Any component that registers a Tauri event listener (e.g., for PTY output, notifications, agent status) will leak the listener forever. In Dioxus 0.7, components re-render and `use_effect` with missing cleanup will re-register listeners.
- **Impact**: Event listener accumulation. Memory leak and duplicate event handling bugs. Potential for unbounded growth of callbacks.
- **Suggested Fix**: Ensure all `use_effect` blocks that call `listen()` capture the returned unlisten function and call it in the cleanup closure. Example pattern:
  ```rust
  use_effect(move || {
      let unlisten = crate::tauri_bridge::listen("event", |msg| { ... }).ok();
      move || { if let Some(u) = unlisten { u(); } }
  });
  ```

### H2: `onkeydown` Handler in `App` Captures Stale Mutable References (`lib.rs`)

- **Severity**: High
- **File**: `frontend/src/lib.rs`
- **Line**: `onkeydown` event handler (around line ~250)
- **Category**: Signal/Reactive state / Logic error
- **Description**: The `onkeydown` handler uses `let mut ui_state = use_ui_store()` and then mutates it inside the handler with `ui_state.write()`. In Dioxus 0.7, closures in event handlers can capture signals by value (via `Copy` for `Signal`), but the way the handler is written (directly inside the rsx) means it captures the signal handle by value, which is correct. However, the handler also uses `workspace.read()` and `workspace_mut.write()`—two separate signal handles for the *same* store. This creates a potential for split-brain: `workspace.read()` and `workspace_mut.write()` can see different states if a re-render happens mid-handler. More importantly, the handler references `active_space_pane_ids` and `terminal_store` via captured signals. But the handler does not clone the values it reads before writing, so it could read old data. The bigger issue: the `onkeydown` handler is massive and accesses many signals. If the component re-renders while the handler is queued, the captured closures could reference a dropped or reallocated signal.
- **Impact**: Race conditions in keyboard shortcuts, potential panics or inconsistent UI state.
- **Suggested Fix**: Extract the keyboard shortcut handler into a separate memoized function or `use_callback`. Clone all needed read values at the top of the handler before any writes. Use `let current_values = (workspace.read().active_space_id.clone(), ...)` to snapshot state.

### H3: `Default` Implementation on `AthenaState` is Wrong Due to `Partialeq` Custom Override (`athena.rs`)

- **Severity**: High
- **File**: `frontend/src/stores/athena.rs`
- **Line**: `impl Default for AthenaState` (line ~155)
- **Category**: Signal/Reactive state / Logic error
- **Description**: `AthenaState` derives `PartialEq` via `#[derive(Clone, PartialEq, Default)]` but the struct contains a `Vec<AthenaMessage>` and other complex fields. The derived `PartialEq` does a deep comparison of the entire message history on every signal write. In Dioxus, when `Signal::set()` or `Signal::write()` is called, the signal system compares the old and new values using `PartialEq` to determine if subscribers need to re-render. With a large message history, this `PartialEq` comparison is O(N) where N is the number of messages. Each message contains strings, vecs, etc. This can cause severe performance degradation and frame drops.
- **Impact**: Severe performance degradation as chat history grows. UI jank when adding new messages.
- **Suggested Fix**: Wrap `messages` in a `RefCell<Vec<AthenaMessage>>` and implement custom `PartialEq` that only compares a cheap discriminator (e.g., a generation counter or the hash of the last message). Or, better: don't derive `PartialEq`; implement it manually using a generation field that is incremented on mutation. Alternatively, switch to individual signals for each field rather than a monolithic `AthenaState`.

### H4: `mounted_spaces` Signal is Never Cleaned Up (`lib.rs`)

- **Severity**: High
- **File**: `frontend/src/lib.rs`
- **Line**: `mounted_spaces` (around line ~85)
- **Category**: Component lifecycle / Memory leak
- **Description**: `let mut mounted_spaces = use_signal(std::collections::HashSet::<String>::new);` tracks which space IDs have ever been mounted. Spaces are added to the set when they become active, but they are **never** removed. In a long-running session where many workspaces are created and destroyed, this set grows unboundedly. Since `Space` objects contain `Vec<PaneConfig>`, the memory cost includes all the strings and IDs from every space ever seen.
- **Impact**: Memory leak proportional to the number of workspace switches over the session lifetime.
- **Suggested Fix**: Add cleanup logic in the effect that tracks spaces: when a space is removed via `remove_space`, also remove it from `mounted_spaces`. Or, simplify: remove `mounted_spaces` entirely and compute `mounted_workspaces` directly from `spaces`.

### H5: `PanelManagerState` Does Not Affect `UIState` (`panel_manager.rs` + `lib.rs`)

- **Severity**: High
- **File**: `frontend/src/stores/panel_manager.rs`, `frontend/src/lib.rs`
- **Line**: Throughout `panel_manager.rs`
- **Category**: Signal/Reactive state / Logic error
- **Description**: `PanelManagerState` and `UIState` both track panel state with overlapping concerns. `PanelManagerState` has `active_panel: ExclusivePanel` (Browser, Editor, Athena) and `active_right_panel: RightPanel`, while `UIState` has `panel: Panel` (Workspace, Editor, Kanban, etc.) and `right_sidebar_open: bool` / `right_sidebar_tab`. The App component's `onkeydown` handler sets `ui_state.write().panel = Panel::...` but never updates the `panel_manager` state. This creates a split-brain scenario where the UI panel and the panel manager disagree. For example, pressing `Cmd+2` sets `ui_state.panel = Panel::Editor`, but `panel_manager.active_panel` would still be `ExclusivePanel::None` (its default), so any component depending on `panel_manager` would show inconsistent state.
- **Impact**: UI panels may not display correctly. Right sidebar behavior is inconsistent between keyboard shortcuts and button clicks.
- **Suggested Fix**: Consolidate the two panel state systems into a single source of truth, or ensure every `UIState` panel change also updates `PanelManagerState`. Consider making `PanelManagerState` the canonical store and deprecating `UIState.panel`.

---

## Medium Issues

### M1: `use_signal` for `is_maximized` Not Synchronized With Actual Window State (`lib.rs`)

- **Severity**: Medium
- **File**: `frontend/src/lib.rs`
- **Line**: `is_maximized` signal (around line ~72)
- **Category**: Signal/Reactive state / Logic error
- **Description**: `let mut is_maximized = use_signal(|| false);` starts as `false` and is only toggled by the maximize button click. It does not reflect the actual window state (e.g., if the user maximizes via OS native controls). Clicking the button when already maximized via OS will toggle the internal signal in the wrong direction.
- **Impact**: Maximize/restore button icon and behavior can get out of sync with actual window state.
- **Suggested Fix**: Query the actual window maximized state on mount using `window_is_maximized()`, and update `is_maximized` in response to any window state change events if Tauri exposes them.

### M2: `store_get`/`store_set` Do Not Handle Async Errors (`workspace.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/workspace.rs`
- **Line**: `save()` (around line ~115)
- **Category**: Async handling / Error handling
- **Description**: `WorkspaceState::save()` spawns an async task that calls `kv_set()` and only logs errors to the console. If the backend store is full, corrupted, or temporarily unavailable, the save silently fails. The user might close and reopen the app to find their workspace changes lost.
- **Impact**: Data loss on store failure.
- **Suggested Fix**: Return a `Result` from `save()` (or a signal-based error state) and surface it to the user. Consider retry logic or in-memory dirty flag that prevents app close until saved.

### M3: `TerminalStore::kill()` Removes Session Before Confirming Backend Kill (`terminal.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/terminal.rs`
- **Line**: `kill()` method (around line ~65)
- **Category**: Async handling / Logic error
- **Description**: `TerminalStore::kill()` first calls `tauri_bridge::pty_kill(id)` (awaited), then removes the session from the map. However, if `pty_kill` succeeds but the backend hasn't fully cleaned up the PTY, the frontend removes the session but the backend channel might still try to push data. More importantly, if `pty_kill` fails, the session is still removed from the map (the error is only logged), so the user sees the session gone but the backend PTY is still running.
- **Impact**: Orphaned PTY processes consuming system resources.
- **Suggested Fix**: Only remove the session from the map if `pty_kill` succeeds. If it fails, keep the session in the map (perhaps mark it as `exited` or keep it as-is) and show an error.

### M4: `TerminalBlocksStore::append_output` Does Not Check for Overflow (`terminal_blocks.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/terminal_blocks.rs`
- **Line**: `append_output()` (around line ~76)
- **Category**: Resource leak / Memory
- **Description**: `append_output` appends to `self.current_output` and `block.output` without any length limits. A long-running command that produces infinite output (e.g., `yes`, `tail -f`) will cause unbounded string growth. This happens on the WASM heap.
- **Impact**: Memory exhaustion, WASM panic, app crash.
- **Suggested Fix**: Cap the total output size per block. If the accumulated output exceeds a threshold (e.g., 100KB), truncate and append a `[...truncated...]` marker.

### M5: `NotificationStore` Pushes Notifications Unconditionally (`notification.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/notification.rs`
- **Line**: `add_notification()` (around line ~60)
- **Category**: Performance
- **Description**: `add_notification()` does not deduplicate incoming notifications. If a backend event fires multiple times rapidly (e.g., connection retry), identical notifications will be appended. The `MAX_NOTIFICATIONS` limit helps, but during a burst, the drain will keep removing old (potentially more important) notifications.
- **Impact**: Notification spam, loss of older important notifications.
- **Suggested Fix**: Add deduplication logic: if a notification with the same title+message is already present and unread, don't add a duplicate; instead, update the timestamp or increment a count.

### M6: `AthenaState::add_message` Uses `drain` Which Shifts All Elements (`athena.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/athena.rs`
- **Line**: `add_message()` (around line ~165)
- **Category**: Performance
- **Description**: When messages exceed `MAX_MESSAGES` (100), `drain(0..excess)` removes elements from the front of the Vec. In a `Vec`, removing from the front is an O(N) operation because all remaining elements must be shifted down. With 100 messages, this is minor, but it happens on every message addition once the limit is reached.
- **Impact**: O(N) cost per message in a high-chat-throughput scenario.
- **Suggested Fix**: Use a ring buffer (circular buffer) or `VecDeque` for message storage to get O(1) push/pop at both ends. Or use `remove(0)` (still O(N)) — better to switch to `std::collections::VecDeque` for the messages field.

### M7: `CommandState::recent_ids` is Not Persisted (`command.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/command.rs`
- **Line**: `record_execution()` (around line ~120)
- **Category**: Persistence / Feature gap
- **Description**: The command palette tracks recent commands in `recent_ids`, but this list is never persisted to the backend store. On app restart, the recent commands list is empty.
- **Impact**: Poor UX: users lose their command history across sessions.
- **Suggested Fix**: Persist `recent_ids` via `store_set` / `store_get`, or include it in the workspace state persistence.

### M8: `AgentStatusState::statuses` Grows Unbounded for Long-Running Panes (`agent_status.rs`)

- **Severity**: Medium
- **File**: `frontend/src/stores/agent_status.rs`
- **Line**: `update_status()` (around line ~90)
- **Category**: Memory / Logic
- **Description**: `AgentStatusState::statuses` is a `Vec<(String, AgentStatus)>`. New entries are added when `update_status` is called with a new pane ID, but there is no automatic cleanup. If a pane is closed, the component must call `remove_status`, but if it doesn't (or if the component crashes before cleanup), the entry remains. In a long session with many pane open/close cycles, the Vec grows.
- **Impact**: Memory leak proportional to total unique pane IDs ever created.
- **Suggested Fix**: Add a periodic cleanup that removes entries with `last_updated_at` older than some threshold, or ensure `remove_status` is called from the component unmount or terminal kill effect.

---

## Low Issues

### L1: `use_signal(|| false)` for Window Maximize State Not Synced (`lib.rs`)
- **Already covered as M1**.

### L2: No Parameter Validation in Tauri Bridge Wrappers (`tauri_bridge.rs`)
- **Severity**: Low
- **File**: `frontend/src/tauri_bridge.rs`
- **Line**: Throughout
- **Description**: There is no input validation or sanitization on parameters passed to Tauri commands. While this is a desktop app (not a web app), malicious or malformed data could still cause issues if the backend blindly trusts the frontend. For example, `fs_read_file` takes a raw path string.
- **Impact**: Potential path traversal if backend does not validate paths.
- **Suggested Fix**: Add path validation on the frontend (e.g., ensure no `..` sequences) as a defense-in-depth layer, but primarily rely on backend validation.

### L3: `pty_default_shell_cached()` Does Not Handle Cache Race (`tauri_bridge.rs`)
- **Severity**: Low
- **File**: `frontend/src/tauri_bridge.rs`
- **Line**: `pty_default_shell_cached()` (around line ~320)
- **Description**: The `DEFAULT_SHELL_CACHE` is a `OnceLock<String>`. If two components call `pty_default_shell_cached()` concurrently, both might see the cache as empty, both will call `pty_default_shell()` and then `set`. Only one will succeed in setting. This is benign (idempotent) but wasteful.
- **Impact**: Minor: duplicate async IPC calls.
- **Suggested Fix**: Use a `tokio::sync::OnceCell` or simple atomic flag to prevent duplicate concurrent fetches.

### L4: `TerminalStore::ensure_session` Does Not Check for ID Collisions Properly (`terminal.rs`)
- **Severity**: Low
- **File**: `frontend/src/stores/terminal.rs`
- **Line**: `ensure_session()` (around line ~40)
- **Description**: If `ensure_session` is called with the same ID but different `cols`/`rows`, it returns `false` (session exists) and does not update dimensions. The caller might expect the session to be resized/updated.
- **Impact**: Session dimensions could be stale if re-ensured with different sizes.
- **Suggested Fix**: If the session exists but dimensions differ, update the stored dimensions.

### L5: `WorkspaceState::save()` Spawns Fire-and-Forget Tasks Without Cancellation (`workspace.rs`)
- **Severity**: Low
- **File**: `frontend/src/stores/workspace.rs`
- **Line**: `save()` method
- **Description**: Each call to `save()` spawns a new `wasm_bindgen_futures::spawn_local`. On rapid mutation, many overlapping tasks accumulate. While the actual IPC might queue them, the closure captures clone the full JSON string each time, allocating memory.
- **Impact**: Memory pressure and potential for out-of-order writes.
- **Suggested Fix**: As noted in C2, implement a single pending save with debouncing.

### L6: `Theme` and `Font` Applied Twice on Mount (`lib.rs`)
- **Severity**: Low
- **File**: `frontend/src/lib.rs`
- **Line**: Two separate `use_effect` blocks for theme/font loading
- **Description**: There are two `use_effect` blocks that both apply theme and font settings. The first loads from persist (async), the second applies current local settings (sync). On mount, the sync effect applies the defaults, then the async effect overwrites with persisted values. This causes a flash of unstyled content (FOUC) and two DOM mutations.
- **Impact**: Minor visual flicker on app startup.
- **Suggested Fix**: Consolidate into a single effect that loads persisted values and falls back to defaults.

---

## Additional Observations (Not Strictly Bugs)

### Observation 1: `JsValueCast for ()` Ignores Return Value (`tauri_bridge.rs`)
- The `from_js_value` for `()` simply returns `Ok(())` without validating the JsValue is indeed undefined/null. If a command unexpectedly returns a value, it's silently dropped. This is fine for `()` but could mask backend contract changes.

### Observation 2: `AthenaState` Messages Field Not Optimized for Rendering (`athena.rs`)
- Rendering a chat typically only needs the last N messages. Keeping all messages in a single `Vec` means any component that reads messages (e.g., a scrollback view) might re-render the entire list on every new message. Consider virtualizing the message list or using a more fine-grained signal structure.

### Observation 3: `console_error_panic_hook` is Set In `main.rs`
- This is good practice for WASM apps (captures panics and sends them to the console), but there is no custom panic hook to report to the backend or show a user-friendly error screen.

### Observation 4: `web_sys::console` Used Directly Throughout
- Many files call `web_sys::console::error_1()` directly. Consider a centralized logging helper that can also report to the backend (e.g., via a Tauri command) for production debugging.

---

## Architecture Notes

### State Management Pattern
- All stores use monolithic state structs (e.g., `AthenaState`, `WorkspaceState`) wrapped in a single `Signal`. This means any mutation to any field triggers a re-render of all subscribers. In Dioxus 0.7, `Signal<T>` where `T` is a large struct, subscribers will re-render on any field change.
- **Mitigation**: The `PartialEq` derived on the structs does help (Dioxus only notifies subscribers if `T` changes), but as noted in H3, the derived `PartialEq` is expensive for large structs.
- **Recommendation**: Consider splitting into smaller signals or using `Memo`/`use_memo` to create derived read-only signals for expensive computations.

### Tauri Bridge
- The `tauri_bridge.rs` module is a hand-written wrapper over `window.__TAURI__` JS API. This is necessary for WASM builds but introduces a lot of unsafe/unchecked JS interop. The error handling is decent (using `TauriBridgeError`) but the `Closure.forget()` in `pty_listen_binary` is a critical leak.

### Component Lifecycle
- The `App` component is quite large (over 1,000 lines of RSX and logic). It handles global keybindings, theme loading, workspace restoration, and renders the main layout. This makes it hard to test and reason about. The keyboard handler alone is ~200 lines.
- **Recommendation**: Decompose `App` into smaller components: `AppLayout`, `GlobalKeybindings`, `ThemeLoader`, `WorkspaceLoader`.

---

## Summary of Recommended Actions

| Priority | Action | Files |
|----------|--------|-------|
| **Critical** | Fix `Closure.forget()` leak in `pty_listen_binary` | `tauri_bridge.rs` |
| **Critical** | Add save debouncing/serialization queue to `WorkspaceState` | `workspace.rs` |
| **Critical** | Implement buffer/pane cleanup in `AgentOutputState` | `agent_output.rs` |
| **High** | Ensure all `listen()` calls have cleanup/unlisten | All component files |
| **High** | Optimize `AthenaState` `PartialEq` or restructure signals | `athena.rs` |
| **High** | Consolidate `UIState.panel` and `PanelManagerState` | `ui.rs`, `panel_manager.rs`, `lib.rs` |
| **High** | Clean up `mounted_spaces` on space removal | `lib.rs` |
| **Medium** | Sync `is_maximized` with actual window state | `lib.rs` |
| **Medium** | Add per-buffer output capping | `terminal_blocks.rs` |
| **Medium** | Handle `save()` errors and retry | `workspace.rs` |
| **Medium** | Fix `TerminalStore::kill()` session removal logic | `terminal.rs` |
| **Medium** | Use `VecDeque` for Athena messages | `athena.rs` |
| **Low** | Consolidate duplicate theme/font effects | `lib.rs` |
| **Low** | Add deduplication to notifications | `notification.rs` |
| **Low** | Persist command palette recent list | `command.rs` |

---

*Audit completed.*
