# Athenas-Core Roadmap

> Comprehensive tracking of bugs, issues, and fixes discovered during deep-dive audit and refactoring sessions.
> Last updated: 2026-09-01 (v3.3.0 release + CI/release hardening pass)

---

## 🚀 2026-09-01 — v3.3.0 release + CI/release hardening

Version bumped from 0.3.0 → **3.3.0** across `package.json`, `package-lock.json`,
`src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `frontend/Cargo.toml`.

**Bugs found by CI this pass (all fixed):**

- [x] `alsa-sys` build failure on Ubuntu CI — `cpal` (voice input) needs `libasound2-dev`; added to all three CI apt lines.
- [x] **RUSTSEC-2026-0258** — `h2 0.4.14 → 0.4.19` (unbounded empty DATA frames).
- [x] Release identity script: CI-env guard fired on every branch run after
  `test:release-scripts` invoked it bare; now only tag runs require an explicit
  version, and empty `RELEASE_VERSION` is treated as unset.
- [x] `check-release-identity` in workflow_dispatch had no version source —
  release workflow now forwards `inputs.version`.
- [x] Inline release-identity check read `$GITHUB_ENV` within the same step
  (never applied) — now passes `RELEASE_VERSION=…` directly to the node invocation.
- [x] `dx build` strip step died on GitHub macOS runners (`libLLVM.dylib`
  missing) — release workflow now installs the `llvm-tools` rustup component.
- [x] MCP `search-files` tests failed on the macOS release runner — needs
  `brew install ripgrep`; added.
- [x] Tauri build script failed on fresh checkouts (`resource path
  ../frontend/dist/mobile.html doesn't exist`): ubuntu rust jobs materialize a
  placeholder dist; release workflow builds the real dist before `cargo check`.
- [x] Flaky `agent_detection::foreground_cache_dedupes_within_ttl` —
  both cache tests share a process-global cache; serialized with a test mutex.
- [x] Root prettier drift in `packages/mcp-server/src/server.ts`.

**This session also closed earlier roadmap items:**

- F1 (fd-reuse flake) — subprocess isolation (commit `8837202`)
- P1 (clippy warm-cache blind spot) — dedicated, wiped `target/clippy-baseline` (commit `651fd49`)
- D1 (theme count) + D2 (palette shortcut) — README corrected

**Still open (needs user action):**

- [x] **Apple signing** — decision (2026-09-02): ship **unsigned**. **Release published 2026-09-02**: tag `v3.3.0` → `a2371ca`, CI `macOS Release` run 33573856164 succeeded; public (non-draft) GitHub release carries `Athena.s.Core_3.3.0_aarch64.dmg` (~10 MB) + sha256. Post-ship fix landed in that tag: `byte_char_slices` (clippy 1.98, CI stable is newer than local 1.95 gate — toolchain drift remains a risk; consider pinning `rust-toolchain.toml`). The
  release workflow now publishes an unsigned DMG on tag push (signing-gate
  hard-fail removed, publish step no longer conditioned on
  `APPLE_SIGNING_IDENTITY`); users bypass Gatekeeper via right-click → Open.
  Restore signing + notarization when secrets are configured.
- [x] **Deprecation UX** — provider responses classified centrally (`orchestrator_support::classify_api_error`): HTTP 410, or 404 with a model-scoped body, now return `OrchestratorError::ModelUnavailable`, which emits `AthenaStreamEvent::Error{ model_unavailable: true }` with guidance text; the desktop Athena panel (`athena_panel.rs`) detects the flag and opens the Settings modal so the user picks another model. Covered by `model_gone_yields_model_unavailable_error_event` (stream contract) + three classifier unit tests (2026-09-02).

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
| 🟢 Low priority remaining    | 0       |
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
- [x] Plugin API contract — reconciled the current manifest, capability, config, lifecycle, and MCP contracts
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

## 🔁 2026-08-30 Verification Pass (full test + tooling audit)

Complete verification of every automated gate in the repo. Results:

| Gate | Result |
| ---- | ------ |
| `cargo check --workspace` | ✅ clean |
| `cargo test --workspace` | ✅ 674 tests passed, 0 failed (52+225+1+4+8+8+8+199+14+51+9+26+10+41+9+30+30) |
| `cargo clippy` vs baseline | ✅ baseline current (fixed 2 new warnings: `needless_return` in `diagnostics.rs`, `assertions_on_constants` in `relay/ws.rs`; `type_complexity` in `orchestrator.rs` resolved via `AutoSaveSlot` type alias) |
| `npm test` (vitest, 11 files) | ✅ 184 passed |
| `npm run check:tauri-commands` | ✅ 144 commands consistent |
| `npm run check:tauri-permissions` | ✅ 144 commands consistent |
| `npm run check:plugin-integration` | ✅ passed (12 lint warnings, 0 errors) |
| `npm run check:tauri-security` | ✅ 5 invariants passed |
| `npm run check:release-privacy` | ✅ 23 invariants passed |
| `npm run check:release-identity` | ✅ Athena's Core 3.3.0 (macOS Apple Silicon DMG scope) |
| `npm run lint` | ⚠️ 45 warnings, 0 errors (all `no-explicit-any` / unused-vars in plugins + tests — cosmetic) |
| `frontend/build-dist.sh` | ✅ dist built (sw.js, xterm vendor addons) |
| E2E suite (18 specs) | ⚠️ requires `tauri-wd` driver running on port 4444 — environment gate, not a code defect; specs themselves unchanged and healthy |

- [x] **F1 — Flaky test `owned_fd_lease_survives_close_and_master_fd_reuse`** — fixed via subprocess isolation (commit `8837202`).
- [x] **F2 — E2E setup docs** — `e2e-tests/README.md` now documents `tauri-wd` install/start, the debug-binary build, the 4444 port expectation, and the "Unable to connect" failure meaning (2026-09-02).

### 🟢 Low — found this pass

- [x] **F3 — `plugins/shared/setup.ts` warnings fixed** — unused `PLUGIN_IDS` import and unused `writeMcpConfig` `agentType` param removed; `Record<string, any>` → `Record<string, unknown>`; `catch (err: any)` → `unknown` + `instanceof Error` (2026-09-02).
- [x] **F4 — `postcss.config.js` renamed to `postcss.config.mjs`** — `MODULE_TYPELESS_PACKAGE_JSON` warning gone; vitest 184/184 still green (2026-09-02).
### 🐛 Fixed during this pass (independent audit)

- [x] **F5 — IPC metrics double-counted** (`frontend/src/tauri_bridge.rs`): `invoke()` called `record_ipc()` and then delegated to `invoke_js_value()`, which recorded the same call again — every JSON invoke was counted twice in `window.__athenaMetrics`. The function also built a dead `invoke_fn` handle (Reflect lookup + downcast, never used). Fixed: `invoke()` now only parses args and delegates; each call counts once.
- [x] **F6 — clippy `redundant_redefinitions` in `modal.rs`**: two `let overlay_count = overlay_count;` shadows on a `Copy` `Signal<u32>` (would have failed the cold-CI clippy gate). Removed.

### 📄 Docs drift — found this pass

- [x] **D1 — README theme count** (closed: README corrected in v3.3.0 pass): README claims "16 themes"; `ALL_THEMES` in `frontend/src/themes/definitions.rs` has **6** (nyx, aegis, erebus, pentelic, olive, sky). Fix the claim or ship more themes.
- [x] **D2 — Command palette advertised but removed** (closed: README corrected in v3.3.0 pass): README shortcuts table lists `Cmd+K`/`Cmd+P` "Show command palette", yet the palette is gone from the frontend — `keybindings.rs` test `removed_command_palette_shortcuts_are_not_global_actions` asserts both keys classify to `None`. Decide: restore the palette or fix the README.
- [x] **D3 — contributor note added** — README "Build from source" now states there are no local pre-commit hooks and CI is the only lint gate (2026-09-02).

### 🛠 Tooling findings — found this pass

- [x] **P1 — Clippy baseline script is warm-cache blind** (closed: dedicated wiped target dir, commit 651fd49): `run-clippy-baseline.mjs` compares `cargo clippy --message-format=json` output against the baseline, but on a warm `target/` the JSON stream omits cached-crate diagnostics, so a dirty tree can pass locally and fail on cold CI (observed this session: first run flagged 2 new warnings, all subsequent warm runs reported the baseline current). Fix: clean the checked packages in the script (or use a dedicated `CARGO_TARGET_DIR`) before each run.
- [x] **P2 — baseline script now passes `--all-targets`**; test-code warnings fixed (`field_reassign_with_default` via struct-update in `open_link.rs`, `useless_format`/`format_in_format_args` in `athena-resume-scanner` / `orchestrator_stream_contract` tests). One pre-existing `needless_question_mark` in `frontend/src/components/mobile_xterm.rs` remains — part of the in-flight mobile-relay diff, owned by that workstream (2026-09-02).
- [x] **P3 — Perf metrics gated to debug builds** — `install_window_snapshot` + the 2 s refresh interval now live behind `#[cfg(debug_assertions)]` in `frontend/src/lib.rs`; e2e metrics specs run against the debug binary and are unaffected (2026-09-02).

### 🧪 Coverage gaps — ranked by launch impact

1. ~~**AI chat happy path**~~ — ✅ **Done 2026-08-31**: `e2e-tests/test/specs/athena-chat-stub.e2e.mjs` drives the full loop (workspace → composer → loopback OpenAI stub → streaming bubble) and asserts the request hit the stub with the stored key/model. Config is injected at runtime via `store_set` (disk seeding races the app's in-memory store); user store values + keyring are snapshotted and restored. Caveat found & worked around: `store_get("llm.api_key")` trusts the stale `llm.api_key_status` sentinel over the keyring — the spec deletes the sentinel; whether the backend should is filed below.
2. **Kanban persistence** — create/move/persist-across-restart is unit-tested in the store but has no UI-level e2e.
3. **Mobile mirror relay pairing UX** — ws auth/pairing is unit-tested in Rust; the desktop↔phone approval flow has no e2e.
4. **Plugin failure paths** — malformed manifest / plugin crash isolation is unit-tested but not e2e.
5. **Settings round-trip** — the new settings codex has no persistence e2e.

---

## 🔁 2026-08-27 Re-Audit (fresh deep dive)

Fresh whole-repo audit after the August 2026 refactor and post-roadmap work
(perf metrics, launch handoff, funding config). Prior leak/bounds classes are
verified closed: every event-listener registration found (plugin/output event
buses, athena panel, notification bell/toast, file tree, swarm board, terminal
drop, browser surface, relay prompts) has a matching `use_drop` unlisten; all
bounded stores still enforce caps (`MAX_NOTIFICATIONS=50`, `MAX_TOASTS=50`,
`MAX_LINES_PER_BUFFER=5000`, `MAX_TEXT_LENGTH=10000`, `MAX_PANE_COUNT=100`,
`MAX_MESSAGES=100`, 4 MB PTY write-queue coalesce cap, 20 MB image-drop cap).
Remaining `unwrap()`/`expect()` hits are test code or crash-safe startup
paths, with the exceptions below.

### 🟡 Medium

- [x] **R1 — Extra-root path allowlist** — implemented: `PathValidator::with_extra_roots` (canonicalized, nonexistent roots rejected at construction) in `athena-fs`; `ToolExecutor::with_fs_extra_roots` (construction-time opt-in) in `athena-core`; `fs_tools.rs` TODO removed. Covered by `test_extra_roots_accept_paths_under_additional_root` and `test_extra_roots_rejects_nonexistent_root`.
- [x] **R2 — HTTP-client construction deduplicated** — single `AthenaOrchestrator::build_http_client()` helper; all three constructors call it. Panic remains intentional (TLS-init failure is startup-fatal) but now has a precise message.

### 🟢 Low

- [x] **R3 — let-else in `agent_comms_connection.rs`** — guard and bind fused via `let Some(session) = session else`, unwrap removed.
- [x] **R4 — Single-pass validation in `plan_tools.rs`** — validate loop now collects typed `(&str, StepStatus)` pairs; the expect-based second loop is gone.
- [x] **R5 — Relay keep-alive fields renamed** — `_runtime` / `_discovery` in `src-tauri/src/relay/mod.rs`; `#[allow(dead_code)]` removed.
- [x] **R6 — Diagnostics fallback cfg-gated** — `#[allow(unreachable_code)]` replaced with `#[cfg(not(any(target_os = "macos", windows, linux")))]` fallback in `diagnostics.rs`.

**Verification:** `cargo check --workspace` clean (one pre-existing unrelated frontend warning); `cargo test -p athena-fs` 14 passed, `-p athena-core` passed, `-p athenas-core` 80 passed (2026-08-27).

### 🟢 Process / release gates confirmed still open

- [x] Native window chrome/menu polish — window `title` is now `Athena's Core` with `hiddenTitle: true` (correct Window-menu/Mission Control name, no native text over the custom titlebar); an explicit macOS app menu (App/Edit/View/Window) is built in `src-tauri/src/main.rs` `setup` — the missing Edit menu (copy/paste/undo shortcuts) and Window menu entries are now guaranteed. Custom titlebar already had the 80px traffic-light spacer and correct drag/no-drag regions. Code verified via `cargo check` + `cargo tauri build --debug --no-bundle`; visual menu/text confirmation remains a manual macOS UX gate (2026-08-28).
- [ ] Apple signing secrets not exercised — `release-macos.yml` fully supports signing/notarization/stapling, but tag pushes hard-fail without `APPLE_SIGNING_IDENTITY`; the gate is verifying this works end-to-end on a real tag.
- [x] E2E coverage healthy — 18 specs in `e2e-tests/test/specs/`, zero `it.skip`/`describe.skip`; includes 12-pane scaling, pane swap, and `release-soak.e2e.mjs`.
- [x] CI note — `athena-terminal` tests are intentionally excluded from GitHub-hosted Ubuntu (PTY fork kills the runner VM); they run locally. Acceptable, but document in contributor docs.

---

## Appendix: Dead Code Analysis

> **Status:** ✅ Resolved by the August 2026 codebase refactor.
> All items below were removed or consolidated during the P0–P1 refactor passes.

| Category            | File                                                                | What                       | Status     |
| ------------------- | ------------------------------------------------------------------- | -------------------------- | ---------- |
| Dead types          | `frontend/src/types/{command.rs,editor.rs,notification.rs,task.rs}` | Deprecated type re-exports | ✅ Removed |
| Dead component dirs | `frontend/src/components/{browser,editor}/`                         | Unused component trees     | ✅ Removed |
| Dead component      | `frontend/src/components/panel.rs`                                  | Legacy panel               | ✅ Removed |
| Dead function       | `frontend/src/stores/agent_output.rs` `find_buffer_index()`         | `#[allow(dead_code)]`      | ✅ Removed |
| Dead store          | `frontend/src/stores/layout.rs`                                     | Deprecated no-op shim      | ✅ Removed |
