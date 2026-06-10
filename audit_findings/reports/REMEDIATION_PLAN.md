# Remediation Plan for Athena's Core — Security, Concurrency & Quality Audit

## Overview

This document consolidates findings from 10 focused sub-agent audits of the **Athena's Core** Tauri 2 + Dioxus 0.7 codebase. It provides a structured remediation plan grouped by category, with priority, effort, ownership, dependencies, and verification ideas for each item.

---

## Severity to Priority Mapping

| Severity in Audit | Remediation Priority | Action Timeline |
|---|---|---|
| Critical | **P0** | Fix immediately; blocks release |
| High | **P0/P1** | P0 if security/ crash; P1 if performance/ maintainability |
| Medium | **P1** | Next sprint |
| Low | **P2** | Backlog / opportunistic |

---

## Legend

| Effort | Definition |
|---|---|
| Small | 1–2 hours |
| Medium | 1–2 days |
| Large | 1+ weeks |

---

## 1. Security Hardening

### P0: Path Traversal in `athena-fs` — `ensure_within_home` TOCTOU + Symlink Bypass

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `crates/athena-fs/src/lib.rs` |
| **Owner** | Core Platform Team |
| **Description** | `ensure_within_home` canonicalizes but downstream operations use the original path; TOCTOU races exist; symlink escapes are possible. See AGENT_06: FS-01/FS-02. |
| **Dependencies** | None |
| **Verification** | 1. Unit test with symlink created after canonicalization check. 2. Fuzz test `..` and symlink combinations. 3. Verify `read_tree` uses canonical path. |

### P0: Path Traversal in `validate_path` (Write Path)

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Small |
| **Module** | `src-tauri/src/commands/mod.rs`, `src-tauri/src/commands/fs.rs` |
| **Owner** | Core Platform Team |
| **Description** | Write path in `validate_path` does not canonicalize before `starts_with` check, allowing symlink escapes. See AGENT_07: Finding 1. |
| **Dependencies** | None |
| **Verification** | 1. Unit test: create symlink inside workspace → write outside. 2. Ensure `validate_path` returns `Err` for symlink-escaping paths. |

### P0: Shell Command Injection via `tool_executor.rs`

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/tool_executor.rs` |
| **Owner** | Core Platform Team |
| **Description** | `command` argument to `launch_custom_agent` passes raw string to `agent_spawned`; allowlist is optional and off by default. See AGENT_01: C2. |
| **Dependencies** | None |
| **Verification** | 1. Test that `command` containing shell metacharacters is rejected when allowlist is empty. 2. Test that `bash -c "evil"` fails allowlist check. |

### P0: API Key Exposure / Leakage Pathways

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/orchestrator.rs`, `src-tauri/src/commands/athena.rs`, `frontend/src/components/settings/settings_modal.rs` |
| **Owner** | Security / Backend Team |
| **Description** | API keys in `ProviderConfig` are `Clone`-able; error responses may leak keys; frontend stores key in plain `use_signal`; key is not zeroized. See AGENT_01: C3, AGENT_07: Finding 7, AGENT_09: M-12. |
| **Dependencies** | None |
| **Verification** | 1. Search codebase for `api_key` logging/serialization. 2. Verify `SecretString` usage. 3. Test that no error response contains the key substring. |

### P1: Plugin Hook Script Validation Broken on Windows

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-plugins/src/lib.rs` |
| **Owner** | Plugin / Cross-Platform Team |
| **Description** | `validate_hook_script` only checks for `/`-prefixed absolute paths and `/..` sequences, missing Windows `C:\` and `\..` forms. See AGENT_06: PL-01. |
| **Dependencies** | None |
| **Verification** | 1. Unit test on Windows with `C:\evil`, `..\file`, `dir/..` paths. 2. Ensure all are rejected. |

### P1: Dangerous Scheme Blocks Missing in `normalize_url`

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-browser/src/lib.rs` |
| **Owner** | Frontend / Browser Team |
| **Description** | `normalize_url` only blocks `javascript:`, `data:`, `vbscript:`, `file:` — missing `about:`, `chrome:`, `edge:`, `view-source:`, `blob:`, `filesystem:`. See AGENT_06: BR-01. |
| **Dependencies** | None |
| **Verification** | 1. Test each forbidden scheme returns `Err`. 2. Test `http://` / `https://` still passes. |

### P1: MCP Server — Unauthenticated `tools/call` After Connection

| Field Harness | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/mcp.rs` |
| **Owner** | Backend / MCP Team |
| **Description** | After `initialize`, subsequent `tools/call` requests are processed without re-authentication. Skipping `initialize` may still allow tool execution. See AGENT_03: Finding 4. |
| **Dependencies** | None |
| **Verification** | 1. Integration test: send `tools/call` without `initialize` → expect auth error. 2. Send `tools/call` after `initialize` → expect success. 3. Verify token expiry/rotation. |

### P1: `browser_open_external` URL Validation Missing

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `src-tauri/src/commands/mod.rs` |
| **Owner** | Frontend / Browser Team |
| **Description** | Opens any URL from frontend without scheme filtering. Dangerous schemes can trigger local application execution. See AGENT_07: Finding 8. |
| **Dependencies** | None |
| **Verification** | 1. Test `file:///etc/passwd` is rejected. 2. Test `http://example.com` is allowed. |

### P1: `agent_comms_token` Exposed Without Access Control

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `src-tauri/src/commands/agent.rs` |
| **Owner** | Security / Backend Team |
| **Description** | Any caller can retrieve the comms token. See AGENT_07: Finding 23. |
| **Dependencies** | None |
| **Verification** | 1. Verify that only authenticated/authorized agents can call this command. 2. Add integration test for unauthorized access. |

### P1: Plugin Registration Without Authentication

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `src-tauri/src/commands/plugin.rs`, `plugin_host.rs` |
| **Owner** | Plugin / Security Team |
| **Description** | `plugin_register` and `plugin_host_setup_plugin` allow arbitrary plugin registration without any auth. See AGENT_07: Finding 22. |
| **Dependencies** | None |
| **Verification** | 1. Attempt unauthorized plugin registration → expect failure. 2. Verify signature or capability token check. |

### P1: `ALLOWED_MCP_COMMANDS` Too Permissive (Shells Included)

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-plugins/src/lib.rs` |
| **Owner** | Plugin / Security Team |
| **Description** | `sh`, `bash`, `zsh` in the whitelist defeat the purpose of command allowlisting. See AGENT_06: PL-02. |
| **Dependencies** | None |
| **Verification** | 1. Verify `sh -c "rm -rf /"` is rejected. 2. Ensure non-shell commands still pass. |

### P2: `frame-src` CSP Too Permissive

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `src-tauri/tauri.conf.json` |
| **Owner** | Frontend / Security Team |
| **Description** | `frame-src 'self' *` allows framing any origin, enabling clickjacking. See AGENT_10: #17. |
| **Dependencies** | None |
| **Verification** | 1. Verify CSP header in built app. 2. Test that external framing is blocked. |

### P2: `unsafe-eval` in CSP

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `src-tauri/tauri.conf.json` |
| **Owner** | Frontend / Security Team |
| **Description** | `script-src 'unsafe-eval'` allows `eval()`-equivalent execution in a privileged desktop context. See AGENT_10: #1. |
| **Dependencies** | None |
| **Verification** | 1. Attempt `eval()` in frontend → expect CSP violation. 2. Verify app still functions without `unsafe-eval`. |

### P2: MCP Environment Variable Validation Insufficient

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `crates/athena-plugins/src/lib.rs` |
| **Owner** | Plugin / Security Team |
| **Description** | Only `PATH` and `HOME` are blocked. `LD_PRELOAD`, `LD_LIBRARY_PATH`, `PYTHONPATH`, `NODE_PATH`, etc. should also be forbidden. See AGENT_06: PL-08. |
| **Dependencies** | None |
| **Verification** | 1. Test that `LD_PRELOAD` in MCP env is rejected. 2. Test that safe vars like `FOO` are allowed. |

---

## 2. Concurrency & Async Fixes

### P0: Lock Poisoning Not Handled in Critical Orchestrator Paths

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/orchestrator.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `.lock().ok()` silently drops poisoned locks, causing immutable state after any panic. See AGENT_01: C4. |
| **Dependencies** | None |
| **Verification** | 1. Simulate lock poisoning in test. 2. Verify operations return `Err` or auto-recover. |

### P0: TOCTOU Race in `SessionManager::spawn`

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `crates/athena-terminal/src/session.rs` |
| **Owner** | Terminal / Backend Team |
| **Description** | Read-lock check for session ID followed by write-lock insert without re-check allows double-insert and fd leak. See AGENT_04: Critical #1. |
| **Dependencies** | None |
| **Verification** | 1. Concurrent spawn with same ID → only one session created. 2. Verify no fd leak via `/proc/self/fd` or equivalent. |

### P0: `output_buffer.rs` — `std::sync::Mutex` Event Emitter Held Across Sync Point

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/output_buffer.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `Arc<std::sync::Mutex<Option<...>>>` for event emitter can deadlock on reentrant access. See AGENT_01: H7. |
| **Dependencies** | None |
| **Verification** | 1. Trigger callback that re-enters `OutputBuffer` → should not deadlock. |

### P1: `RateLimiter` Not Globally Coordinated

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/orchestrator.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | Concurrent requests race the limiter, causing burst behavior. See AGENT_01: H6. |
| **Dependencies** | None |
| **Verification** | 1. Spawn multiple concurrent requests → verify rate is actually capped globally. |

### P1: `ask_user` Synchronous Callback May Deadlock

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/tool_executor.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `ask_user` blocks the calling thread; frontend may need to acquire same locks. See AGENT_01: H5. |
| **Dependencies** | `ask_user` async refactor |
| **Verification** | 1. Simulate `ask_user` with frontend mid-operation → no deadlock. |

### P1: `execute_tool` Runs Blocking I/O on Main Thread

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/orchestrator.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `execute_tool` does blocking file I/O, DB ops inside async loops without `spawn_blocking`. See AGENT_01: H4. |
| **Dependencies** | None |
| **Verification** | 1. Profile async runtime during tool execution. 2. Verify no thread blocking. |

### P1: `handle_request_input` Blocks Indefinitely Without Timeout

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/agent_comms.rs` |
| **Owner** | Agent / Backend Team |
| **Description** | `input_rx.recv()` with `sync_channel(1)` has no timeout; thread can block forever. See AGENT_02: 2.3. |
| **Dependencies** | None |
| **Verification** | 1. Test `handle_request_input` with no response → thread should timeout after N seconds. |

### P1: Thread Leak in `agent_comms.rs` Writer Thread

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/agent_comms.rs` |
| **Owner** | Agent / Backend Team |
| **Description** | Writer thread may not terminate when connection is cleaned up because `tx` is not dropped. See AGENT_02: 3.1. |
| **Dependencies** | None |
| **Verification** | 1. Monitor thread count during connect/disconnect cycles. 2. Verify writer thread exits. |

### P0: Unbounded Memory Growth in `pty_read_loop` (Coalesce Buffer)

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `src-tauri/src/commands/mod.rs` |
| **Owner** | Terminal / Backend Team |
| **Description** | `coalesce_buf` grows unbound if PTY output is faster than flush rate. See AGENT_04: Critical #2. |
| **Dependencies** | None |
| **Verification** | 1. Run `yes` in PTY → verify memory stays bounded. 2. Monitor `coalesce_buf` size via instrumentation. |

### P1: `blocking_lock()` in Non-Async Context (`AppState`)

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `src-tauri/src/state.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `blocking_lock()` on `tokio::sync::Mutex` in non-async context is correct but risky; may deadlock if called from async context later. See AGENT_05: #13. |
| **Dependencies** | None |
| **Verification** | 1. Audit all callers of `wire_swarm_events`. 2. Ensure never called from async context. |

---

## 3. Memory Management

### P0: `Closure.forget()` Leak in `pty_listen_binary`

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `frontend/src/tauri_bridge.rs` |
| **Owner** | Frontend / WASM Team |
| **Description** | `Closure` dropped via `.forget()` can never be GC'd, leaking memory per terminal session. See AGENT_08: C3. |
| **Dependencies** | None |
| **Verification** | 1. Open/close 100 terminal sessions → check WASM heap size. 2. Verify Closure count in JS heap. |

### P0: Unbounded Agent Output Buffer Growth

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `frontend/src/stores/agent_output.rs` |
| **Owner** | Frontend Team |
| **Description** | `AgentOutputInfo` entries never pruned; `OutputLine` stores full unbounded `String`s. See AGENT_08: C1. |
| **Dependencies** | None |
| **Verification** | 1. Simulate 1000 pane open/close cycles → verify no heap growth. 2. Monitor `AgentOutputState` size. |

### P1: `mounted_spaces` Signal Never Cleaned Up

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/lib.rs` |
| **Owner** | Frontend Team |
| **Description** | `mounted_spaces` HashSet grows unboundedly as spaces are created/destroyed. See AGENT_08: H4. |
| **Dependencies** | None |
| **Verification** | 1. Create/destroy 100 spaces → monitor `mounted_spaces` size. |

### P1: `SessionStore` / `KeyValueStore` — Unbounded Message/History Growth

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `crates/athena-store/src/types.rs`, `crates/athena-store/src/store.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `ChatSession` messages Vec is unbounded; `KeyValueStore` rewrites entire store on every `set`/`delete`. See AGENT_05: #7, #8. |
| **Dependencies** | None |
| **Verification** | 1. Create session with 10k messages → measure memory and write latency. 2. Batch 1000 keys and measure write time. |

### P1: Agent Status & Notification Vectors Grow Unbounded

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/stores/agent_status.rs`, `frontend/src/stores/notification.rs` |
| **Owner** | Frontend Team |
| **Description** | `statuses` and notification arrays never cleaned up automatically. See AGENT_08: M8, AGENT_09: C-1/C-2. |
| **Dependencies** | None |
| **Verification** | 1. Open/close many panes → verify no `statuses` growth. 2. Trigger many notifications → verify max cap respected. |

### P1: Unbounded `Line` Buffer in `search_code`

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/search.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `context_lines` and `max_results` are unbounded, can cause DoS. See AGENT_03: Finding 6, Finding 7. |
| **Dependencies** | None |
| **Verification** | 1. Request `u32::MAX` context lines → expect capped at 100. 2. Request `usize::MAX` results → expect capped at 5000. |

---

## 4. Error Handling

### P0: Silent Ignores of Deserialization Errors

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/agent_comms.rs`, `crates/athena-core/src/search.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `Err => continue;` silently drops invalid JSON and ripgrep parse errors. See AGENT_02: 6.1, AGENT_03: Finding 19. |
| **Dependencies** | None |
| **Verification** | 1. Send malformed JSON to agent comms → expect logged error. 2. Corrupt ripgrep JSON output → expect parse error logged. |

### P1: `has()` Silently Recovers from Poisoned Mutex

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-store/src/store.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `has()` returns `bool` even if mutex poisoned; other methods return `StoreError::Generic`. Inconsistent. See AGENT_05: #2. |
| **Dependencies** | None |
| **Verification** | 1. Poison the mutex, call `has()` → expect `Err` instead of silent recovery. |

### P1: `ask_user` Returns Error String Literal Instead of Error Type

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `src-tauri/src/state.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | Returns `"error: user response timed out"` as a literal String; caller may act on it. See AGENT_05: #4. |
| **Dependencies** | None |
| **Verification** | 1. Verify `ask_user` returns `Result<String, UserTimeout>` or similar. 2. Test caller handles timeout correctly. |

### P1: `AppState::new()` Redundant Retry with Identical Call

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `src-tauri/src/state.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `.with_name_sync("store")` is called, and on failure the *exact same* call is retried. See AGENT_05: #3. |
| **Dependencies** | None |
| **Verification** | 1. Verify only one `with_name_sync` call and direct `new_empty()` fallback. |

### P1: `now_ms()` Uses `unwrap_or_default()` with Misleading Timestamp

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/orchestrator.rs`, `src-tauri/src/commands/session.rs`, `src-tauri/src/commands/notification.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()` yields `0` on epoch failure, corrupting ordering. See AGENT_01: L2, AGENT_07: Finding 11, Finding 15. |
| **Dependencies** | None |
| **Verification** | 1. Mock system time before UNIX_EPOCH → expect warning / error instead of `0`. |

---

## 5. Performance Optimization

### P1: `AthenaState` `PartialEq` Triggers O(N) Comparison on Every Signal Write

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `frontend/src/stores/athena.rs` |
| **Owner** | Frontend Team |
| **Description** | Derived `PartialEq` does deep comparison of entire message history. See AGENT_08: H3. |
| **Dependencies** | None |
| **Verification** | 1. Profile `Signal::set()` with 1000 messages → O(1) comparison. |

### P1: `highlighter.rs` Massive Code Duplication

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `frontend/src/utils/highlighter.rs` |
| **Owner** | Frontend Team |
| **Description** | ~1860 lines with copy-pasted line-number parsing across ~8 language highlighters. See AGENT_10: #8. |
| **Dependencies** | None |
| **Verification** | 1. Refactor to shared tokenizer or `syntect`. 2. Unit test all language paths. |

### P1: `fuzzy_search.rs` Repeated `to_lowercase()` Computes O(n²)

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/utils/fuzzy_search.rs` |
| **Owner** | Frontend Team |
| **Description** | `item.to_lowercase()` called in sort closure; O(n²) lowercase operations. See AGENT_10: #6. |
| **Dependencies** | None |
| **Verification** | 1. Benchmark search on 1000 items → <10ms. |

### P1: `KeyValueStore` Rewrites Entire Store on Every `set`/`delete`

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `crates/athena-store/src/store.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | Every `set`/`delete` serializes and writes the entire in-memory `HashMap`. See AGENT_05: #8. |
| **Dependencies** | None |
| **Verification** | 1. Batch 1000 writes → <100ms total. 2. No full-store rewrites observed in profile. |

### P2: `save_image` / `load_image` Wasteful Base64 Round-Trip

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `crates/athena-store/src/session.rs` |
| **Owner** | Core Platform / Backend Team |
| **Description** | `save_image` decodes base64; `load_image` re-encodes. CPU overhead on every load. See AGENT_05: #6. |
| **Dependencies** | None |
| **Verification** | 1. Load 100 images → measure CPU. 2. Verify no re-encoding or caching works. |

### P2: `CircuitBreaker` `failure_count` Not Reset on `record_success`

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `frontend/src/utils/circuit_breaker.rs` |
| **Owner** | Frontend Team |
| **Description** | `failure_count` not reset on success, causing premature tripping. See AGENT_10: #11. |
| **Dependencies** | None |
| **Verification** | 1. Unit test: fail N times, succeed, fail again → circuit should not trip. |

---

## 6. Frontend Stability

### P0: Tauri Event Listeners Never Cleaned Up (`listen()` leak)

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `frontend/src/tauri_bridge.rs`, all component files |
| **Owner** | Frontend Team |
| **Description** | `listen()` returns unlisten function; no callers store or invoke it. See AGENT_08: H1, AGENT_09: C-1/C-2/C-3. |
| **Dependencies** | None |
| **Verification** | 1. Mount/unmount component 100x → verify event listener count in Tauri. |

### P1: `WorkspaceState` Save Race Condition / Write Ordering Bug

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Medium |
| **Module** | `frontend/src/stores/workspace.rs` |
| **Owner** | Frontend Team |
| **Description** | Multiple overlapping `spawn_local` save tasks can cause stale writes. See AGENT_08: C2. |
| **Dependencies** | None |
| **Verification** | 1. Rapidly mutate workspace → verify final persisted state matches last mutation. |

### P1: `PanelManagerState` / `UIState` Split Brain

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/stores/panel_manager.rs`, `frontend/src/lib.rs` |
| **Owner** | Frontend Team |
| **Description** | Keyboard shortcuts set `UIState.panel` but never `PanelManagerState.active_panel`. See AGENT_08: H5. |
| **Dependencies** | None |
| **Verification** | 1. Press `Cmd+2` → verify both stores reflect `Editor`. |

### P1: `NotificationBell` / `NotificationToast` Duplicate Listener Registration

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/components/notifications/notification_bell.rs`, `notification_toast.rs` |
| **Owner** | Frontend Team |
| **Description** | Listeners re-registered on every mount without cleanup. See AGENT_09: C-1/C-2. |
| **Dependencies** | None |
| **Verification** | 1. Mount/unmount 100x → verify single listener. 2. Check `use_drop` cleanup runs. |

### P1: `FileTree` Registers New `fs:change:*` Listener on Every Render

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/components/sidebar_dir/file_tree.rs` |
| **Owner** | Frontend Team |
| **Description** | `listen("fs:change:*", ...)` called in effect without unlisten or guard. See AGENT_09: C-3. |
| **Dependencies** | None |
| **Verification** | 1. Change `active_dir` 100x → verify only one listener exists. |

### P1: `TerminalStore::kill()` Removes Session Before Confirming Backend Kill

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `frontend/src/stores/terminal.rs` |
| **Owner** | Frontend Team |
| **Description** | Session removed from map even if `pty_kill` fails. See AGENT_08: M3. |
| **Dependencies** | None |
| **Verification** | 1. Mock `pty_kill` failure → session should remain in map. |

### P2: `ToastContainer` Never Removes Expired Toasts

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `frontend/src/components/shared/toast.rs` |
| **Owner** | Frontend Team |
| **Description** | Toasts stay in `ToastState` forever unless manually dismissed. See AGENT_09: M-1. |
| **Dependencies** | None |
| **Verification** | 1. Add 100 toasts → verify auto-removal after `duration_ms`. |

### P2: `Tooltip` Component is Just `title` Attribute Placeholder

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `frontend/src/components/shared/tooltip.rs` |
| **Owner** | Frontend Team |
| **Description** | Using `title` attribute is not accessible for keyboard or touch users. See AGENT_09: M-10. |
| **Dependencies** | None |
| **Verification** | 1. Keyboard tab navigation → tooltip appears. 2. Screen reader reads tooltip text. |

### P2: `Modal` Does Not Trap Focus

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Medium |
| **Module** | `frontend/src/components/shared/modal.rs` |
| **Owner** | Frontend Team |
| **Description** | No focus trapping; keyboard users can tab out of modal. See AGENT_09: M-11. |
| **Dependencies** | None |
| **Verification** | 1. Open modal → tab should cycle within modal only. |

### P2: `agent_output_panel.rs` Clones Entire Buffer on Every Render

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `frontend/src/components/agents/agent_output_panel.rs` |
| **Owner** | Frontend Team |
| **Description** | `l.clone()` clones entire `Vec<OutputLine>` per render for selected pane. See AGENT_09: M-8. |
| **Dependencies** | None |
| **Verification** | 1. Render with 10k lines → no full clone in profile. |

### P2: `ErrorBoundary` is a No-Op

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `frontend/src/components/shared/error_boundary.rs` |
| **Owner** | Frontend Team |
| **Description** | Component is just a pass-through; crashes take down the entire app. See AGENT_09: L-2. |
| **Dependencies** | None |
| **Verification** | 1. Trigger error in child → expect graceful fallback UI. |

---

## 7. Testing Gaps

### P0: No Concurrency Stress Tests for `AgentComms`

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Large |
| **Module** | `crates/athena-core/src/tests.rs`, `crates/athena-core/src/agent_comms.rs` |
| **Owner** | QA / Backend Team |
| **Description** | Zero tests for actual TCP communication, concurrent agents, or lock poisoning. See AGENT_02: 7.1. |
| **Dependencies** | None |
| **Verification** | 1. Add integration test: spin up `init_agent_comms`, connect, authenticate, send messages, handle input, disconnect. |

### P0: No Test for Stalled Agent Logic in `SwarmCoordinator`

| Field | Detail |
|---|---|
| **Priority** | P0 |
| **Effort** | Medium |
| **Module** | `crates/athena-core/src/tests.rs`, `crates/athena-core/src/swarm.rs` |
| **Owner** | QA / Backend Team |
| **Description** | Core stall detection logic is completely untested. See AGENT_02: 7.3. |
| **Dependencies** | None |
| **Verification** | 1. Write stale state file, call `watch_state`, verify stall flag set. |

### P1: Tests Write to Real Data Directory, Not Isolated Temp Dir

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-store/src/session_tests.rs`, `crates/athena-store/src/tests.rs` |
| **Owner** | QA / Backend Team |
| **Description** | Tests write to `~/.config/athena-core/`, risking clobbering production data. See AGENT_05: #16. |
| **Dependencies** | None |
| **Verification** | 1. Run tests → verify no files created in real data dir. |

### P1: `test_watch_prevents_duplicates` Only Sleeps, No Behavior Verification

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/tests.rs` |
| **Owner** | QA / Backend Team |
| **Description** | Test sleeps but does not assert only one watch task is running. See AGENT_02: 7.2. |
| **Dependencies** | None |
| **Verification** | 1. Verify `watching_dirs` contains exactly one entry after duplicate calls. |

### P1: No Lock Poisoning Tests for `NotificationService`

| Field | Detail |
|---|---|
| **Priority** | P1 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/tests.rs`, `crates/athena-core/src/notification.rs` |
| **Owner** | QA / Backend Team |
| **Description** | `NotificationService` has explicit error paths for poisoning but no test covers them. See AGENT_02: 7.4. |
| **Dependencies** | None |
| **Verification** | 1. Poison lock, call `push_notification` → verify `Err` or graceful handling. |

### P2: `send_to_agent_not_found` Tests Error Type but Not Delivery

| Field | Detail |
|---|---|
| **Priority** | P2 |
| **Effort** | Small |
| **Module** | `crates/athena-core/src/tests.rs` |
| **Owner** | QA / Backend Team |
| **Description** | Only checks error type returned, not actual TCP message delivery. See AGENT_02: 7.5. |
| **Dependencies** | TCP comms stress tests (above) |
| **Verification** | 1. Integration test with real TCP connection verifies message delivery. |

---

## Appendix: Effort Summary by Category

| Category | P0 Count | P1 Count | P2 Count | Total |
|---|---|---|---|---|
| Security Hardening | 3 | 7 | 3 | 13 |
| Concurrency & Async | 4 | 5 | 0 | 9 |
| Memory Management | 2 | 4 | 0 | 6 |
| Error Handling | 1 | 3 | 0 | 4 |
| Performance Optimization | 0 | 4 | 2 | 6 |
| Frontend Stability | 1 | 5 | 5 | 11 |
| Testing Gaps | 2 | 3 | 1 | 6 |
| **Total** | **13** | **31** | **11** | **55** |

---

*Plan generated: 2026-06-09*
*Audits: AGENT_01 through AGENT_10*
