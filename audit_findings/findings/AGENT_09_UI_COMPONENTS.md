# UI Components Audit Findings

## Summary
- **Files Analyzed:** 74 Rust component files (Dioxus 0.7 frontend)
- **Total Findings:** 46 issues across 9 severity levels
- **Categories:** Memory leaks, event listener leaks, rendering bugs, performance issues, stale state, accessibility, lifecycle bugs

---

## 🔴 Critical

### C-1: NotificationBell Clones Effect Store Captures, Causing Listener Duplication
**File:** `frontend/src/components/notifications/notification_bell.rs`  
**Line:** 18-92 (use_effect block)  
**Category:** Memory Leak / Event Listener Leak  
**Description:** Inside `use_effect`, the same `notifications` signal is moved into multiple event listeners via `.clone()`, and each listener closure captures a _different_ cloned signal. On re-render, the effect runs again creating duplicate `tauri_bridge::listen()` registrations. The `mounted` guard prevents re-execution, but the initial mount's `let mut new_store = notifications` creates aliasing of the same signal across closures. More critically, if the component unmounts and remounts, the listeners are not cleaned up (no unlisten stored), causing unbounded listener accumulation.

**Impact:** Each mount leaks Tauri event listeners. After many mount/unmount cycles, the Tauri event bus becomes saturated and the app slows down/crashes.

**Suggested Fix:** Store unlisten handles in a `Rc<RefCell<Vec<Box<dyn FnOnce()>>>>` like `PluginEventBus` does, and call them in `use_drop`. Only register listeners once per mount using the `mounted` guard, but also properly clean up on unmount.

### C-2: Every Event Bus Component Missing Proper Cleanup on Unmount
**Files:**
- `frontend/src/components/notifications/notification_bell.rs`
- `frontend/src/components/notifications/notification_toast.rs`
- `frontend/src/components/plugin/plugin_event_bus.rs`
- `frontend/src/components/agents/output_event_bus.rs`

**Category:** Memory Leak / Event Listener Leak  
**Description:** `notification_bell.rs` and `notification_toast.rs` register Tauri event listeners inside `use_effect` without ever storing or calling the unlisten function. The `tauri_bridge::listen()` returns an `unlisten` function that must be called to unregister. While `plugin_event_bus.rs` and `output_event_bus.rs` DO use `use_drop` for cleanup, `notification_bell.rs` and `notification_toast.rs` do not. Additionally, even in components with cleanup, the `use_effect` runs once with `mounted` guard, but if the component unmounts and remounts without the app being reloaded, the cleanup from the previous mount should run, but the new mount creates fresh listeners.

**Impact:** Unbounded growth of Tauri event listeners, memory and performance degradation.

**Suggested Fix:** For `notification_bell.rs` and `notification_toast.rs`, store unlisten handles and implement `use_drop` cleanup matching the pattern in `plugin_event_bus.rs`.

### C-3: FileTree Creates Duplicate fs:change:* Listeners on Every Render
**File:** `frontend/src/components/sidebar_dir/file_tree.rs`  
**Line:** 130-145 (second `use_effect` block labeled "Subscribe to fs:change:*")  
**Category:** Memory Leak / Event Listener Leak  
**Description:** The `tauri_bridge::listen("fs:change:*", ...)` call is wrapped in a `{}` scope (line 130) and does NOT store the returned unlisten handle. Even more critically, because this block creates a new listener on every render where `active_dir` changes, and there's no `mounted` guard:

```rust
{
    let dir_for_listen = active_dir.clone();
    let mut nodes_for_listen = nodes;
    let mut loading_for_listen = loading;
    let _ = tauri_bridge::listen("fs:change:*", move |_payload: String| {
        // ...
    });
}
```

Every time `use_effect` re-runs (which happens when `active_dir` changes), a NEW listener is registered. The old listeners are never removed.

**Impact:** Unbounded growth of file system change listeners. Each re-render of FileTree adds another listener that triggers on every fs change event.

**Suggested Fix:** Move the `fs:change:*` listener registration outside the effect that depends on `active_dir`, or store the unlisten handle and call it before registering a new one. Better: register the listener once per mount using a `mounted` guard, and handle changes reactively rather than re-registering.

---

## 🟠 High

### H-1: use_effect Dependencies Not Declared, Causing Stale Closure Captures
**Files:** Multiple files  
**Category:** Stale State / Rendering Bug  
**Description:** Dioxus 0.7 `use_effect` does NOT automatically track dependencies like React's useEffect. It runs ONCE by default. Many effects in this codebase rely on signals being read inside the effect closure for automatic reactivity, but some closures capture values at effect-run time. Examples:

- `frontend/src/components/athena/athena_panel.rs` line ~325: The session loading `use_effect` captures `athena_state` without proper signal reactivity - while it uses `spawn` which does keep the signal handle alive, the initial values read could be stale.
- `frontend/src/components/settings/settings_modal.rs` line ~110: `AthenaSettings` `use_effect` loads store values once at mount, but if the store changes externally, the UI won't reflect it.
- `frontend/src/components/workspace/xterm_mount.rs` line ~43: The `use_effect` captures `is_initialized` signal but doesn't observe its changes for the theme effect.

**Impact:** State can become desynchronized. UI shows stale values. Effects that should re-run when dependencies change don't.

**Suggested Fix:** Review all `use_effect` usages to ensure proper reactive patterns. In Dioxus, preferably read signals inside the effect closure (which auto-subscribes to changes). For effects that need to run when specific signals change, use `use_memo` or read the reactive values inside spawn blocks. For the xterm theme effect (line ~328), this is done correctly by reading `ui_state.read().theme` inside the effect - but many other effects lack this pattern.

### H-2: AgentInspector Captures signals Mutably in Render Path Without Proper Memoization
**File:** `frontend/src/components/agents/agent_inspector.rs`  
**Lines:** 175-178, 181-183  
**Category:** Performance / Rendering Bug  
**Description:** On every render, the component:
1. Reads `agent_status.read().statuses` to find the active pane status
2. Filters all notifications
3. Converts them to `NotificationItem`

The `filtered_notifications` is a plain `Vec<NotificationItem>` computed on every render without `use_memo`. For large notification counts, this is expensive.

**Impact:** Unnecessary recomputation on every render. Scrolling, typing, or parent re-renders all trigger this expensive filter+map operation.

**Suggested Fix:** Wrap the filtered computation in `use_memo`:
```rust
let filtered_notifications = use_memo(move || {
    let query = search_query();
    notifications.read().iter().filter(...).map(...).collect::<Vec<_>>()
});
```

### H-3: PaneItem's `spawn` for PTY Kill is Not Awaited, Fire-and-Forget
**File:** `frontend/src/components/workspace/terminal_grid.rs`  
**Lines:** 249-254  
**Category:** Async Resource Leak  
**Description:** When a pane is closed, the code spawns a future to call `pty_kill` but doesn't track or await it:

```rust
spawn({
    let pane_id = pane_id_for_close.clone();
    async move {
        let _ = pty_kill(&pane_id).await;
    }
});
```

If `pty_kill` hangs (e.g., backend deadlocked), this future leaks in the executor. Thousands of hanging futures could accumulate over time.

**Impact:** Resource exhaustion in the WASM executor. Potential denial of service by repeatedly opening/closing panes.

**Suggested Fix:** Add a timeout to the `pty_kill` call, or at minimum add an `AbortHandle` and cancel the future if the component unmounts before the kill completes.

### H-4: AthenaInput's `submit_message_async` Captures Entire `terminal_blocks_store` Every Render
**File:** `frontend/src/components/athena/athena_input.rs`  
**Lines:** 30-200 (entire submit flow)  
**Category:** Performance  
**Description:** Each call to `submit_message` captures the `terminal_blocks_store`, clones ALL blocks into a `Vec<TerminalBlock>`, and moves them into an async closure. For large terminal histories with many blocks, this is a heavy allocation per message.

**Impact:** Memory pressure and allocation spikes when sending messages with large terminal history.

**Suggested Fix:** Instead of capturing all blocks, only capture a summary or reference. Or use `use_memo` in the parent to pre-compute the terminal fragment, and pass a reference to `submit_message`.

---

## 🟡 Medium

### M-1: ToastContainer Never Removes Expired Toasts
**File:** `frontend/src/components/shared/toast.rs`  
**Lines:** 111-146  
**Category:** Memory Leak  
**Description:** `ToastItem` receives a Toast with a `duration_ms`, but there's no timeout or automatic removal. The toast stays in the `ToastState` forever unless manually removed by clicking X.

**Impact:** Memory growth as toasts accumulate. UI can eventually have hundreds of stale toast DOM nodes.

**Suggested Fix:** Implement a `use_effect` in `ToastItem` that starts a timer on mount, and after `duration_ms`, calls `toast_store.write().remove(&toast_id)`. Or better, use a declarative approach where the store manages a timeout per toast.

### M-2: AgentOutputLine Computes `is_stderr_like` on Every Render with String Allocation
**File:** `frontend/src/components/agents/agent_output_line.rs`  
**Lines:** 42-49  
**Category:** Performance  
**Description:** The `is_stderr_like` function allocates a lowercase string on EVERY render:

```rust
fn is_stderr_like(text: &str) -> bool {
    let lower = text.to_lowercase(); // ALLOCATES every render
    lower.contains("error") || ...
}
```

This runs once per output line per render. With 1000 lines, that's 1000 small string allocations per render.

**Impact:** Unnecessary allocation pressure. Can cause GC pauses in WASM.

**Suggested Fix:** Pre-compute `is_stderr` when creating the `OutputLine`, or use a case-insensitive match without allocating. Also, the `is_stderr_like` logic is redundant since `OutputLine` already has `is_stderr` field.

### M-3: WorkspaceTabs/search Eagerly Clones Entire Space List on Every Render
**File:** `frontend/src/components/workspace/workspace_tabs.rs`  
**Lines:** 10-30  
**Category:** Clone Overhead  
**Description:**

```rust
let spaces: Vec<Space> = workspace_state.read().spaces.clone();
let active_space_id = workspace_state.read().active_space_id.clone();
```

This clones the entire spaces Vec on every render. With many spaces and panes, this is expensive.

**Suggested Fix:** Use an iterator-based approach, or read the store only when needed. The `spaces` clone is unnecessary since it's only used to iterate.

### M-4: RightBrowserPanel Clones iframe_src and actual on Every Click
**File:** `frontend/src/components/right_sidebar/browser_panel.rs`  
**Lines:** 53-95 (all button onclicks)  
**Category:** Performance / Memory  
**Description:** Every button click handler clones `iframe_src` and `actual` signals:

```rust
onclick: move |_| {
    let mut iframe_clone = iframe_src.clone();
    let mut actual_clone = actual.clone();
    wasm_bindgen_futures::spawn_local(async move { ... });
}
```

The signals are cheap to clone (Rc<RefCell<_>>), but the pattern of creating local clones in every handler is verbose and error-prone. More importantly, `iframe_src` and `actual` are `use_signal`, which holds reactive state. Creating multiple clones per click doesn't cause leaks but is unnecessary overhead.

**Suggested Fix:** Move the signal writes inside a function that takes the signals by reference, or simply use `iframe_src.set(...)` directly in the async block since `use_signal` is `Clone` and thread-safe for single-threaded WASM.

### M-5: SettingsModal's `GeneralSettings` Reads `ui_state` Extensively in Render Loop Without Memoization
**File:** `frontend/src/components/settings/settings_modal.rs`  
**Lines:** 140-190 (Font Family and Font Size tabs)  
**Category:** Performance  
**Description:** The font rendering loops over `AVAILABLE_FONTS` and reads `ui_state.read()` multiple times per font:

```rust
let is_selected = *font == ui_state.read().font_family;
let current_theme = get_theme(ui_state.read().theme.name());
```

This reads the store twice per font, and with few fonts it's fine, but this pattern (read-in-render-loop) should be consolidated.

**Suggested Fix:** Read the store once outside the loop:
```rust
let ui = ui_state.read();
let current_font = &ui.font_family;
let current_theme = get_theme(ui.theme.name());
// then use in loop
```

### M-6: PluginEventBus and OutputEventBus Have Race Conditions in Signal Writes
**Files:**
- `frontend/src/components/plugin/plugin_event_bus.rs` (lines 53, 66, etc.)
- `frontend/src/components/agents/output_event_bus.rs` (multiple places)

**Category:** Race Condition / WASM Panic Risk  
**Description:** These components access multiple stores from within event callbacks. In Dioxus, writing to signals from event callbacks is generally safe for single-writer, but there are patterns like:

```rust
let mut registry_store = plugin_store;
let registry_unlistens = unlistens_effect.clone();
if let Ok(u) = tauri_bridge::listen("plugin:registryUpdated", move |payload: String| {
    if let Ok(val) = ... {
        registry_store.write().upsert_plugin(...);
    }
}) {
```

The `registry_store` signal is moved into the closure. While this works in Dioxus, writing to a signal while another piece of code holds a read lock on it can panic. Additionally, multiple event handlers writing to the same store concurrently could theoretically cause issues.

**Impact:** Potential `RuntimeError: Unreachable code should not be executed` in WASM (matches known Dioxus 0.7 issue mentioned in CLAUDE.md). Could cause intermittent crashes during heavy IPC.

**Suggested Fix:** Use `use_coroutine` or a single dispatch-based approach. Ensure all signal writes are done from within Dioxus's reactive system rather than from external callbacks. Consider batching updates.

### M-7: AthenaPanel's Nested use_effect Calls Cause Signal Read During Write on Session Restore
**File:** `frontend/src/components/athena/athena_panel.rs`  
**Lines:** 325-380  
**Category:** WASM Panic Risk  
**Description:** During session restoration, the code does:

```rust
athena.write().set_messages(loaded);
athena.write().set_session_id(Some(id.to_string()));
let title = ...;
athena.write().set_session_title(title);
```

Multiple rapid signal writes can trigger Dioxus's reactive system to attempt reads while a write is in progress, especially if any of these setters trigger other reactive computations.

**Impact:** Known to cause `RuntimeError: Unreachable code should not be executed` in WASM environments, per the project documentation.

**Suggested Fix:** Use a single batched write, or wrap the updates in a closure that collects all changes atomically.

### M-8: agent_output_panel.rs Clones ALL Store Lines on Every Render
**File:** `frontend/src/components/agents/agent_output_panel.rs`  
**Lines:** 20-30  
**Category:** Performance  
**Description:**

```rust
let store_lines: Vec<StoreLine> = selected_id
    .as_ref()
    .and_then(|id| {
        agent_output
            .read()
            .buffers
            .iter()
            .find(|(pid, _)| pid == id)
            .map(|(_, l)| l.clone()) // clones entire Vec!
    })
    .unwrap_or_default();
```

This clones the entire output buffer for the selected pane on every render. For large outputs (1000s of lines), this is very expensive.

**Suggested Fix:** Use an reference-counted buffer (Rc<Vec<OutputLine>>) in the store, or iterate directly over the store data without cloning.

### M-9: Button Component Does Not Handle `disabled` Style Override
**File:** `frontend/src/components/shared/button.rs`  
**Lines:** 45-48  
**Category:** Accessibility / UX  
**Description:** The `Button` component has a `disabled` prop and uses it with `disabled: props.disabled`, but the style string sets `opacity: {opacity}` where `opacity = if props.disabled { "0.5" } else { "1" }`. However, the `cursor` is always `pointer` regardless of disabled state.

**Impact:** Users see `pointer` cursor on disabled buttons, which is confusing and non-standard.

**Suggested Fix:** Change cursor to `not-allowed` when disabled, matching standard web behavior.

### M-10: Tooltip Component is Just a Title Attribute Placeholder
**File:** `frontend/src/components/shared/tooltip.rs`  
**Lines:** 10-25  
**Category:** Accessibility  
**Description:** The `Tooltip` component currently does:

```rust
div {
    title: "{props.text}",
    {props.children}
}
```

This uses `title` attribute, which is not accessible for keyboard users and has inconsistent cross-browser behavior. It also doesn't work with complex content.

**Impact:** Poor accessibility. Tooltips don't show for keyboard navigation or touch devices.

**Suggested Fix:** Implement a proper accessible tooltip with `role="tooltip"`, `aria-describedby`, keyboard navigation, and correct positioning.

### M-11: Modal Does Not Trap Focus
**File:** `frontend/src/components/shared/modal.rs`  
**Lines:** 15-50  
**Category:** Accessibility  
**Description:** The `Modal` component has `aria-modal="true"` and `role="dialog"`, but does not implement focus trapping. When the modal is open, keyboard users can tab out of it and interact with the behind elements.

**Impact:** Critical accessibility violation (WCAG). Keyboard users can accidentally interact with background UI while the modal is open.

**Suggested Fix:** Implement focus trap using `focus` event listeners on the modal container, or use a `focus-trap` library equivalent. Manage `aria-hidden="true"` on the app container when a modal is open.

### M-12: Settings Modal's API Key Stored in Plain Text in Signal
**File:** `frontend/src/components/settings/settings_modal.rs`  
**Lines:** 105-155 (`AthenaSettings` tab)  
**Category:** Security  
**Description:** API key is stored in a plain `use_signal(String)` and read with `api_key.read()` which can be observed by any component. The `input type="password"` only obscures the visual display, not the memory.

**Impact:** API key is accessible in memory to any component with access to the signal or through browser dev tools.

**Suggested Fix:** Store sensitive values in a secure store, or at minimum, clear the signal after saving to persistent storage. Do NOT keep API keys in reactive signals.

---

## 🟢 Low

### L-1: Multiple Components Use Inline Styles Extensively
**Files:** Nearly all component files  
**Category:** Maintainability / Performance  
**Description:** Almost all components use inline `style` attributes instead of CSS classes. This makes the code harder to maintain, prevents CSS deduplication, and can cause performance issues with large numbers of DOM nodes.

**Impact:** Code bloat, harder to theme, slight performance overhead from inline style parsing.

**Suggested Fix:** Gradually migrate to CSS classes defined in external stylesheets or use a CSS-in-Rust solution.

### L-2: ErrorBoundary is Just a Placeholder
**File:** `frontend/src/components/shared/error_boundary.rs`  
**Category:** Error Handling  
**Description:** The ErrorBoundary component is a no-op pass-through:

```rust
pub fn ErrorBoundary(props: ErrorBoundaryProps) -> Element {
    rsx! { {props.children} }
}
```

It doesn't catch or handle errors. The comment acknowledges this but it's a known gap.

**Impact:** Application crashes from any child component will take down the entire app.

**Suggested Fix:** Implement proper error handling using Dioxus's error boundary equivalent, or remove and handle errors at a higher level.

### L-3: ResizablePanel is Not Actually Resizable
**File:** `frontend/src/components/shared/resizable_panel.rs`  
**Category:** Feature Gap  
**Description:** The component title says "Simple resizable panel wrapper" with a TODO, but it just renders a div with `flex: 1`:

```rust
#[component]
pub fn ResizablePanel(props: ResizablePanelProps) -> Element {
    rsx! {
        div {
            style: "flex: 1; min-width: 0; min-height: 0; overflow: hidden;",
            {props.children}
        }
    }
}
```

**Impact:** Confusing for users of this component who expect resizable behavior.

**Suggested Fix:** Either implement actual resize functionality or rename the component to avoid confusion.

### L-4: ContextMenu is a Pass-Through
**File:** `frontend/src/components/shared/context_menu.rs`  
**Category:** Feature Gap  
**Description:** Same as ResizablePanel - just renders children without any context menu behavior.

**Suggested Fix:** Implement or remove.

### L-5: SwarmLauncher's "Launch Swarm" Button is No-Op
**File:** `frontend/src/components/swarm/swarm_launcher.rs`  
**Lines:** 13-16  
**Category:** Feature Gap  
**Description:**

```rust
onclick: move |_| {
    // TODO: open SwarmModal
}
```

**Suggested Fix:** Wire up to the actual swarm launch flow.

### L-6: Multiple TODO Comments in Production Code
**Files:** Multiple  
**Category:** Maintainability  
**Description:** TODOs found in:
- ` swarm_launcher.rs` - TODO: open SwarmModal
- ` plugin/plugin_dashboard.rs` - TODO: refresh plugins via Tauri IPC
- ` plugin/plugin_card.rs` - TODO: toggle plugin via Tauri IPC
- ` kanban/kanban_column.rs` - TODO: add task via store
- ` kanban/kanban_card.rs` - TODO: delete task via store
- ` athena/ask_user_block.rs` - TODO: respond via Tauri IPC (2x)
- ` settings/settings_modal.rs` - TODO comments in nudge agent handler

**Impact:** Incomplete features in production code.

### L-7: NewSpaceModal Uses Long Inline Style Strings
**File:** `frontend/src/components/workspace/new_space_modal.rs`  
**Category:** Maintainability  
**Description:** The modal button styles are massive inline style strings (200+ chars each), especially for `next_btn_style`, `launch_btn_style`, and `swarm_btn_style`. These are repeated and hard to read/maintain.

**Suggested Fix:** Extract common style patterns to a helper function or use CSS classes.

### L-8: KeyboardEvent Handling in AthenaInput Has Unreachable Last History Path
**File:** `frontend/src/components/athena/athena_input.rs`  
**Lines:** 84-95 (ArrowDown handler)  
**Category:** Logic Bug  
**Description:**

```rust
else if e.key() == Key::ArrowDown {
    let hist = input_history.read();
    if !hist.is_empty() {
        let current = history_idx();
        if let Some(i) = current {
            if i + 1 < hist.len() {
                history_idx.set(Some(i + 1));
                input_text.set(hist[i + 1].clone());
            } else {
                history_idx.set(None);
                input_text.set(String::new());
            }
        }
    }
}
```

When `history_idx` is `None` (cursor at end), pressing ArrowDown does nothing. Also, the `i + 1` increment logic is backwards - ArrowDown should move towards newer history, but this moves towards older history. Actually the logic seems correct for going forward through history (returning to more recent), but when at the end position, it should be possible to go forward to empty/new. Currently `history_idx` only wraps back to None from the last position, which is fine, but there's no handling for when `history_idx` starts at None and user presses ArrowDown - nothing happens, which might be intentional but is a UX gap.

**Suggested Fix:** Make Arrow Down go to the most recent history item when `history_idx` is None, or document this behavior.

### L-9: GridTemplateSelector Creates String Format on Every Render
**File:** `frontend/src/components/workspace/grid_template.rs`  
**Lines:** 27-35  
**Category:** Performance  
**Description:** For each template, it does `format!("{}x{}", cols, rows)` and `format!(...)` for styles on every render.

**Impact:** Minor - only a few allocations per render.

**Suggested Fix:** Precompute label strings since they're static.

### L-10: AgentCard Uses `format!("{:?}", props.agent.role)` for Display
**File:** `frontend/src/components/swarm/agent_card.rs`  
**Lines:** 31-34  
**Category:** Best Practice  
**Description:** Using Debug format for user-facing display is fragile. If the enum variant names change, the UI changes without warning.

**Suggested Fix:** Add a human-readable display method to `AgentRole` and use that instead.

### L-11: ThemePicker's `is_light_bg` Function Has Deeply Nested if-let
**File:** `frontend/src/components/settings/theme_picker.rs`  
**Lines:** 75-93  
**Category:** Readability  
**Description:**

```rust
fn is_light_bg(bg: &str) -> bool {
    if let Some(hex) = bg.strip_prefix('#') {
        if hex.len() == 6 {
            if let Ok(r) = u8::from_str_radix(&hex[0..2], 16) {
                if let Ok(g) = u8::from_str_radix(&hex[2..4], 16) {
                    if let Ok(b) = u8::from_str_radix(&hex[4..6], 16) {
                        let luminance = ...;
                        return luminance > 0.5;
                    }
                }
            }
        }
    }
    false
}
```

**Suggested Fix:** Flatten using `?` operator or early returns, or use a proper color parsing library.

### L-12: session_list.rs fetch_sessions Returns Vec on Error
**File:** `frontend/src/components/athena/session_list.rs`  
**Lines:** 24-30  
**Category:** Error Handling  
**Description:** On parse failure, returns `Vec::new()` without logging or showing any error to the user.

**Suggested Fix:** Log parse errors or return a Result.

### L-13: settings_modal.rs CustomAgentList Clones Entire agents Vec
**File:** `frontend/src/components/settings/settings_modal.rs`  
**Lines:** 310-320  
**Category:** Performance  
**Description:**

```rust
let agents = ui_state.read().custom_agents.clone();
```

**Impact:** Minor for typical numbers of agents.

**Suggested Fix:** Use an iterator, or read only when needed.

---

## Summary Table

| Severity | Count | Main Issues |
|----------|-------|-------------|
| 🔴 Critical | 3 | Event listener leaks (NotificationBell, FileTree), no cleanup |
| 🟠 High | 4 | Stale closures, unmemoized expensive computations, async resource leaks |
| 🟡 Medium | 12 | Toast auto-removal, performance in loops, race conditions, accessibility |
| 🟢 Low | 13 | Placeholders, TODOs, missing features, minor optimizations |

## Files Most at Risk of Changes

1. `frontend/src/components/notifications/notification_bell.rs` - Event listener cleanup (CRITICAL)
2. `frontend/src/components/sidebar_dir/file_tree.rs` - Duplicate listener registration (CRITICAL)
3. `frontend/src/components/workspace/terminal_grid.rs` - PaneItem cleanup and signal usage
4. `frontend/src/components/agents/agent_output_panel.rs` - Buffer cloning
5. `frontend/src/components/athena/athena_panel.rs` - Session restoration signal writes
6. `frontend/src/components/agents/agent_inspector.rs` - Unmemoized filtering
7. `frontend/src/components/athena/athena_input.rs` - Terminal blocks capture
8. `frontend/src/components/shared/toast.rs` - Toast lifetime management
9. `frontend/src/components/plugin/plugin_event_bus.rs` - Signal write safety
10. `frontend/src/components/agents/output_event_bus.rs` - Signal write safety
