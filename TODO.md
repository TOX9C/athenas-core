# Workstream 4: Frontend Memory Leaks Implementation TODO

## Phase 1: Core Infrastructure (tauri_bridge.rs)
- [ ] Fix `listen()` to properly store closures and return usable unlisten handles
- [ ] Ensure `pty_listen_raw` properly stores closure without .forget()
- [ ] Add `TauriBridgeError` improvements if needed

## Phase 2: Event Listener Cleanup (Notification Components)
- [ ] Add unlisten-on-unmount to `notification_bell.rs`
- [ ] Add unlisten-on-unmount to `notification_toast.rs`
- [ ] Ensure listeners are properly tracked and cleaned up

## Phase 3: Workspace State Save Fix
- [ ] Implement debounced/single-pending save in `workspace.rs`
- [ ] Add pending save tracking mechanism
-:

## Phase 4: Verification
- [ ] cargo check passes
- [ ] Write tests for event listener cleanup
- [ ] Update progress.md

## Constraints
- Do NOT change public API signatures unless absolutely necessary
- Add tests for event listener cleanup
- Ensure cargo check passes after changes
- Do NOT fix unrelated issues
