# FIX SPEC: Workstream 4 — Frontend Memory Leaks & Event Listeners

## Background
The audit found systemic memory leaks due to unclosed Tauri event listeners and mismatched state.
- AGENT_08 [C3]: `Closure.forget()` in `pty_listen_binary` permanently leaks JS references
- AGENT_09 [C-1/C-2/C-3]: Unbounded event listener growth across multiple components
- AGENT_08 [H1]: `listen()` returning unlisten functions that are never called
- AGENT_08 [C2]: `WorkspaceState` race condition with fire-and-forget saves

## Key Changes
1. **`pty_listen_binary`**: Store `Closure` and return an unlisten function; remove `.forget()`.
2. **Event Listener Cleanup**: Implement `use_drop` in `notification_bell.rs` and `notification_toast.rs` to clean up listeners.
3. **`tauri_bridge.rs`**: Update `listen()` to return `Result` and track handles.
4. **`WorkspaceState`**: Implement a debounced/single-pending save mechanism.

## Files to Modify
- `frontend/src/tauri_bridge.rs`
- `frontend/src/components/notifications/notification_bell.rs`
- `frontend/src/components/notifications/notification_toast.rs`
- `frontend/src/stores/workspace.rs`

## Verification
- Memory profiling: Open/close multiple sessions and verify no heap growth.
- Integration tests for workspace state persistence.
