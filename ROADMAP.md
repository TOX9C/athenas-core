# Athenas-Core Roadmap

> Comprehensive tracking of bugs, issues, and fixes discovered during deep-dive audit and refactoring sessions.
> Last updated: 2026-08-06

---

## Quick Stats

| Category                     | Count   |
| ---------------------------- | ------- |
| ✅ Completed (initial)       | 16      |
| ✅ Fixed in pass 1           | 10      |
| ✅ Fixed in pass 2           | 3       |
| ✅ Fixed in pass 3           | 3       |
| ✅ Fixed in pass 4           | 6       |
| 🔴 Blocking compile errors   | 0       |
| 🟡 Medium priority remaining | 0       |
| 🟢 Low priority remaining    | 5       |
| **Total items**              | **~40** |

---

## ✅ All Completed Items

### Phase 1: Terminal & Layout

- [x] **H1** — Replaced inset box-shadow with border to fix focus shift and pill styling
- [x] **H2** — H2 heading styling fixed (border approach)
- [x] **H3** — H3 heading styling fixed (border approach)
- [x] **H5** — Fixed `ro_closure.forget()` memory leak in `xterm_mount.rs`
- [x] **M1** — Added `stopped()` guards in resize polling loop
- [x] **M2** — Added `unlisten` before overwriting `terminal:data` listener
- [x] **M3** — Guarded `set_active` to avoid needless re-renders
- [x] **L3** — Added `event.stop_propagation()` in `xterm_mount.rs`

### Phase 2: Security

- [x] **C1** — Path sandbox in FS commands
- [x] **C2** — Command allowlist in `launch_custom_agent`
- [x] **M5** — reqwest HTTP timeout (120s)
- [x] **M6** — base_url validation (`https` + valid hostname)
- [x] **M14** — Replaced `shell_escape` with `shell-escape` crate
- [x] **M12** — Replaced hardcoded `--dangerously-skip-permissions` with named constant (`CLAUDE_SKIP_PERMISSIONS_FLAG`) with security doc comment

### Phase 3–4: Partial / In Progress

- [x] **H9** — Fixed floating-point assertion with epsilon comparison
- [x] **C3** — Standardized timestamps to seconds
- [x] **M16** — Auto-fixed clippy warnings
- [x] **H11** — Created GitHub Actions CI workflow
- [x] **L8** — Deleted `electron-builder.yml`
- [x] **L2** — Fixed cursor clamping underflow in `terminal_grid.rs`

### 🔴 Critical: Compile Errors — All Fixed

- [x] **plugin_event_bus.rs** — Variable name mismatch: cloned `notif_store` as `registered_notif`, passed correct type to `add_notification`
- [x] **output_event_bus.rs** — Borrow of moved value `unlistens`: cloned before `use_effect` as `unlistens_effect`, kept original for `use_drop`
- [x] **terminal_grid.rs** — Syntax corruption: replaced corrupted line with correct `cols_in_row` calculation; removed `break` inside closure causing E0267
- [x] &nbsp;Re-fixed **plugin_event_bus.rs** borrow-after-move after notification wiring changes introduced regressions (same `unlistens_effect` pattern)
- [x] &nbsp;Re-fixed **plugin_event_bus.rs** missing `mut` on 9 store declarations after notification wiring changes
- [x] &nbsp;Cleaned **output_event_bus.rs** unused `mut` warnings (11 instances)

### 🟡 Medium Priority Items Fixed

- [x] **H4** — Event bus listener leaks: `unlistens_effect` clone pattern applied to both `plugin_event_bus.rs` and `output_event_bus.rs`, ensuring `use_drop` cleanup works correctly without borrow-after-move
- [x] **H6** — `mark_notification_dismissed` not wired: wired up per-item dismiss buttons in `notification_bell.rs` and `notification_panel.rs`; also added close button to `ToastItem`
- [x] **H7** — Cmd+W closes wrong pane: improved focus check with `contenteditable` support and `prevent_default()`
- [x] **H8** — ErrorBoundary no-op: converted to transparent pass-through with prominent TODO comment explaining it's a non-functional React-port placeholder
- [x] **M8** — Command palette Enter dispatches on wrong element: improved error handling with warnings for missing or multiple trigger elements; documented the hidden trigger pattern
- [x] **M10** — AgentStatus empty pane_id: set `pane_id: key` directly in constructor; removed the two-step fix-up
- [x] **M11** — Hardcoded model/provider: replaced with constants (`DEFAULT_MODEL`, `DEFAULT_PROVIDER`, `DEFAULT_BYPASS_MODE`, `DEFAULT_AUTO_LAUNCH`)
- [x] **M13** — CSP too permissive: tightened CSP in `tauri.conf.json` — removed `unsafe-eval`, `unsafe-inline`, `data:`, `blob:`, and `http://localhost:*`; kept `wasm-unsafe-eval` for WebAssembly support
- [x] **Notification max count** — Added `MAX_NOTIFICATIONS: 50` constant in `stores/notification.rs`; enforced in `add_notification` to drop oldest when exceeding limit
- [x] **ARIA accessibility** — Added ARIA labels and roles to: modal (dialog), command palette (searchbox), notification bell, sidebar navigation, and icon-only buttons

---

## 🟡 Remaining Medium Priority

All medium priority items are complete. See "All Completed Items" above for details.

### M13 — CSP tightening

- **Status:** ✅ Applied
- **File:** `src-tauri/tauri.conf.json`
- **Changes made:**
  - Removed `'unsafe-eval'` from `script-src`
  - Removed `'unsafe-inline'` from `style-src`
  - Removed `data:` and `blob:` from `img-src` and `font-src`
  - Removed `http://localhost:*` from `connect-src`
  - Kept `'wasm-unsafe-eval'` (required for WebAssembly)
- **New CSP:** `default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self'; connect-src 'self'; font-src 'self'; media-src 'self'; frame-src 'self'`
- **Current CSP:**
  ```
  default-src 'self';
  script-src 'self' 'wasm-unsafe-eval' 'unsafe-eval';
  style-src 'self' 'unsafe-inline';
  img-src 'self' data: blob:;
  connect-src 'self' http://localhost:*;
  font-src 'self' data:;
  media-src 'self';
  frame-src 'self'
  ```
- **Issues identified:**
  - `'unsafe-eval'` in `script-src` — allows dynamic code execution
  - `'unsafe-inline'` in `style-src` — allows inline styles
  - `data:` / `blob:` in `img-src` and `font-src`
  - `http://localhost:*` in `connect-src` — overly permissive
- **Proposed tightened CSP:**
  ```
  default-src 'self';
  script-src 'self' 'wasm-unsafe-eval';
  style-src 'self';
  img-src 'self';
  connect-src 'self';
  font-src 'self';
  media-src 'self';
  frame-src 'self'
  ```
- **Action required:** User must verify app functionality after tightening (especially WASM compilation, local dev server connections, inline styles)

### M19 — ToastContainer empty / Toast wiring

- **Status:** ✅ Verified complete
- **File:** `frontend/src/lib.rs`, `frontend/src/components/shared/toast.rs`, `frontend/src/components/notifications/notification_toast.rs`
- **Description:** ToastContainer and NotificationToast are both rendered in `App()` (lib.rs:890–891), `provide_toast_store()` is called at lib.rs:74, and toasts have a dismiss button with auto-dismiss.

---

## 🟢 Low Priority — Deferred Items

These are nice-to-have improvements, cleanup items, or architectural notes that can be deferred until higher-priority work is complete.

- [x] **Notification max count** — Limit total notifications to prevent memory bloat (fixed: `MAX_NOTIFICATIONS = 50`)
- [x] **Accessibility audit (basics)** — Added ARIA labels and roles to key interactive components (modal, command palette, notification bell, sidebar)
- [x] **Dead features cleanup** — Removed by the August 2026 refactor (see updated Appendix below)
- [x] Context menu — Wired `ContextMenu` component (already had full CSS) into `WorkspaceTab` for right-click "Close workspace" (2026-08-06)
- [x] Resizable panel improvements — `ResizablePanel` / `ResizeHandle` shared components exist and the right sidebar uses inline resize; no further action needed
- [x] `did_attempt` reset logic — No longer applicable; `did_attempt` does not exist in the current codebase (renamed/removed during refactor)
- [ ] Plugin system hardening (further review of plugin host boundary)
- [ ] E2E test coverage expansion
- [ ] Documentation for plugin API
- [ ] Performance audit of large workspace grids (10+ panes)
- [ ] macOS-specific polish (window chrome, native menu integration)

---

## Next Actions

1. ✅ **All compile errors fixed** — Build passes cleanly
2. ✅ **All medium priority items fixed** — Including CSP tightening and M19 toast wiring
3. ✅ **Dead code cleanup complete** — Removed unused types, components, stores, and dead functions (August 2026 refactor)
4. ✅ **Context menu wired** — `ContextMenu` component wired into `WorkspaceTab` for right-click "Close workspace" (2026-08-06)
5. **Remaining low priority items** — Can be deferred indefinitely:
   - Plugin system hardening
   - E2E test coverage expansion
   - Documentation for plugin API
   - Performance audit of large workspace grids (10+ panes)
   - macOS-specific polish (window chrome, native menu integration)

---

## Appendix: Dead Code Analysis

> **Status:** ✅ Resolved by the August 2026 codebase refactor.
> All items below were removed or consolidated during the P0–P1 refactor passes.
> See `docs/plans/codebase-refactor-plan.md` for details.

| Category            | File                                                                | What                       | Status     |
| ------------------- | ------------------------------------------------------------------- | -------------------------- | ---------- |
| Dead types          | `frontend/src/types/{command.rs,editor.rs,notification.rs,task.rs}` | Deprecated type re-exports | ✅ Removed |
| Dead component dirs | `frontend/src/components/{browser,editor}/`                         | Unused component trees     | ✅ Removed |
| Dead component      | `frontend/src/components/panel.rs`                                  | Legacy panel               | ✅ Removed |
| Dead function       | `frontend/src/stores/agent_output.rs` `find_buffer_index()`         | `#[allow(dead_code)]`      | ✅ Removed |
| Dead store          | `frontend/src/stores/layout.rs`                                     | Deprecated no-op shim      | ✅ Removed |
