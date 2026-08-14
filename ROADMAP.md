# Athenas-Core Roadmap

> Comprehensive tracking of bugs, issues, and fixes discovered during deep-dive audit and refactoring sessions.
> Last updated: 2026-08-10

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
| 🟢 Low priority remaining    | 1       |
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
- [x] **M13** — CSP baseline hardened: removed `unsafe-eval` and wildcard network sources; inline styles and `data:`/`blob:` assets remain explicitly required by the current bundled UI and are release-reviewed allowances
- [x] **Notification max count** — Added `MAX_NOTIFICATIONS: 50` constant in `stores/notification.rs`; enforced in `add_notification` to drop oldest when exceeding limit
- [x] **ARIA accessibility** — Added ARIA labels and roles to: modal (dialog), command palette (searchbox), notification bell, sidebar navigation, and icon-only buttons

---

## 🟡 Remaining Medium Priority

All medium priority items are complete. See "All Completed Items" above for details.

### M13 — CSP tightening

- **Status:** ✅ Applied
- **File:** `src-tauri/tauri.conf.json`
- **Current policy:** `script-src` is restricted to `'self' 'wasm-unsafe-eval'`; `connect-src` is restricted to `'self' ipc:`; wildcard localhost/network sources and `'unsafe-eval'` are absent.
- **Intentional allowances:** `style-src`, `style-src-elem`, and `style-src-attr` include `'unsafe-inline'` for the current Dioxus/bundled UI; `img-src` includes `data:`/`blob:` for QR and generated image content; `font-src` includes `data:` for bundled asset compatibility.
- **Release requirement:** These allowances must remain documented and validated against the exact production WebView. They are not equivalent to a fully nonce-based CSP and should not be described as removed until the UI no longer needs them.

### M19 — ToastContainer empty / Toast wiring

- **Status:** ✅ Verified complete
- **File:** `frontend/src/lib.rs`, `frontend/src/components/shared/toast.rs`, `frontend/src/components/notifications/notification_toast.rs`
- **Description:** ToastContainer and NotificationToast are both rendered in `App()` (lib.rs:890–891), `provide_toast_store()` is called at lib.rs:74, and toasts have a dismiss button with auto-dismiss.

---

## 🟢 Low Priority — Completed Items

These are nice-to-have improvements, cleanup items, or architectural notes that can be deferred until higher-priority work is complete.

- [x] **Notification max count** — Limit total notifications to prevent memory bloat (fixed: `MAX_NOTIFICATIONS = 50`)
- [x] **Accessibility audit (basics)** — Added ARIA labels and roles to key interactive components (modal, command palette, notification bell, sidebar)
- [x] **Dead features cleanup** — Removed by the August 2026 refactor (see updated Appendix below)
- [x] Context menu — Wired `ContextMenu` component (already had full CSS) into `WorkspaceTab` for right-click "Close workspace" (2026-08-06)
- [x] Resizable panel improvements — `ResizablePanel` / `ResizeHandle` shared components exist and the right sidebar uses inline resize; no further action needed
- [x] `did_attempt` reset logic — No longer applicable; `did_attempt` does not exist in the current codebase (renamed/removed during refactor)
- [x] Plugin system hardening — bounded manifests/config/events, trusted-integration policy, and atomic owner-aware session operations with cross-plugin isolation coverage
- [x] E2E test coverage expansion — added and passed `pane-swap.e2e.mjs` for two-pane drag/swap plus `pane-scaling-10plus.e2e.mjs` for 12-pane geometry and timing coverage
- [x] Documentation for plugin API — reconciled current manifest, capability, config, lifecycle, and MCP contracts in `docs/plugin-development.md` and `docs/plugin-system-guide.md`
- [x] Performance audit of large workspace grids (10+ panes) — deterministic 12-pane mount/relayout stress coverage added
- [x] macOS release evidence automation — local release-check orchestration plus unsigned DMG `.app` structure and arm64 verification added
- [ ] Native window chrome/menu polish review — remains a manual macOS UX gate

---

## Next Actions

1. ✅ **All compile errors fixed** — Build passes cleanly
2. ✅ **All medium priority items fixed** — Including CSP tightening and M19 toast wiring
3. ✅ **Dead code cleanup complete** — Removed unused types, components, stores, and dead functions (August 2026 refactor)
4. ✅ **Context menu wired** — `ContextMenu` component wired into `WorkspaceTab` for right-click "Close workspace" (2026-08-06)
5. ✅ **Local backlog implementation pass complete** — plugin hardening/docs, 12-pane geometry coverage, and macOS release-evidence automation are implemented and locally validated
6. **Remaining release-owner gates** — clean-machine macOS UX review, signing/notarization, Finder install/launch, packaged stability soak, supply-chain disposition, and named approvals

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
