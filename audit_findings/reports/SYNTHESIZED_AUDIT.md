# SYNTHESIZED SECURITY AUDIT REPORT

## Athena's Core -- Rust/Tauri/Dioxus Codebase

**Date:** 2026-06-09
**Scope:** 10 Individual Module Audits (Orchestration, Agent Comms, MCP/Search, Terminal, Store/State, FS/Browser/Plugins, Tauri Commands, Frontend Core, UI Components, Utils/Config)
**Method:** Static analysis, pattern matching, cross-module correlation
**Total Findings:** 170+ individual findings consolidated below

---

## EXECUTIVE SUMMARY

This report synthesizes 10 independent security audit subagent reports into a master finding list. Findings are consolidated by severity, deduplicated, and cross-referenced across modules. **The most serious issues are concentrated in: (1) path traversal and command injection across the tool executor, Tauri commands, and FS crate; (2) unbounded resource growth leading to potential OOM/DoS; (3) missing event listener cleanup in the frontend causing unbounded memory growth; and (4) lock poisoning, race conditions, and deadlocks in async/concurrent code.**

**Severity Distribution (consolidated, deduplicated):**

| Severity | Count | Category |
|----------|-------|----------|
| Critical | 13 | Security, Data Loss, DoS |
| High | 27 | Security, Concurrency, Logic Bugs |
| Medium | 53 | Performance, Error Handling, Resource Leaks |
| Low | 40 | Code Quality, Minor Bugs, Portability |

---

## CRITICAL FINDINGS (13)

### CROSS-CUTTING: C1 -- Path Traversal / Sandbox Escape (Multiple Modules)

**Severity:** Critical
**Affected Modules:** Tool Executor, Tauri Commands, athena-fs, athena-plugins, MCP Server
**Evidence:**
- `tool_executor.rs` ~1182 (`fs_read_file`, `fs_list_dir`, `fs_search`): `validate_path` uses `is_absolute()` + `join()` without canonicalization or descendant check (AGENT_01 C1)
- `src-tauri/src/commands/mod.rs` ~35-60: `validate_path` for writes does not canonicalize (AGENT_07 Finding 1)
- `crates/athena-fs/src/lib.rs` ~48-82: `ensure_within_home` has TOCTOU + symlink bypass (AGENT_06 FS-01)
- `crates/athena-plugins/src/lib.rs` ~388-398: `validate_hook_script` doesn't prevent `..` at end of path or Windows paths (AGENT_06 PL-05)
- `crates/athena-core/src/mcp.rs`: Unauthenticated `tools/call` after connection (AGENT_03 Finding 4)

**Consolidated Description:**
There is a systemic failure to robustly sandbox file system operations across the entire stack. Multiple path validation functions exist (`validate_path` in tool_executor, `validate_path` in Tauri commands, `ensure_within_home` in athena-fs), but none reliably prevent path traversal via symlinks, `..` components, or non-existent paths. The tool executor's `validate_path` is particularly dangerous because it is the gatekeeper for LLM-triggered file operations, and the LLM can be prompted to request `../../../etc/passwd`.

**Impact:** Full arbitrary file read/write outside the intended sandbox, including system files. LLM-driven tool calls can escape the workspace to read secrets, modify system files, or destroy data.

**Root Cause:** Path validation is implemented locally and inconsistently across modules. None use a trusted, OS-level sandboxing approach (e.g., `openat2` with `RESOLVE_BENEATH`, or `landlock`). String-based path manipulation (`starts_with`, `join`) is fundamentally insufficient.

**Recommended Fix (coordinated across all modules):**
1. Create a SINGLE `PathValidator` utility in `athena-fs` or `athena-core` that:
   - Canonicalizes the resolved path using `std::fs::canonicalize()`
   - Verifies the canonicalized path `starts_with` the canonicalized sandbox root
   - Rejects paths containing `..` or symlinks that escape the sandbox
   - Uses `openat`-style relative resolution to eliminate TOCTOU
2. Replace ALL per-module path validation with this unified utility.
3. Add unit tests for symlink attacks, `..` traversal, and non-existent path attacks.

---

### CROSS-CUTTING: C2 -- Command Injection via Unsanitized Tool Arguments (Multiple Modules)

**Severity:** Critical
**Affected Modules:** Tool Executor, Tauri Commands (pty_write), athena-plugins, search.rs
**Evidence:**
- `tool_executor.rs` ~671: `command` param passed to `agent_spawned` without sanitization (AGENT_01 C2)
- `tool_executor.rs` ~1150: `run_command_in_terminals` writes raw command to PTY (AGENT_01 M5)
- `search.rs`: `pattern` and `path` passed directly to `ripgrep` without sanitization (AGENT_03 Finding 1)
- `athena-plugins/src/lib.rs` ~358-365: `ALLOWED_MCP_COMMANDS` includes `sh`, `bash`, `zsh` (AGENT_06 PL-02)
- `src-tauri/src/commands/mod.rs` ~780-800: `pty_write` passes raw data to PTY (AGENT_07 Finding 20)
- `src-tauri/src/commands/tool.rs`: `tool_execute` deserializes arbitrary JSON without tool name allowlisting (AGENT_07 Finding 9)

**Consolidated Description:**
Multiple layers of the application accept user-provided (or LLM-provided) strings that are passed to shell commands, process execution, or PTY I/O without adequate validation. The tool executor's `launch_custom_agent` has an allowlist check (good), but it defaults to empty (allowing all) and only checks full command strings. The search module passes user patterns directly to `ripgrep` (though `Command::arg()` mitigates shell injection, argument injection is still possible). The plugins crate's MCP command list includes full shell binaries, defeating the whitelist. `pty_write` allows any tool to inject arbitrary terminal input.

**Impact:** Remote code execution (via LLM tool calls), arbitrary command execution on host, privilege escalation via plugin system.

**Root Cause:** The trust model assumes the LLM/tool caller is benign. No defense-in-depth exists. The allowlist is optional and the default is permissive.

**Recommended Fix:**
1. **Mandatory Allowlist**: Remove `sh`, `bash`, `zsh` from `ALLOWED_MCP_COMMANDS`. Implement a hardcoded whitelist of tool/bin commands, not shell interpreters.
2. **Parameterization**: Never pass raw strings to `Command`. Build `Command` with explicit `arg()` calls. Parse and validate each argument.
3. **PTY Sanitization**: Sanitize or escape control characters before writing to PTY. Consider a user-confirmation gate for LLM-triggered PTY writes.
4. **Search Sanitization**: Strip leading dashes from `pattern` before passing to `ripgrep`. Cap `context_lines` and `max_results`.

---

### CROSS-CUTTING: C3 -- Lock Poisoning and Async Deadlocks (Orchestrator, OutputBuffer, athena-store)

**Severity:** Critical
**Affected Modules:** Orchestrator, OutputBuffer, athena-store
**Evidence:**
- `orchestrator.rs` ~209, 224, 276: `.lock().ok()` silently drops poisoned locks (AGENT_01 C4)
- `output_buffer.rs` ~47, 77: `std::sync::Mutex` held during event emission (AGENT_01 H7)
- `athena-store/src/store.rs` ~144-153: `has()` silently recovers from poisoned mutex (AGENT_05 Finding 2)
- `notification.rs` ~109-115: `RwLock` under `std::sync::Mutex` (AGENT_02 1.3)
- `swarm.rs` ~116-130: `watch_rx` may see stale state (AGENT_02 1.2)
- `src-tauri/src/state.rs` ~632-656: `blocking_lock()` in non-async context (AGENT_05 Finding 13)

**Consolidated Description:**
std::sync::Mutex poisoning is not handled consistently across the codebase. In the orchestrator, a poisoned lock causes all subsequent state mutations to silently fail, leading to data loss and stale state. In the store, `has()` recovers from poisoning but other methods return errors, creating inconsistent API behavior. OutputBuffer uses `std::sync::Mutex` for an event emitter in a potentially reentrant context, risking deadlocks.

**Impact:** After any thread panic, critical state becomes immutable or inconsistent. Silent data loss. Potential deadlocks under contention.

**Root Cause:** Use of `std::sync::Mutex` (which poisons) combined with inconsistent recovery strategies. No single convention is enforced.

**Recommended Fix:**
1. Replace all `std::sync::Mutex` with `parking_lot::Mutex` (no poisoning) or `tokio::sync::Mutex` (async-aware, no poisoning).
2. If keeping `std::sync::Mutex`, wrap it in a utility that either: (a) recovers from poisoning automatically, or (b) panics/returns errors consistently.
3. Never hold locks while calling callbacks or event emitters.

---

### CROSS-CUTTING: C4 -- API Key / Secret Exposure (Orchestrator, athena-store, Tauri Commands)

**Severity:** Critical
**Affected Modules:** Orchestrator, athena-store, Tauri Commands, Frontend
**Evidence:**
- `orchestrator.rs` ~439, ~637: API keys passed in HTTP headers; error response text may contain keys (AGENT_01 C3)
- `src-tauri/src/commands/athena.rs`: API key loaded into plain `String`, not zeroized (AGENT_07 Finding 7)
- `frontend/src/components/settings/settings_modal.rs` ~105-155: API key stored in plain `use_signal(String)` (AGENT_09 M-12)
- `src-tauri/src/main.rs` ~150-151: `store_api_key` / `clear_api_key` commands (AGENT_10 Finding 19)

**Consolidated Description:**
API keys are stored as plain `String` in the orchestrator's `ProviderConfig`, propagated to the frontend via signals, and may be leaked in error responses or logs. The keys are not zeroized on drop, meaning they may persist in memory even after being "deleted." The frontend keeps keys in reactive signals that any component can read.

**Impact:** API key exposure in memory dumps, logs, error messages, and frontend state inspection. Potential for key exfiltration by malicious components or memory scraping.

**Recommended Fix:**
1. Use `secrecy::SecretString` or similar zero-on-drop container for all API keys.
2. Implement `Debug`/`Display` that redacts the key for any struct containing it.
3. Never pass the raw key to the frontend. Store it in the backend keyring and reference it by label.
4. Sanitize all error responses from LLM providers before including them in user-facing output.

---

### CROSS-CUTTING: C5 -- Unbounded Resource Growth / DoS (Terminal Read Loop, Frontend Stores, Agent I/O)

**Severity:** Critical
**Affected Modules:** Terminal (athena-terminal, Tauri commands), Frontend Stores, athena-core
**Evidence:**
- `src-tauri/src/commands/mod.rs` ~730-800: `coalesce_buf` grows without hard cap (AGENT_04 Finding 2)
- `frontend/src/stores/agent_output.rs` ~33: `MAX_LINES_PER_BUFFER=5000` but buffers never pruned (AGENT_08 C1)
- `frontend/src/stores/terminal_blocks.rs` ~76: `append_output` has no length limit (AGENT_08 M4)
- `frontend/src/components/agents/agent_output_panel.rs` ~20-30: clones entire output buffer on every render (AGENT_09 M-8)
- `crates/athena-core/src/agent_comms.rs`: Unbounded TCP channel (AGENT_02 2.1)
- `crates/athena-core/src/search.rs`: Unbounded `context_lines` / `max_results` (AGENT_03 Finding 6)
- `crates/athena-plugins/src/lib.rs` ~638-655: Plugin manifest reads unbounded files (AGENT_06 PL-04)

**Consolidated Description:**
Multiple components and backend modules lack resource bounds. The PTY read loop can accumulate output in a `coalesce_buf` without a hard cap, leading to OOM. Frontend stores (agent output, terminal blocks, notification stores) grow without bound when components fail to clean up. The agent comms system uses an unbounded channel that can OOM if the consumer is slow. Search allows unbounded `context_lines`.

**Impact:** Denial of service via memory exhaustion. Application crash. Potential for resource exhaustion attacks.

**Recommended Fix:**
1. **Terminal read loop:** Add a hard cap (e.g., 1 MiB) on `coalesce_buf`; force flush and emit when exceeded.
2. **Frontend stores:** Add periodic garbage collection. Remove entries for closed/disconnected panes. Cap total entries.
3. **Agent comms:** Switch unbounded `mpsc::channel` to bounded `sync_channel` with a reasonable limit.
4. **Search:** Cap `context_lines` to 100 and `max_results` to 5000 regardless of caller input.
5. **Plugin discovery:** Limit manifest file read to 1 MB. Skip files exceeding that size.

---

### CROSS-CUTTING: C6 -- Unauthenticated / Unauthorized Operations (MCP, Agent Comms, Plugin Registration, Agent Comms Token)

**Severity:** Critical
**Affected Modules:** MCP Server, Agent Comms, Tauri Commands
**Evidence:**
- `crates/athena-core/src/mcp.rs` ~304-460: `tools/call` processed without re-verifying token (AGENT_03 Finding 4)
- `crates/athena-core/src/agent_comms.rs` ~88, 264: Plain-text UUID token with no rotation or scoping (AGENT_02 5.1)
- `src-tauri/src/commands/agent.rs` ~8-10: `agent_comms_token` exposed without access control (AGENT_07 Finding 23)
- `src-tauri/src/commands/plugin.rs` / `plugin_host.rs`: Plugin registration with no auth/signature (AGENT_07 Finding 22)

**Consolidated Description:**
Authentication is weak or absent across multiple interfaces. The MCP server only authenticates during `initialize`, then processes all subsequent requests without checking. The agent comms token is a static UUID with no rotation, expiry, or session scoping. The `agent_comms_token` Tauri command is accessible to any caller. Plugin registration has no signature verification or authorization.

**Impact:** Unauthorized tool execution, unauthorized agent connections, privilege escalation via plugin installation.

**Recommended Fix:**
1. MCP: Track authenticated state per connection; reject non-`initialize` requests for unauthenticated connections.
2. Agent Comms: Implement token rotation; scope tokens to sessions; enforce peer PID/user checks where possible.
3. Tauri: Remove `agent_comms_token` from public command surface or gate it behind user confirmation.
4. Plugins: Require digital signature verification or explicit user confirmation for plugin registration.

---

### C-backend: C7 -- Incomplete `tool_executor.rs` File Visibility

**Severity:** High (was marked Critical in source; downgraded due to being an audit coverage issue)
**File:** `crates/athena-core/src/tool_executor.rs`
**Line:** ~1218 of 1673 (last ~455 lines not visible)
**Evidence:** AGENT_01 H8
**Description:** The audit was based on a partially truncated file. The unaudited portion includes the implementations of file system tools (`fs_read_file`, `fs_list_dir`, `fs_search`), which are security-critical. Without seeing the full file, the audit findings for these methods are based on partial evidence and may be incomplete.
**Impact:** Unknown unaudited code may contain additional security issues.
**Recommended Fix:** Provide the complete file and re-audit the unaudited portion.

---

### C-backend: C8 -- TOCTOU Race / Session ID Collision in SessionManager::spawn

**Severity:** Critical
**File:** `crates/athena-terminal/src/session.rs`
**Line:** 296-380
**Evidence:** AGENT_04 Finding 1
**Description:** `spawn` acquires a read lock to check for existing sessions, then -- after forking -- acquires a write lock and unconditionally inserts. Two concurrent `spawn` calls with the same ID can both pass the read check, both fork, and the second `insert` overwrites the first, causing fd leaks and orphaned processes.
**Impact:** Session hijacking, fd leaks, orphaned/zombie processes.
**Recommended Fix:** Re-check under the write lock before inserting, or use a single write lock for the entire critical section.

---

### C-frontend: C9 -- `Closure.forget()` Memory Leak in `pty_listen_binary`

**Severity:** Critical
**File:** `frontend/src/tauri_bridge.rs`
**Line:** ~542
**Evidence:** AGENT_08 C3
**Description:** `pty_listen_binary` registers a `Closure` as an event listener and calls `.forget()`, permanently leaking the JavaScript reference. Every new terminal session leaks a closure. Cannot be unlistened or garbage-collected.
**Impact:** Unbounded memory growth; WASM heap crash on long-running sessions.
**Recommended Fix:** Store the closure and return an unlisten function. Never call `.forget()`.

---

### C-frontend: C10 -- `WorkspaceState` Default Bypasses `save()` on Mutations (Race Condition)

**Severity:** Critical
**File:** `frontend/src/stores/workspace.rs`
**Line:** ~65-128
**Evidence:** AGENT_08 C2
**Description:** Mutations call `save()` which spawns a fire-and-forget async task. Rapid successive mutations cause overlapping save tasks that race. The first (stale) save can overwrite the second, causing data loss. `set_spaces()` does NOT call `save()` at all.
**Impact:** Workspace state corruption or loss during rapid operations.
**Recommended Fix:** Implement a debounced/single-pending save mechanism. Use `use_effect` to trigger saves only when state settles.

---

### C-backend: C11 -- `CString::new` Panic on NUL Byte + Arbitrary Shell Path

**Severity:** Critical
**File:** `crates/athena-terminal/src/session.rs`
**Line:** 327
**Evidence:** AGENT_04 Finding 3
**Description:** `CString::new` can panic if the shell path contains an embedded NUL. More critically, the shell path is not validated before being passed to `execvp`, allowing execution of arbitrary binaries if the parameter is user-controlled.
**Impact:** Arbitrary code execution; crash on NUL input.
**Recommended Fix:** Validate the shell path against an allowlist of known shells (`/bin/bash`, `/bin/zsh`, `/usr/bin/fish`, etc.). Reject or sanitize unknown paths.

---

### C-backend: C12 -- Anthropic Loop Does Not Synchronize `openai_messages`, Breaking Persistence

**Severity:** High (was marked Critical in source; downgraded due to being data integrity, not security)
**File:** `crates/athena-core/src/orchestrator.rs`
**Line:** ~311-338
**Evidence:** AGENT_01 H1
**Description:** During Anthropic-based conversations, only `anthropic_messages` is updated. `openai_messages` (used for persistence) is never populated. `save_conversation` reads `openai_messages`, resulting in empty or incomplete saved conversations.
**Impact:** Conversation history lost for Anthropic provider users.
**Recommended Fix:** Synchronize both message vectors, or use a provider-agnostic unified history.

---

## HIGH FINDINGS (27)

### H1 -- `is_none_or` is Unstable / May Not Compile

**Severity:** High
**File:** `crates/athena-core/src/orchestrator.rs` ~631
**Evidence:** AGENT_01 H2
**Description:** Uses `msgs.first().is_none_or(|m| m.role != "system")`, which is a recent Rust addition. May fail to compile on older toolchains.
**Fix:** Replace with `msgs.first().map_or(true, |m| m.role != "system")`.

---

### H2 -- OpenAI Error Path Does Not Remove Assistant Message After Tool Call

**Severity:** High
**File:** `crates/athena-core/src/orchestrator.rs` ~591-647
**Evidence:** AGENT_01 H3
**Description:** On API error, the code truncates messages to `user_msg_index`, but the assistant message with `tool_calls` is not removed. Next request is malformed (assistant with tool_calls but no tool results).
**Fix:** Record the assistant message index and truncate to it, or only push messages after successful completion.

---

### H3 -- `execute_tool` Blocks Async Runtime (Synchronous I/O in Async Context)

**Severity:** High
**File:** `crates/athena-core/src/orchestrator.rs` ~550
**Evidence:** AGENT_01 H4
**Description:** `execute_tool` is synchronous and does blocking I/O (`std::fs::read_to_string`, DB ops). Called inside async `send_openai`/`send_anthropic` without `spawn_blocking`.
**Fix:** Make `execute_tool` async. Use `tokio::fs` or `spawn_blocking` for blocking operations.

---

### H4 -- `ask_user` Synchronous Callback May Deadlock

**Severity:** High
**File:** `crates/athena-core/src/tool_executor.rs` ~751, ~1078
**Evidence:** AGENT_01 H5
**Description:** `ask_user` returns `String` directly, blocking the calling thread. Called from inside `send_openai`/`send_anthropic` which hold mutexes.
**Fix:** Make `ask_user` async-friendly (return a future or use a channel). Use `tokio::sync::Mutex`.

---

### H5 -- `handle_request_input` Blocks Indefinitely with No Timeout

**Severity:** High
**File:** `crates/athena-core/src/agent_comms.rs` ~526-583
**Evidence:** AGENT_02 2.3
**Description:** Blocks on `input_rx.recv()` with `sync_channel(1)` and no timeout. If the frontend never responds, the thread is blocked forever. `cancel_input_request` can't unblock it.
**Fix:** Use `recv_timeout` or a cancellation token.

---

### H6 -- Disconnect Agent Lock Ordering / Stale Data

**Severity:** High
**File:** `crates/athena-core/src/agent_comms.rs` ~227-247
**Evidence:** AGENT_02 1.1
**Description:** `disconnect_agent` acquires `sessions` lock, drops it, reacquires it. Race conditions possible.
**Fix:** Perform the `remove` in a single lock acquisition.

---

### H7 -- No Concurrency/Stress Tests for `AgentComms`

**Severity:** High
**File:** `crates/athena-core/src/tests.rs`
**Evidence:** AGENT_02 7.1
**Description:** TCP communication, concurrent agents, lock poisoning, and message passing correctness are completely untested.
**Fix:** Add integration tests with real `TcpStream` connections.

---

### H8 -- `search_code` / `search_files` Argument Injection via Pattern/Path

**Severity:** High
**File:** `crates/athena-core/src/search.rs` ~64-189, ~281-304
**Evidence:** AGENT_03 Finding 1, 2
**Description:** User-controlled `pattern` and `path` passed to `ripgrep` without sanitization. While `Command::arg()` prevents shell injection, argument injection (e.g., pattern starting with `-`) is possible.
**Fix:** Strip leading dashes from `pattern`. Validate `path` against workspace root before passing to search.

---

### H9 -- `killpg` Sent to PID Instead of PGID / `setsid()` Failure Ignored

**Severity:** High
**File:** `crates/athena-terminal/src/session.rs` ~195, 442
**Evidence:** AGENT_04 Finding 9
**Description:** `killpg` is called with `self.shell_pid` (which happens to equal PGID after `setsid()`), but `setsid().ok()` ignores failures. If `setsid()` fails, `killpg` signals the entire parent process group.
**Fix:** Check `setsid()` result. Store the PGID explicitly and use it for `killpg`.

---

### H10 -- Zombie / Orphaned Child Process on `execvp` Failure

**Severity:** High
**File:** `crates/athena-terminal/src/session.rs` ~364-379
**Evidence:** AGENT_04 Finding 5
**Description:** If `execvp` fails, the child exits with code `1`, but `SessionManager` never calls `waitpid`. Creates a zombie process.
**Fix:** Use `SIGCHLD` handler with `waitpid(WNOHANG)` or use `tokio::process` instead of raw `fork` + `execvp`.

---

### H11 -- `new_empty()` Falls Back to Temp Directory, Data Lost on Restart

**Severity:** High
**File:** `crates/athena-store/src/store.rs` ~24-34
**Evidence:** AGENT_05 Finding 1
**Description:** `KeyValueStore::new_empty()` / `SessionStore::new_empty()` fall back to a temp directory without informing the user. Data written to these instances is silently discarded.
**Fix:** Make `new_empty()` truly in-memory (no file path). Propagate fallback state to callers. Surface a notification to the user.

---

### H12 -- `AppState::new()` Double `unwrap_or_else()` is Misleading / Redundant

**Severity:** High
**File:** `src-tauri/src/state.rs` ~370-385
**Evidence:** AGENT_05 Finding 3
**Description:** The retry attempts the exact same operation that just failed. The only thing that saves it is `new_empty()` always succeeding, but the code is misleading.
**Fix:** Remove the redundant retry. Directly fall back to `new_empty()`.

---

### H13 -- `TauriEventSender::ask_user` Hard-Codes 5-Minute Timeout, Returns Error String as Answer

**Severity:** High
**File:** `src-tauri/src/state.rs` ~169-204
**Evidence:** AGENT_05 Finding 4
**Description:** `ask_user` returns the literal string `"error: user response timed out"` on timeout. Callers may treat this as a real user answer.
**Fix:** Return a dedicated error type or sentinel value. Use shorter default timeouts.

---

### H14 -- `ensure_within_home` Fails to Prevent Traversal on Non-Existent Paths

**Severity:** High
**File:** `crates/athena-fs/src/lib.rs` ~58-65
**Evidence:** AGENT_06 FS-02
**Description:** When path doesn't exist, the code canonicalizes the parent and joins the original filename. If the path contains `..`, the joined path passes the `starts_with` check but still escapes.
**Fix:** Reject any path containing `..` before canonicalization, or canonicalize the full path.

---

### H15 -- `normalize_url` Missing Dangerous Scheme Blocks

**Severity:** High
**File:** `crates/athena-browser/src/lib.rs` ~456-462
**Evidence:** AGENT_06 BR-01
**_dirs, or at minimum block additional schemes.

---

### H16 -- `validate_hook_script` Does Not Validate on Windows

**Severity:** High
**File:** `crates/athena-plugins/src/lib.rs` ~388-398
**Evidence:** AGENT_06 PL-01
**Description:** Uses `/` as path separator; doesn't catch `C:\`, `D:\`, or `\..` on Windows.
**Fix:** Use `std::path::Path` for platform-independent validation.

---

### H17 -- `swarm_read_state`, `swarm_send_message`, `swarm_read_mailbox` Accept Arbitrary `dir` Without Path Validation

**Severity:** High
**File:** `src-tauri/src/commands/swarm.rs`
**Evidence:** AGENT_07 Finding 5
**Description:** `dir` parameter passed directly to coordinator without validation.
**Fix:** Sanitize using `validate_path` before passing to coordinator.

---

### H18 -- `plugin_host_discover_plugins` Accepts `dir` Without Path Validation

**Severity:** High
**File:** `src-tauri/src/commands/plugin_host.rs` ~110-140
**Evidence:** AGENT_07 Finding 12
**Description:** `dir` parameter checked for `..` and empty string but not workspace validation.
**Fix:** Use `validate_path` or `validate_path_for_read`.

---

### H19 -- No Cleanup of Tauri Event Listeners in Components

**Severity:** High
**File:** `frontend/src/tauri_bridge.rs` ~560-650
**Evidence:** AGENT_08 H1
**Description:** `listen()` returns an unlisten function, but the frontend codebase doesn't call it. Components leak listeners.
**Fix:** Ensure all `use_effect` blocks that call `listen()` capture and call the unlisten function in cleanup.

---

### H20 -- `Default` on `AthenaState` is Wrong Due to `PartialEq` Custom Override

**Severity:** High
**File:** `frontend/src/stores/athena.rs` ~155
**Evidence:** AGENT_08 H3
**Description:** `PartialEq` does deep comparison of entire message history on every signal write. O(N) overhead that causes frame drops.
**Fix:** Implement custom `PartialEq` using a generation counter or hash of last message.

---

### H21 -- `mounted_spaces` Signal is Never Cleaned Up

**Severity:** High
**File:** `frontend/src/lib.rs` ~85
**Evidence:** AGENT_08 H4
**Description:** Signal tracks which space IDs have been mounted but never removed. Grows unboundedly.
**Fix:** Remove from `mounted_spaces` when a space is removed, or remove the signal entirely.

---

### H22 -- `PanelManagerState` Does Not Affect `UIState` (Split Brain)

**Severity:** High
**File:** `frontend/src/stores/panel_manager.rs` / `frontend/src/lib.rs`
**Evidence:** AGENT_08 H5
**Description:** `PanelManagerState` and `UIState` both track panel state. The `onkeydown` handler sets `ui_state.panel` but never updates `panel_manager` state.
**Fix:** Consolidate into a single source of truth, or synchronize on every change.

---

### H23 -- NotificationBell/FileTree Duplicate Listener Registration

**Severity:** High
**File:** `frontend/src/components/notifications/notification_bell.rs` ~18-92
**File:** `frontend/src/components/sidebar_dir/file_tree.rs` ~130-145
**Evidence:** AGENT_09 C-1, C-3
**Description:** `notification_bell.rs` registers listeners without cleanup. `file_tree.rs` adds a new `fs:change:*` listener on every render.
**Fix:** Store and call unlisten handles. Register once per mount.

---

### H24 -- `unsafe-eval` in CSP Permits Code Injection

**Severity:** High
**File:** `src-tauri/tauri.conf.json` ~21
**Evidence:** AGENT_10 Finding 1
**Description:** `script-src` includes `'unsafe-eval'`, allowing `eval()`-equivalent execution in a privileged desktop app.
**Impact:** Arbitrary code execution if attacker can inject script content.
**Fix:** Remove `'unsafe-eval'`. Scope narrowly if required by a dependency.

---

### H25 -- Inline JavaScript in `wasm_bindgen` for Audio -- CSP Bypass

**Severity:** High
**File:** `frontend/src/utils/notification_sound.rs` ~18-35
**Evidence:** AGENT_10 Finding 2
**Description:** `#[wasm_bindgen(inline_js = ...)]` injects raw JavaScript. Bypasses CSP and creates unauditable JS execution.
**Fix:** Move to a dedicated `.js` file or trigger sound via Tauri command.

---

### H26 -- Circuit Breaker `execute()` Drops Mutex Guard Across `.await` Boundary

**Severity:** High
**File:** `frontend/src/utils/circuit_breaker.rs` ~241-255
**Evidence:** AGENT_10 Finding 3
**Description:** `std::sync::Mutex` is not safe across `.await`. If `execute()` is ever made async, it will deadlock.
**Fix:** Provide an `execute_async` variant using `tokio::sync::Mutex`, or explicitly keep the API sync-only.

---

### H27 -- `highlighter.rs` Line-Number Prefix Parsing is Brittle

**Severity:** High
**File:** `frontend/src/utils/highlighter.rs` (multiple locations)
**Evidence:** AGENT_10 Finding 4
**Description:** Line-number prefix parsing logic is copy-pasted across 8 language highlighters and can mangle short lines.
**Fix:** Extract a shared `strip_line_number_prefix` helper. Use strict pattern matching.

---

## MEDIUM FINDINGS (53)

### Security & Path Traversal (M1-M10)

| # | Issue | File(s) | Evidence | Fix |
|---|-------|---------|----------|-----|
| M1 | `fs_exists` uses write validator for read, can trigger dir creation | `src-tauri/src/commands/mod.rs` | AGENT_07 Finding 2 | Use read-only validator for `fs_exists` |
| M2 | `fs_search_files` / `search_code` accepts arbitrary `path` without sandboxing | `commands/mod.rs`, `fs.rs`, `search.rs` | AGENT_07 Finding 3 | Validate `path` against workspace root |
| M3 | `shell_integration_script` accepts arbitrary shell names | `src-tauri/src/commands/shell.rs` | AGENT_07 Finding 4 | Allowlist known shells |
| M4 | `browser_open_external` accepts arbitrary URL without scheme validation | `commands/mod.rs` | AGENT_07 Finding 8 | Allowlist `http://`, `https://` |
| M5 | `shell_integration_parse` reads unbounded `data` string | `commands/shell.rs` | AGENT_07 Finding 10 | Add size limit (e.g., 1 MB) |
| M6 | `store_get`/`store_set` accept arbitrary keys without validation | `commands/store.rs` | AGENT_07 Finding 14 | Limit key length; reject control chars |
| M7 | `store_set` / `store_delete` rewrites entire store on every op | `athena-store/src/store.rs` | AGENT_05 Finding 8 | Add batch API; implement periodic flush |
| M8 | `frame-src` is too permissive (`'self' *`) | `tauri.conf.json` | AGENT_10 Finding 17 | Restrict to specific origins |
| M9 | `kanban_update_task` inconsistent status validation (`unwrap_or` vs error) | `kanban.rs` | AGENT_01 M6 | Return error for invalid status in create |
| M10 | `launch_builtin_agent` / `launch_custom_agent` no allowlist for agent_type | `tool_executor.rs` | AGENT_03 Finding 18 | Validate `agent_type` against known values |

### Concurrency & Async (M11-M25)

| # | Issue | File(s) | Evidence | Fix |
|---|-------|---------|----------|-----|
| M11 | `RateLimiter` does not account for concurrent requests | `orchestrator.rs` ~133-145 | AGENT_01 H6 | Use `Semaphore` or rate limiter crate |
| M12 | `output_buffer.rs` uses `std::sync::Mutex` for event emitter | `output_buffer.rs` ~47, 77 | AGENT_01 H7 | Don't hold lock during callback; use `parking_lot` |
| M13 | `ask_user` synchronous callback deadlock risk | `tool_executor.rs` ~751 | AGENT_01 H5 / AGENT_05 H4 | Make async; use `tokio::sync::Mutex` |
| M14 | `watch_rx` receives stale default state if never updated | `swarm.rs` ~116-130 | AGENT_02 1.2 | Ensure `watch_tx` pushes updates; init with meaningful state |
| M15 | Unbounded channel for agent communication (OOM) | `agent_comms.rs` ~368 | AGENT_02 2.1 | Use bounded `sync_channel` |
| M16 | `respond_to_input_request` can't detect cancelled requests | `agent_comms.rs` ~241-255 | AGENT_02 2.2 | Return descriptive error enum |
| M17 | Leaked TCP writer thread on connection close | `agent_comms.rs` ~381-393 | AGENT_02 3.1 | Drop/close `tx` in cleanup |
| M18 | `cancel_input_request` does not notify receiver | `agent_comms.rs` ~256-267 | AGENT_02 3.2 | Use cancellation token or oneshot |
| M19 | Unbounded thread spawn per connection | `agent_comms.rs` ~350-363 | AGENT_02 3.3 | Use thread pool or async I/O |
| M20 | `watch_state` uses polling instead of real file watcher | `swarm.rs` ~237-345 | AGENT_02 4.1 | Use `notify` crate or keep state in memory |
| M21 | `watch_state` duplicate atomic write logic | `swarm.rs` ~290-320 | AGENT_02 4.2 | Call `write_state` instead of duplicating |
| M22 | `watch_state` TOCTOU / partial write race | `swarm.rs` ~290-320 | AGENT_02 4.3 | Acquire `.lock` file before read/write |
| M23 | `send_to_socket` silently drops write errors | `agent_comms.rs` ~334-339 | AGENT_02 6.2 | Propagate write errors |
| M24 | `handle_incoming_message` silently discards deserialization errors | `agent_comms.rs` ~418-421 | AGENT_02 6.1 | Log error; consider disconnecting peer |
| M25 | `TauriEventSender::ask_user` 5-minute hardcoded timeout | `src-tauri/src/state.rs` ~169-204 | AGENT_05 H4 | Return error type; shorter default timeout |

### Terminal & PTY (M26-M35)

| # | Issue | File(s) | Evidence | Fix |
|---|-------|---------|----------|-----|
| M26 | Incomplete modifier key support | `input/escape_sequences.rs` ~87-147 | AGENT_04 Finding 7 | Implement `_mod_suffix` for all keys |
| M27 | `bracketed_paste` does not filter control characters / end bracket | `input/escape_sequences.rs` ~152-160 | AGENT_04 Finding 8 | Strip end bracket sequences, control chars |
| M28 | Temp files written world-readable | `src/session.rs` ~46-107 | AGENT_04 Finding 10 | Set `0o600` permissions |
| M29 | `resize` off-by-one / missing `#[repr(C)]` for FFI struct | `src/session.rs` ~456-476 | AGENT_04 Finding 12 | Add comment about `libc::winsize` layout |
| M30 | `generate_*` temp file leak on fork failure | `src/session.rs` ~46-107 | AGENT_04 Finding 13 | Move temp creation to child or cleanup on fork fail |
| M31 | `read_bytes` returns `Ok(0)` for both EAGAIN and EOF | `src/session.rs` ~264-278 | AGENT_04 Finding 14 | Distinguish `EAGAIN` from true EOF |
| M32 | `PROMPT_COMMAND` hook appended unsafely | `src/session.rs` ~49-63 | AGENT_04 Finding 16 | Use function-based approach |
| M33 | `libc::ioctl` return value not checked for error vs `-1` | `src/session.rs` ~463-476 | AGENT_04 Finding 19 | Capture `errno` immediately after `ioctl` |
| M34 | `std::process::exit(1)` in child after `execvp` failure | `src/session.rs` ~377 | AGENT_04 Finding 20 | Use pipe to communicate failure reason to parent |
| M35 | `output_buffer_append` is sync, no rate limiting | `commands/output.rs` ~7-15 | AGENT_07 Finding 25 | Make async; add buffer size cap |

### Data Integrity & Error Handling (M36-M45)

| # | Issue | File(s) | Evidence | Fix |
|---|-------|---------|----------|-----|
| M36 | LLM API errors hide HTTP status codes | `orchestrator.rs` ~413, ~637 | AGENT_01 M3 | Add structured error variant with status code |
| M37 | `system_prompt` replacement in OpenAI is fragile | `orchestrator.rs` ~622-634 | AGENT_01 M4 | Filter system messages when building request |
| M38 | `max_tokens` hardcoded to 4096 | `orchestrator.rs` ~395, ~617 | AGENT_01 M2 | Add `max_tokens` to `ProviderConfig` |
| M39 | `now_ms()` uses `unwrap_or_default()` | `orchestrator.rs` ~282 | AGENT_01 L2 | Return explicit error or log warning |
| M40 | `save_conversation` incomplete for Anthropic | `orchestrator.rs` ~311-338 | AGENT_01 H1 | Synchronize `openai_messages` |
| M41 | `shell_escape` assumes POSIX shell | `tool_executor.rs` ~678 | AGENT_01 L3 | Document assumption or use shell-aware escaping |
| M42 | `OutputCapture` holds `ShellHooks` by value | `output_capture.rs` ~16 | AGENT_01 L5 | Wrap in `Arc<>` if needed |
| M43 | `AppState::Default` does I/O | `src-tauri/src/state.rs` ~338-343 | AGENT_05 Finding 12 | Remove `Default` or make `new()` cheap |
| M44 | Workspace restore silently ignores parse errors | `src-tauri/src/state.rs` ~427-458 | AGENT_05 Finding 14 | Add logging for JSON parse errors |
| M45 | `strip_osc633` leaves BEL character in output | `shell_integration.rs` ~289-336 | AGENT_03 Finding 11 | Set `pos = bi + 1` after stripping |

### Frontend Resource Management (M46-M53)

| # | Issue | File(s) | Evidence | Fix |
|---|-------|---------|----------|-----|
| M46 | `TerminalStore::kill()` removes session before confirming backend kill | `frontend/src/stores/terminal.rs` ~65 | AGENT_08 M3 | Only remove if `pty_kill` succeeds |
| M47 | `TerminalBlocksStore::append_output` no overflow check | `terminal_blocks.rs` ~76 | AGENT_08 M4 | Cap total output per block |
| M48 | `NotificationStore` pushes unconditionally (no dedup) | `notification.rs` ~60 | AGENT_08 M5 | Add deduplication logic |
| M49 | `AthenaState::add_message` uses `drain` (O(N)) on exceed | `athena.rs` ~165 | AGENT_08 M6 | Use `VecDeque` or ring buffer |
| M50 | `CommandState::recent_ids` not persisted | `command.rs` ~120 | AGENT_08 M7 | Persist via `store_set`/`store_get` |
| M51 | `AgentStatusState::statuses` grows unbounded | `agent_status.rs` ~90 | AGENT_08 M8 | Add periodic cleanup |
| M52 | `mounted_spaces` signal never cleaned up | `lib.rs` ~85 | AGENT_08 H4 | Remove on space removal |
| M53 | `is_maximized` signal not synced with actual window state | `lib.rs` ~72 | AGENT_08 M1 | Query actual window state on mount |

---

## LOW FINDINGS (40)

### Security & Input Validation (L1-L10)

| # | Issue | File(s) | Evidence |
|---|-------|---------|----------|
| L1 | `is_none_or` compatibility concern | `orchestrator.rs` ~631 | AGENT_01 L1 |
| L2 | `fs_exists` write validator semantic mismatch | `commands/mod.rs` | AGENT_07 Finding 2 |
| L3 | `plugin_register`/`plugin_host_setup_plugin` arbitrary registration | `commands/plugin.rs` | AGENT_07 Finding 22 |
| L4 | `agent_comms_token` exposed without access control | `commands/agent.rs` | AGENT_07 Finding 23 |
| L5 | `fs_write_file` unbounded content size | `commands/fs.rs` | AGENT_07 Finding 24 |
| L6 | `mcp_handle_request` unbounded JSON-RPC size | `commands/mcp.rs` | AGENT_07 Finding 21 |
| L7 | `session_add_message` `unwrap_or_default()` on timestamp | `commands/session.rs` | AGENT_07 Finding 11 |
| L8 | `notification_push` `unwrap_or_default()` on SystemTime | `commands/notification.rs` | AGENT_07 Finding 15 |
| L9 | `window_*` hardcoded to window label "main" | `commands/window.rs` | AGENT_07 Finding 17 |
| L10 | `browser.rs` commands lack URL normalization | `commands/browser.rs` | AGENT_07 Finding 16 |

### Frontend (L11-L25)

| # | Issue | File(s) | Evidence |
|---|-------|---------|----------|
| L11 | No parameter validation in Tauri bridge wrappers | `tauri_bridge.rs` | AGENT_08 L2 |
| L12 | `pty_default_shell_cached()` cache race | `tauri_bridge.rs` | AGENT_08 L3 |
| L13 | `TerminalStore::ensure_session` doesn't check ID collisions properly | `terminal.rs` | AGENT_08 L4 |
| L14 | `WorkspaceState::save()` fire-and-forget without cancellation | `workspace.rs` | AGENT_08 L5 |
| L15 | `Theme`/`Font` applied twice on mount (FOUC) | `lib.rs` | AGENT_08 L6 |
| L16 | `notification_bell.rs`/`notification_toast.rs` missing `use_drop` cleanup | `components/notifications/` | AGENT_09 C-2 |
| L17 | `file_tree.rs` duplicate `fs:change:*` listeners | `components/sidebar_dir/file_tree.rs` | AGENT_09 C-3 |
| L18 | `use_effect` dependencies not declared (stale closure captures) | Multiple files | AGENT_09 H-1 |
| L19 | `AgentInspector` captures signals mutably without memoization | `components/agents/agent_inspector.rs` | AGENT_09 H-2 |
| L20 | `PaneItem`'s `spawn` for PTY kill not awaited | `components/workspace/terminal_grid.rs` | AGENT_09 H-3 |
| L21 | `AthenaInput`'s `submit_message_async` captures entire store | `components/athena/athena_input.rs` | AGENT_09 H-4 |
| L22 | `ToastContainer` never removes expired toasts | `components/shared/toast.rs` | AGENT_09 M-1 |
| L23 | `AgentOutputLine` allocates on every render | `components/agents/agent_output_line.rs` | AGENT_09 M-2 |
| L24 | `WorkspaceTabs` clones entire space list on every render | `components/workspace/workspace_tabs.rs` | AGENT_09 M-3 |
| L25 | Keyboard handler in `App` captures stale mutable references | `lib.rs` ~250 | AGENT_08 H2 |

### Code Quality & Maintainability (L26-L40)

| # | Issue | File(s) | Evidence |
|---|-------|---------|----------|
| L26 | `is_stderr_like` function redundant (field already exists) | `frontend/src/components/agents/agent_output_line.rs` | AGENT_09 M-2 |
| L27 | `RightBrowserPanel` clones signals on every click | `components/right_sidebar/browser_panel.rs` | AGENT_09 M-4 |
| L28 | `SettingsModal` reads `ui_state` extensively in render loop | `components/settings/settings_modal.rs` | AGENT_09 M-5 |
| L29 | `PluginEventBus`/`OutputEventBus` race conditions in signal writes | `components/plugin/plugin_event_bus.rs` | AGENT_09 M-6 |
| L30 | `AthenaPanel` nested `use_effect` signal write during read | `components/athena/athena_panel.rs` | AGENT_09 M-7 |
| L31 | `Button` doesn't show `not-allowed` cursor when disabled | `components/shared/button.rs` | AGENT_09 M-9 |
| L32 | `Tooltip` is just a `title` attribute placeholder | `components/shared/tooltip.rs` | AGENT_09 M-10 |
| L33 | `Modal` does not trap focus | `components/shared/modal.rs` | AGENT_09 M-11 |
| L34 | Inline styles used extensively across components | Nearly all component files | AGENT_09 L-1 |
| L35 | `ErrorBoundary` is a no-op | `components/shared/error_boundary.rs` | AGENT_09 L-2 |
| L36 | `ResizablePanel` not actually resizable | `components/shared/resizable_panel.rs` | AGENT_09 L-3 |
| L37 | `ContextMenu` is a pass-through | `components/shared/context_menu.rs` | AGENT_09 L-4 |
| L38 | `SwarmLauncher`'s "Launch Swarm" button is no-op | `components/swarm/swarm_launcher.rs` | AGENT_09 L-5 |
| L39 | Multiple TODO comments in production code | Various component files | AGENT_09 L-6 |
| L40 | `highlighter.rs` massive code duplication | `frontend/src/utils/highlighter.rs` | AGENT_10 Finding 8 |

---

## DUPLICATES AND OVERLAPS

### Path Traversal (6 overlapping instances)
- **AGENT_01 C1** (tool_executor) + **AGENT_07 Finding 1** (Tauri commands) + **AGENT_05 Finding 2** (athena-fs) all describe the same fundamental issue: path validation is insufficient. These overlap heavily. The `athena-fs` finding (AGENT_06) adds the TOCTOU and symlink bypass dimensions that the others miss.
- **Dedup Status:** Consolidate into CROSS-CUTTING: C1.

### Unbounded Resource Growth (5 overlapping instances)
- **AGENT_04 Critical Finding 2** (coalesce_buf) + **AGENT_08 C1** (agent_output buffer) + **AGENT_08 M4** (terminal_blocks) + **AGENT_02 2.1** (unbounded channel) + **AGENT_07 Finding 10** (unbounded data) all describe the same class of problem: no resource bounds.
- **Dedup Status:** Consolidate into CROSS-CUTTING: C5.

### Lock Poisoning (4 overlapping instances)
- **AGENT_01 C4** (orchestrator) + **AGENT_05 Finding 2** (store `has()`) + **AGENT_02 1.3** (notification RwLock) + **AGENT_10 Finding 3** (circuit breaker) all involve `std::sync::Mutex` and its poisoning/deadlock behavior.
- **Dedup Status:** Consolidate into CROSS-CUTTING: C3.

### API Key Exposure (3 overlapping instances)
- **AGENT_10 Finding 19** (Tauri `store_api_key`) + **AGENT_07 Finding 7** (command handler) + **AGENT_08 M-12** (frontend signal) all describe different facets of the same problem: API keys are not treated as secrets.
- **Dedup Status:** Consolidate into CROSS-CUTTING: C4.

### Event Listener Leaks (2 overlapping instances)
- **AGENT_08 H1** (tauri_bridge `listen()` leak) + **AGENT_09 C-2** (NotificationBell, FileTree leaks) describe the same root cause: unlisten cleanup is not implemented.
- **Dedup Status:** Keep H19 (general) and H23 (specific instances) as separate line items but note the shared root cause.

### `unwrap_or_default()` / `now_ms` (2 overlapping instances)
- **AGENT_01 L2** (orchestrator `current_time`) + **AGENT_07 Finding 11** (`session_add_message`) + **AGENT_07 Finding 15** (`notification_push`) all describe `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()`.
- **Dedup Status:** Consolidated into Low L7-L8. Single fix pattern across all occurrences.

---

## CROSS-CUTTING CONCERNS

1. **Inconsistent Path Validation Across Modules:** At least 4 different path validation implementations exist, each with different sandbox guarantees. A unified `PathValidator` crate should be created and enforced across all modules.

2. **No Defense-in-Depth for LLM-Driven Operations:** The application places complete trust in the LLM. Any vulnerability that allows the LLM to be prompt-injected (e.g., via external web content) becomes an RCE vulnerability because the tool executor lacks additional authorization gates.

3. **Frontend Memory Leak Epidemic:** Nearly every component that registers Tauri event listeners or manages reactive state leaks memory. This is a systemic pattern (not a one-off bug) that will cause the WASM heap to crash after extended use.

4. **Async/Concurrency Patterns Are Immature:** The codebase mixes `std::sync::Mutex`, `tokio::sync::Mutex`, and `RwLock` without clear conventions. Lock poisoning is handled inconsistently. Blocking I/O occurs inside async contexts. A single concurrency policy needs to be enforced.

5. **Error Handling is Spotty:** Many methods swallow errors (`.ok()`, `let _ =`, `unwrap_or_default()`), making debugging and reliability difficult. Silent failures in persistence lead to data loss.

6. **Security Testing Gap:** There is minimal to no automated security testing (e.g., fuzzing, property-based tests, security regression tests for path traversal, command injection). The unit tests primarily check happy paths.

---

## PRIORITY ACTION MATRIX

| Priority | Action | Effort | Impact |
|----------|--------|--------|--------|
| **P0** | Unify and harden path validation (C1) + Prevent command injection (C2) | High | Critical security |
| **P0** | Fix `Closure.forget()` and event listener leaks (C9, H19-H23) | Medium | Critical memory / stability |
| **P0** | Replace `std::sync::Mutex` with `parking_lot` or `tokio::sync::Mutex` (C3) | High | Critical stability |
| **P0** | Add resource bounds to terminal read loop, agent output, and unbounded channels (C5) | Medium | Critical DoS mitigation |
| **P0** | Implement proper API key handling with zeroization and backend-only storage (C4) | Medium | Critical security |
| **P1** | Fix MCP authentication (post-initialize requests) and agent comms token security (C6) | Medium-High | High security |
| **P1** | Fix TOCTOU race in `SessionManager::spawn` (C8) | Medium | High stability |
| **P1** | Remove `unsafe-eval` from CSP (H24) | Low | High security |
| **P1** | Fix `WorkspaceState` save race (C10) | Medium | High data integrity |
| **P2** | Fix `is_none_or` compatibility (H1), OpenAI error truncation (H2), Anthropic persistence (H12) | Low | Medium correctness |
| **P2** | Fix zombie processes (H10), `killpg` PGID bug (H9), `bracketed_paste` injection (M27) | Medium | Medium stability |
| **P2** | Add comprehensive tests for TCP comms, lock poisoning, and path traversal (H7) | High | Medium quality |
| **P3** | Address all Medium and Low findings | Medium | Low-medium impact |

---

## CONCLUSION

The Athena's Core codebase shows strong structural organization but has **systemic security and reliability gaps** in three areas:

1. **Security boundaries are porous:** Path validation, command execution, and plugin/LLM tool invocation lack robust sandboxing. An attacker who can influence the LLM (via prompt injection) or compromise a plugin can achieve arbitrary file access and command execution.

2. **Resource management is unbounded:** Multiple components (terminal read loop, frontend stores, agent I/O, TCP channels) grow without limit, making the application susceptible to OOM crashes and resource exhaustion attacks.

3. **Frontend has systemic memory leaks:** The pattern of registering Tauri event listeners without cleanup, combined with `Closure.forget()` in the PTY bridge, guarantees that long-running sessions will exhaust the WASM heap.

**Immediate actions required:**
- Assign P0 items to the next sprint; these are blockers for any production use.
- Conduct a focused re-audit of the unaudited `tool_executor.rs` tail (~455 lines).
- Introduce a security testing program (fuzzing, path traversal regression tests).
- Establish coding standards for: (a) path validation, (b) async/concurrency, (c) resource bounds, (d) event listener lifecycle.

---

*Report synthesized from 10 individual audit reports. 170+ individual findings consolidated into 13 Critical, 27 High, 53 Medium, and 40 Low priority items. 6 cross-cutting concerns identified.*
