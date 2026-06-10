# Verified Remediation Plan — Athena's Core

**Author:** Deep-dive review pass (no sub-agents)
**Date:** 2026-06-09
**Basis:** Direct verification of current working-tree code against the prior `audit_findings/` sub-agent reports. The prior reports are accurate in spirit but their line numbers are stale and ~half the P0 items are already fixed. **This document supersedes them** — it lists only what is *actually still broken* as of the current tree, with exact locations and concrete fixes.

> Executor: treat each item as an independent task. Each has a **Verify** step. Run `cargo check --workspace` and `bash frontend/build-dist.sh --debug` after each workstream. Do not "fix unrelated issues" — keep diffs minimal.

---

## 0. Status snapshot (what is ALREADY fixed — do NOT redo)

Confirmed fixed in the current tree:

- ✅ `orchestrator.rs` — migrated to `parking_lot::Mutex` / `tokio::sync::Mutex` (no more `.lock().ok()` poison-swallowing).
- ✅ `crates/athena-fs/src/path_validator.rs` — unified `PathValidator` exists with canonicalize + descendant check + symlink/`..` tests. `athena-fs/src/lib.rs` uses it for all read/write/tree ops.
- ✅ `search.rs` — both async and sync paths now push `--` before pattern/path (kills argument injection) and cap `max_results` (`MAX_RESULTS`) and `context_lines` (`MAX_CONTEXT_LINES`).
- ✅ `output_buffer.rs` — `parking_lot::Mutex`, `MAX_LINES_PER_PANE=5000`, `MAX_TOTAL_BYTES_PER_PANE=2_000_000`, emitter cloned out before callback (no lock held during emit).
- ✅ Frontend stores bounded: `agent_output.rs` (`MAX_LINES_PER_BUFFER`, `MAX_TEXT_LENGTH`), `terminal_blocks.rs` (`MAX_BLOCKS`, `MAX_OUTPUT_PER_BLOCK`, `VecDeque`).
- ✅ `tauri_bridge::listen()` — no longer calls `Closure::forget()`; returns a real unlisten fn that drops the JS GC roots.
- ✅ `xterm_mount.rs` — PTY listener stored and cleaned up via `use_drop` + `unlisten()`.
- ✅ `session.rs` — `CString::new` no longer panics (maps to `InvalidInput` error).

---

## P0 — Critical, still open

### P0-1. LLM file tools have NO path sandbox (`tool_executor.rs`)
**This is the single most important remaining issue.**

- **File:** `crates/athena-core/src/tool_executor.rs:1339`
- **Current code:**
  ```rust
  fn validate_path(&self, path: &str) -> Result<PathBuf, ToolExecutorError> {
      let root = self.get_workspace_root()?;
      let path = if Path::new(path).is_absolute() {
          PathBuf::from(path)            // absolute path taken VERBATIM
      } else {
          root.join(path)               // relative joined, but ".." not resolved
      };
      Ok(path)                          // NO canonicalize, NO descendant check
  }
  ```
- **Why it matters:** This is the gatekeeper for `fs_read_file` (1507), `fs_list_dir` (1542), and `fs_search` (1611) — all LLM-callable. An LLM (or prompt injection via web/file content) can request `/etc/passwd`, `~/.ssh/id_rsa`, `../../anything` and it is read/listed with zero restriction. The robust `PathValidator` already exists one crate away but is not used here.
- **Fix:**
  1. Add `athena-fs` (with `path_validator`) as a dependency of `athena-core` if not already (`crates/athena-core/Cargo.toml`).
  2. Replace the body of `validate_path` to construct a `PathValidator` rooted at the workspace root and call `.validate()` (reads/list) — return its error as `ToolExecutorError::PathTraversal` (add the variant).
  3. **DECIDED: root to the workspace dir (strict).** The tool executor faces adversarial LLM input (prompt injection via web/file content), so it must be stricter than the user-driven file-tree sidebar in `athena-fs` (which roots at home — that's a different, human-driven trust context; do not copy it here). Workspace-strict blocks `~/.ssh`, `~/.aws`, shell history, and other projects from a single crafted instruction. If legitimate out-of-workspace reads are ever needed, add them later as an explicit, logged allowlist opt-in — never as an open default. Leave a `// TODO: opt-in allowlist for extra roots` marker at the validator.
- **Verify:** add unit tests in `tool_executor.rs`: `fs_read_file` with `"/etc/passwd"`, `"../../../etc/passwd"`, and a symlink escaping the workspace all return `Err`. A normal in-workspace file returns `Ok`.

### P0-2. Tauri write-path validator does not canonicalize (`commands/mod.rs`)
- **File:** `src-tauri/src/commands/mod.rs:45` (`validate_path`, used by write at `:270` and `fs_exists` at `:283`).
- **Current code:** checks `path.starts_with(&root)` and scans for `ParentDir` components — but **never canonicalizes**, so a symlink inside the workspace pointing outside is not caught, and `fs_exists` uses the *write* validator (creates parent dirs as a side effect — see P1-5).
- **Note:** the read validator (`validate_path_exists`, `:19`) is correct (it canonicalizes). Only the write path is weak.
- **Fix:** reuse `athena_fs::path_validator::PathValidator::validate_write()` (it already handles the non-existent-file case by canonicalizing the parent). Replace the hand-rolled logic. Keep the `create_dir_all(parent)` only *after* validation passes.
- **Verify:** `fs_write_file` to a path behind an in-workspace symlink that targets `/tmp` is rejected; a normal new file under the workspace succeeds.

### P0-3. MCP `tools/call` executed without auth after `initialize`
- **File:** `crates/athena-core/src/mcp.rs:446` (`handle_request`) and `:563` (`handle_request_impl`).
- **Current behavior:** the token is checked **only** in the `"initialize"` arm (`:574`). `handle_request` short-circuits `tools/call` to the tool executor at `:448` *before* any token check, and `handle_request_impl`'s `tools/call` arm has no token gate. Any client that can reach the port can invoke tools without ever authenticating.
- **Fix:** track per-connection authenticated state (set true only after a valid `initialize`). Reject every non-`initialize` method (`tools/call`, `tools/list`, etc.) with `-32600` until authenticated. The auth flag lives in the per-connection handler loop, not global server state.
- **Verify:** integration test — open a connection, send `tools/call` without `initialize` → expect auth error; with valid `initialize` first → succeeds.

### P0-4. API keys handled as plain `String`, returned to frontend, leak into error text
- **Files:** `crates/athena-core/src/orchestrator.rs:50` (`ProviderConfig.api_key: String`), `:743`/`:931` (sent as headers), `:752`/`:939` (`err_text` from provider returned verbatim into `OrchestratorError::Generic`); `src-tauri/src/commands/athena.rs` (loads key into `String`); frontend settings store keeps key in a signal.
- **Why it matters:** keys are not zeroized, are reflected to the frontend, and provider error bodies (which can echo request headers / contain identifying material) are surfaced raw to the user and logs.
- **Fix (incremental, in priority order):**
  1. **Stop leaking error bodies:** sanitize `err_text` before embedding — truncate, and redact anything matching key prefixes (`sk-`, `sk-ant-`, the loaded key value). Log full detail at `debug` only.
  2. **Zeroize in memory:** wrap `api_key` in `secrecy::SecretString`; implement redacting `Debug`. (`secrecy` is not yet a dependency — add it.)
  3. **Backend-only storage:** the frontend should never receive the raw key. Store in backend (keyring/store) and reference by label; settings UI shows a masked "set / not set" state.
- **Verify:** unit test that `format!("{:?}", provider_config)` does not contain the key; manual check that a deliberately-bad key produces an error message with the key redacted.

---

## P1 — High, still open

### P1-1. `handle_request_input` blocks forever (no timeout) — agent comms
- **File:** `crates/athena-core/src/agent_comms.rs` ~`:825` (`handle_request_input`); the `sync_channel::<String>(1)` recv at the helper ~line 78 uses `input_rx.recv()`.
- **Issue:** if the frontend never answers an agent's `requestInput`, the handler thread is parked permanently; `cancel_input_request` cannot wake it.
- **Fix:** use `recv_timeout(Duration)` (e.g. 5 min default, configurable) and return a typed "timed out / cancelled" result. Wire `cancel_input_request` to drop/close the sender so recv returns `Err` promptly.
- **Verify:** test that a request with no response returns a timeout error within the bound, and that cancel unblocks it immediately.

### P1-2. `SessionManager::spawn` TOCTOU on session id
- **File:** `crates/athena-terminal/src/session.rs:175` (read-lock existence check) → fork → `:231` (`sessions.insert` under write lock, unconditional).
- **Issue:** two concurrent `spawn(id)` calls both pass the read check, both `openpty`+`fork`, second `insert` overwrites the first → leaked master fd + orphaned shell process.
- **Fix:** re-check `sessions.contains_key(&id)` under the **write** lock before insert; if present, close the just-created `master_fd`, `killpg` the just-forked child, and return the existing session. (Better: hold a single write lock across the whole critical section, or use an `entry`-style guard.)
- **Verify:** stress test spawning the same id from N tasks concurrently yields exactly one live session and no leaked fds.

### P1-3. `setsid()`/`killpg` process-group correctness
- **File:** `crates/athena-terminal/src/session.rs:201` (`setsid().ok()` — failure ignored) and `:70`/`:251` (`killpg(self.shell_pid, …)`).
- **Issue:** `killpg` assumes the child's PGID equals its PID, which is only true if `setsid()` succeeded. If it failed, the kill targets the wrong group (potentially the parent app's group).
- **Fix:** in the child, check `setsid()` result; on failure write to the failure pipe (see P1-4) and `exit`. Store the PGID explicitly in the session and use it for `killpg`.
- **Verify:** kill a session and confirm only its subtree dies; assert the stored pgid is used.

### P1-4. Zombie child on `execvp` failure
- **File:** `crates/athena-terminal/src/session.rs:213` (`execvp`) → `:214` `std::process::exit(1)`.
- **Issue:** on exec failure the child exits but the parent never `waitpid`s it → zombie. The parent also has no way to learn the spawn failed (it gets a session that immediately dies).
- **Fix:** add a `SIGCHLD` reaper using `waitpid(WNOHANG)` (or migrate this path to `tokio::process`/`portable-pty`). Use a close-on-exec pipe so the child can report exec failure to the parent, which then returns `Err` instead of a dead session.
- **Verify:** spawn with a bogus shell path → `spawn` returns `Err`, no zombie remains.

### P1-5. `fs_exists` uses the write validator (side-effecting)
- **File:** `src-tauri/src/commands/mod.rs:283` — `validate_path(path_ref).is_ok()`.
- **Issue:** `validate_path` calls `create_dir_all(parent)`. Probing existence of `a/b/c` silently creates `a/b`. Semantically wrong and a minor write-amplification / sandbox surprise.
- **Fix:** give `fs_exists` a read-only check (canonicalize + descendant test, no dir creation). Once P0-2 routes writes through `PathValidator`, add a non-creating `exists`-style helper.
- **Verify:** `fs_exists("nonexistent/deep/path")` returns false and creates nothing on disk.

### P1-6. Frontend event-listener leaks (notifications + file tree)
- **Files:**
  - `frontend/src/components/notifications/notification_bell.rs:35,77,118` — three `listen()` calls in `use_effect`, return value `let _`'d, no `use_drop`.
  - `frontend/src/components/notifications/notification_toast.rs:24` — same pattern.
  - `frontend/src/components/sidebar_dir/file_tree.rs:113` — registers a `fs:change:*` listener inside a `use_effect` whose deps re-run, so it re-subscribes repeatedly without unsubscribing.
- **Reference implementation:** `xterm_mount.rs:629/701` already does this correctly (store `unlisten` in a struct/signal, call it in `use_drop`). Mirror that.
- **Fix:** capture the `unlisten` handle from each `listen()` into a signal/struct and call it in `use_drop`. For `file_tree.rs`, ensure the listener is registered once per mount (stable effect) and torn down on unmount.
- **Verify:** mount/unmount these components repeatedly (or open/close many spaces) and confirm listener count is stable (manual: add a temporary counter log in `listen`).

### P1-7. `WorkspaceState::save()` race + `set_spaces` doesn't persist
- **File:** `frontend/src/stores/workspace.rs:105` (`save` spawns fire-and-forget `spawn_local`) and `:99` (`set_spaces` mutates without calling `save`).
- **Issue:** rapid mutations spawn overlapping async saves that can land out of order (stale overwrites newer); `set_spaces` changes are never persisted.
- **Fix:** single-pending/debounced save — track a "dirty" + "in-flight" flag (or a generation counter); coalesce so only the latest state is written, and never run two saves concurrently. Make `set_spaces` mark dirty / trigger the same debounced save.
- **Verify:** fire N rapid mutations; assert the persisted store equals the final state.

### P1-8. CSP allows `unsafe-eval`
- **File:** `src-tauri/tauri.conf.json:28` — `script-src 'self' 'unsafe-eval' 'wasm-unsafe-eval'` and `frame-src 'self' *`.
- **Issue:** WASM only needs `'wasm-unsafe-eval'`. Plain `'unsafe-eval'` permits arbitrary `eval()` in a privileged desktop shell; `frame-src 'self' *` lets any origin be framed.
- **Fix:** remove `'unsafe-eval'` (keep `'wasm-unsafe-eval'`); confirm the app still boots (Dioxus 0.7 release build should not need it). Restrict `frame-src` to the specific origins actually embedded (or `'self'` if the browser panel uses a webview, not an iframe).
- **Verify:** app launches and WASM mounts with the tightened CSP; embedded browser still works or is explicitly allowlisted.

---

## P2 — Medium (correctness / hygiene)

- **P2-1 `is_none_or` MSRV:** `orchestrator.rs:856` and `athena-browser/src/lib.rs:203,208` use `Option::is_none_or` (stabilized Rust 1.82). Fine if the toolchain is ≥1.82; otherwise replace with `map_or(true, …)`. Decide based on `rust-version` in the workspace manifest. Low risk but cheap to confirm.
- **P2-2 Leftover backup file:** `crates/athena-core/src/orchestrator.rs.bak` is untracked clutter from the remediation. Delete it (it's a near-duplicate of the live file and will confuse future audits/greps).
- **P2-3 `get_current_time_ms` / `now_ms` `unwrap_or_default()`:** `tool_executor.rs:1349`, plus `session.rs`/`notification.rs` timestamp sites. On clock error they silently yield epoch 0. Log a warning at minimum.
- **P2-4 OpenAI error path message integrity:** `orchestrator.rs:941` truncates `openai_messages` to `user_msg_index` on API error — verify the assistant message carrying `tool_calls` is not left dangling without matching tool results (would malform the next request). Add a test that an error mid-tool-loop leaves a replayable message vector.
- **P2-5 `browser` URL scheme validation:** `athena-browser/src/lib.rs` `normalize_url` — ensure `javascript:`, `file:`, `data:` schemes are blocked for externally-opened URLs (`browser_open_external`). Allowlist `http`/`https`.
- **P2-6 `validate_hook_script` cross-platform:** `crates/athena-plugins/src/lib.rs` — uses `/` separators; harden with `std::path::Path` so Windows `C:\`/`\..` are also caught. (Lower priority if macOS-only for now.)

---

## P3 — Low (quality, defer)

These are real but non-blocking; batch them when touching the relevant files:
- `highlighter.rs` line-number-prefix parsing duplicated across 8 highlighters → extract `strip_line_number_prefix`.
- Shared components that are placeholders: `error_boundary.rs` (no-op), `resizable_panel.rs` (not resizable), `context_menu.rs` (pass-through), `modal.rs` (no focus trap), `tooltip.rs` (title attr only). Track as UX debt.
- `notification_sound.rs` inline JS via `#[wasm_bindgen(inline_js=…)]` → move to a real module or a Tauri command (also helps the CSP tightening in P1-8).
- `store.rs` rewrites the whole KV file on every `set`/`delete` → add a batch/flush API if write volume grows.
- `swarm.rs` `watch_state` polling instead of `notify` file watcher; duplicate atomic-write logic.

---

## Suggested execution order

1. **P0-1, P0-2** (path sandbox) — same `PathValidator`, do together. Highest security ROI.
2. **P0-3** (MCP auth) — self-contained.
3. **P0-4** (API key leakage) — start with error-text sanitization (cheap), then `secrecy`, then backend-only storage.
4. **P1-2 / P1-3 / P1-4** (terminal process lifecycle) — same file, do together.
5. **P1-1** (agent input timeout), **P1-5** (`fs_exists`), **P1-6 / P1-7** (frontend leaks + save race), **P1-8** (CSP).
6. **P2 batch**, then **P3** as opportunistic cleanup.

## Global verification gate (run after each workstream)
```bash
cargo check --workspace
cargo test --workspace
bash frontend/build-dist.sh --debug
cargo clippy --workspace -- -D warnings   # if clippy-clean is a goal
```
Add regression tests alongside each fix (path traversal, MCP auth, spawn race, listener teardown) — the prior audit flagged the near-total absence of security/concurrency tests as a cross-cutting gap, and these fixes are the natural place to start closing it.
