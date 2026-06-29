# Tauri Command Handlers Security Audit

**Commit:** 37d088fc  
**Audited Files:** 17 files in `src-tauri/src/commands/`  
**Date:** 2026-06-09

---

## Summary

This audit covers all Tauri command handlers in `src-tauri/src/commands/`. Findings span path traversal, command injection, race conditions, resource exhaustion, input validation, and error handling issues. Overall the code is well-structured with a consistent `CommandError` enum and good use of `tokio::task::spawn_blocking` for I/O, but several security and correctness issues were identified.

---

## Findings

### 1. Path Traversal — `validate_path` uses `starts_with` and manual `strip_prefix`, not canonicalization (mod.rs, fs.rs)

**Severity:** High  
**File:** `src-tauri/src/commands/mod.rs`, lines 35-60  
**Category:** Path Traversal  
**Description:**
`validate_path` for writes does not canonicalize the path before checking `starts_with(&root)`. It joins relative paths to the root, checks `path.starts_with(&root)`, and then manually strips path components for `ParentDir`. However, on systems that support symlinks, a symlink can be created inside the workspace pointing outside, and the write will follow the symlink, escaping the sandbox. Similarly, race conditions between the `starts_with` check and the actual file write (TOCTOU) are possible. Read validation (`validate_path_exists`) does canonicalize via `std::fs::canonicalize`, but the write path (`validate_path`) intentionally omits it (to allow creating new files). This means the write sandbox is weaker than the read sandbox.

**Impact:** An attacker with the ability to create symlinks within the workspace can write to arbitrary files outside the workspace root.

**Suggested Fix:** After creating parent directories, attempt to canonicalize the created file's path and verify it starts with the workspace root. Alternatively, use a dedicated temp directory for file creation and move atomically. Also resolve symlinks with a strict limit on symlink depth.

---

### 2. `fs_exists` uses `validate_path` (write validator) for a read-like operation, allowing path traversal for existence checks (mod.rs, fs.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/mod.rs`, lines ~330; `fs.rs`, lines ~50  
**Category:** Path Traversal / Information Disclosure  
**Description:**  
`fs_exists(path)` calls `validate_path(path_ref).is_ok()` — this is the write validator, not `validate_path_for_read`. The write validator aborts if the path doesn't start with the workspace root, BUT it also calls `std::fs::create_dir_all` for parent directories. A malicious caller could use `fs_exists` to trigger directory creation anywhere the process has write access (if they bypass the `starts_with` check via symlink, see Finding 1). Even without the symlink bypass, the semantic mismatch is notable: a read-only "exists" check should not create directories.

**Impact:** Can be used to probe file existence, create directories spuriously, and combined with Finding 1, potentially escalate to write access.

**Suggested Fix:** Create a `validate_path_exists_readonly` that only checks path bounds without creating directories or writing. Use it for `fs_exists` and all read-only operations.

---

### 3. `fs_search_files` / `search_code` passes user-controlled `path` directly to ripgrep without sandboxing (mod.rs, fs.rs, search.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/mod.rs`, lines ~470; `fs.rs`, lines ~140; `search.rs`, all  
**Category:** Path Traversal / Command Injection  
**Description:**  
`fs_search_files` and `search_code` accept a `path: String` and construct:
```rust
athena_core::SearchOptions { pattern, path, glob: None, ... }
```
Neither validates that `path` is within the workspace root. If the user passes `path: "/etc"`, ripgrep will search outside the workspace. The `search.rs` version at least limits pattern length (4096 chars) and rejects empty patterns, but still does not validate the `path` parameter.

**Impact:** Users can search/read arbitrary directories on the host filesystem via the search command.

**Suggested Fix:** Run the path through `validate_path_exists` or `validate_path_for_read` before constructing the search options.

---

### 4. `shell_integration_script` and `shell_integration_compatible` accept arbitrary shell names without validation (shell.rs)

**Severity:** Low-Medium  
**File:** `src-tauri/src/commands/shell.rs`, lines ~14 and ~22  
**Category:** Input Validation  
**Description:**  
Both commands accept a user-controlled `shell: String` and pass it directly to `athena_core::shell_integration::get_shell_integration_script(&shell)` and `is_shell_integration_compatible(&shell)` without any validation. If these backend functions use the shell string to read files or build commands, this could lead to directory traversal or command injection.

**Impact:** Depends on the implementation of the integration functions, but from the command perspective this is an unsanitized input.

**Suggested Fix:** Validate the shell name against an allow-list (e.g., "bash", "zsh", "fish") before passing it to the backend.

---

### 5. `swarm_read_state`, `swarm_send_message`, `swarm_read_mailbox` accept arbitrary `dir` strings without path validation (swarm.rs)

**Severity:** High  
**File:** `src-tauri/src/commands/swarm.rs`, all lines  
**Category:** Path Traversal  
**Description:**  
All three swarm commands take a `dir: String` parameter and pass it directly to the coordinator:
```rust
coordinator.read_state(&dir)
coordinator.send_message(&dir, &from, &to, &content)
coordinator.read_mailbox(&dir, &agent_id)
```
There is no validation to ensure `dir` is within the workspace. The swarm coordinator may read/write files in that directory.

**Impact:** Arbitrary file read/write through the swarm coordinator, potentially reading sensitive host files or writing malicious content to system directories.

**Suggested Fix:** Sanitize the `dir` parameter using the existing `validate_path` or `validate_path_exists` helper before passing it to the coordinator.

---

### 6. `athena_chat`, `athena_chat_with_session`, `athena_chat_with_images` race condition on config (athena.rs, mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/athena.rs`, lines ~40-70  
**Category:** Race Condition  
**Description:**  
`athena_chat_with_session` does:
```rust
orchestrator.set_current_session_id(session_id);
orchestrator.send_message(message, None).await...
```
This is not atomic. Another concurrent call to `athena_chat_with_session` could change the session ID between the `set` and the `send_message`, causing the message to be logged to the wrong session. The `athena.rs` version claims to hold the orchestrator lock for the whole sequence, but `state.orchestrator` is a bare `Arc<T>` (no mutex), so there is no actual lock.

**Impact:** Messages may be associated with the wrong session, leading to data corruption or privacy issues.

**Suggested Fix:** Make the "set session ID and send message" operation an atomic call on the orchestrator, or wrap the orchestrator in a way that locks across the set+send sequence.

---

### 7. `athena_chat` stores API key in memory without zeroization (athena.rs, mod.rs)

**Severity:** Low-Medium  
**File:** `src-tauri/src/commands/athena.rs`, `build_provider_config_from_store`  
**Category:** Security / Secret Leakage  
**Description:**  
The API key is loaded from the keyring or store, placed into a `String`, and passed to `orchestrator.set_provider_config()`. `String` values are not zeroized on drop, and the key may remain in memory until the allocator reclaims the page.

**Impact:** API key could be recovered from a core dump or memory inspection.

**Suggested Fix:** Use a ` secrecy::SecretString` or similar zero-on-drop container for the API key.

---

### 8. `browser_open_external` accepts arbitrary URL without validation; opens native webview (browser.rs / mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/mod.rs`, lines ~1850-1890  
**Category:** Input Validation / Security  
**Description:**  
`browser_open_external` accepts any URL string, parses it with `tauri::Url::parse`, and immediately opens a native webview window. There is no filtering for `file://`, `javascript:`, or other dangerous schemes. A `file://` URL could be used to render local files in a webview context.

**Impact:** Could allow rendering local files, executing local scripts, or launching local applications.

**Suggested Fix:** Allow-list only `http://` and `https://` schemes. Reject `file://`, `javascript:`, `data:`, and other dangerous schemes explicitly.

---

### 9. `tool_execute` deserializes arbitrary JSON without validation of `tool_name` or argument structure (tool.rs, mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/tool.rs`, all; `mod.rs`, lines ~1730-1780  
**Category:** Input Validation / Command Injection  
**Description:**  
`tool_execute` deserializes `arguments: String` into `ToolInput`, then passes `tool_name` and the input to the tool executor. If the tool executor invokes shell commands based on the tool name or arguments, this could be a command injection vector. The command handler itself does no allow-listing of tool names.

**Impact:** Potential arbitrary command execution depending on the tool executor's implementation.

**Suggested Fix:** Validate `tool_name` against the list of known tools from `tool_list()` before executing. Also validate the argument JSON structure against the tool's schema.

---

### 10. `shell_integration_parse` reads entire `data` string into memory, no size limit (shell.rs, mod.rs)

**Severity:** Low  
**File:** `src-tauri/src/commands/shell.rs`, lines ~8-18  
**Category:** Resource Exhaustion  
**Description:**  
The `data: String` parameter is unbounded. A malicious or malformed frontend could send a multi-gigabyte string, causing excessive memory consumption before the `Osc633Parser` even processes it.

**Impact:** Denial of service via memory exhaustion.

**Suggested Fix:** Reject inputs larger than a reasonable maximum (e.g., 1 MB) before processing.

---

### 11. `session_add_message` uses `std::time::SystemTime::now()` with `unwrap_or_default()` timestamp (session.rs, mod.rs)

**Severity:** Very Low  
**File:** `src-tauri/src/commands/session.rs`, lines ~88-102; `mod.rs`, lines ~540-580  
**Category:** Logic / Correctness  
**Description:**  
```rust
timestamp: std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .unwrap_or_default()
    .as_millis() as u64,
```
If the system clock is ever before the Unix epoch (highly unlikely on modern systems, but possible in VMs), the timestamp will be `0`, which is misleading for ordering and display. `unwrap_or_default()` on a `Duration` returns `Duration::default() = 0`.

**Impact:** Incorrect timestamps in edge cases; messages may appear out of order.

**Suggested Fix:** Return an explicit error if `duration_since(UNIX_EPOCH)` fails, or at least log a warning.

---

### 12. `plugin_host_discover_plugins` accepts directory string without using path validation helper (plugin_host.rs, mod.rs)

**Severity:** High  
**File:** `src-tauri/src/commands/plugin_host.rs`, lines ~110-140  
**Category:** Path Traversal  
**Description:**  
The `dir` parameter is checked for `..` and empty string, but not for `starts_with` workspace validation. It also does not check for absolute paths that escape the workspace. This allows scanning arbitrary directories on the host for plugin manifests.

**Impact:** Directory enumeration outside the workspace; reading arbitrary directory listings.

**Suggested Fix:** Use `validate_path` or `validate_path_for_read` on the `dir` parameter.

---

### 13. `plugin_host_emit_event` accepts arbitrary `event_type` string and formats it directly into JSON (plugin_host.rs, mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/plugin_host.rs`, lines ~47-65  
**Category:** Input Validation / Injection  
**Description:**  
```rust
let parsed_type: athena_plugins::PluginEventType =
    serde_json::from_value(serde_json::Value::String(event_type.to_lowercase()))
        .map_err(...)?;
```
While `.to_lowercase()` is applied, the event type is converted directly to a JSON string and then deserialized. If the `PluginEventType` deserialization is ever changed to be more permissive or the deserializer has edge cases, this could parse unexpected types. More importantly, the `event_type` is not checked against a known good set before deserialization.

**Impact:** Unexpected event types could be processed, leading to unintended plugin behavior.

**Suggested Fix:** Validate `event_type` against a known allow-list of event type strings before creating the JSON value for deserialization.

---

### 14. `store_get`, `store_set`, `store_delete`, `store_has` accept arbitrary keys without validation (store.rs, mod.rs)

**Severity:** Low  
**File:** `src-tauri/src/commands/store.rs`, all; `mod.rs`, lines ~660-690  
**Category:** Input Validation  
**Description:**  
The `key: String` parameter is passed directly to the store without length limits or character validation. Extremely long keys could cause issues with the underlying SQLite store.

**Impact:** Potential denial of service by creating massive keys; possible SQLite injection if the store implementation is not using parameterized queries.

**Suggested Fix:** Limit key length (e.g., 1024 characters) and reject keys containing null bytes or other control characters.

---

### 15. `notification_push` uses `unwrap_or_default()` on SystemTime (notification.rs, mod.rs)

**Severity:** Very Low  
**File:** `src-tauri/src/commands/notification.rs`, lines ~20-30; `mod.rs`, lines ~1030-1060  
**Category:** Logic / Correctness  
**Description:**  
Same issue as Finding 11: `SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default()`.

**Impact:** Incorrect notification timestamps on systems with clocks before Unix epoch.

**Suggested Fix:** Log a warning or return an error if timestamp cannot be computed.

---

### 16. `browser_show` (commands/browser.rs variant) lacks proxy/URL normalization that the mod.rs variant has (browser.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/browser.rs`, lines ~5-12  
**Category:** Input Validation / Security  
**Description:**  
The standalone `browser.rs` commands (`browser_show`, `browser_navigate`, etc.) do NOT use `get_normalized_url()` or `proxy_url_for()`. They pass the raw `url` string directly to `browser_manager.open_browser(id, &url)`. This is a fragmented implementation: `mod.rs` has a more robust version with URL normalization and proxy URL generation, but `browser.rs` bypasses it entirely.

**Impact:** The standalone browser commands lack URL validation, allowing `javascript:`, `file://`, and other dangerous schemes.

**Suggested Fix:** Consolidate browser logic so all browser commands share the same URL normalization and validation path. Delete the duplicate implementation in `browser.rs` or make it call the same helpers.

---

### 17. `window_minimize`, `window_maximize`, `window_close`, `window_is_maximized` use hardcoded window label "main" (window.rs, mod.rs)

**Severity:** Very Low  
**File:** `src-tauri/src/commands/window.rs`, all  
**Category:** Logic / Usability  
**Description:**  
All window commands are hardcoded to use `"main"` as the window label. If the app ever has multiple windows, these commands will act on the wrong window or fail.

**Impact:** Limited to apps with multi-window support; currently not a security issue.

**Suggested Fix:** Accept a `label` parameter or iterate over all windows when no label is provided.

---

### 18. `session_create` and `session_update` accept `title` as unconstrained user input (session.rs)

**Severity:** Very Low  
**File:** `src-tauri/src/commands/session.rs`, lines ~8-11, ~40-55  
**Category:** Input Validation  
**Description:**  
`title: Option<String>` is passed directly to the session store with no length or content validation. The store may persist it to the database. While the session store likely sanitizes it, the command handler does not enforce any limits.

**Impact:** Potential database bloat or downstream injection if the session store doesn't validate.

**Suggested Fix:** Add a reasonable length limit (e.g., 256 characters) on the title.

---

### 19. `plan_create` accepts unconstrained `goal`, `reasoning`, and `steps` strings (plan.rs)

**Severity:** Very Low  
**File:** `src-tauri/src/commands/plan.rs`, all  
**Category:** Input Validation  
**Description:**  
`goal`, `reasoning`, and `steps` are all user-controlled and unconstrained in length. The `steps` string is deserialized from JSON, but the individual fields inside are not validated.

**Impact:** Potential memory exhaustion or database bloat.

**Suggested Fix:** Add length limits for `goal` and `reasoning` (e.g., 10,000 characters each) and consider a maximum number of steps.

---

### 20. `pty_write` passes raw data to PTY without sanitization (mod.rs, shell.rs)

**Severity:** Low  
**File:** `src-tauri/src/commands/mod.rs`, lines ~780-800  
**Category:** Command Injection  
**Description:**  
`pty_write(state, id, data)` writes `data` directly to a PTY session. This is expected behavior for a terminal, but if an agent (LLM) can invoke `pty_write` through a tool call, it creates a command injection surface where the LLM can execute arbitrary shell commands in any active PTY session.

**Impact:** LLM with tool access can execute arbitrary shell commands unsafely.

**Suggested Fix:** Ensure the tool executor restricts which sessions (by ownership or agent ID) can be written to, and require user confirmation before writing to PTY sessions from automated tools.

---

### 21. `mcp_handle_request` accepts arbitrary JSON-RPC request string without validation (mcp.rs, mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/mcp.rs`, lines ~23-34  
**Category:** Input Validation  
**Description:**  
The `request: String` is parsed by `McpServer::parse_request` and then handled. The command handler doesn't validate the request length or content before passing it to the server. A maliciously crafted JSON-RPC request could trigger unexpected behavior in the MCP server.

**Impact:** Potential denial of service or unexpected MCP server behavior.

**Suggested Fix:** Limit the request string size (e.g., 1 MB) before parsing. Validate it's valid JSON before passing to the MCP server.

---

### 22. `plugin_register` and `plugin_host_setup_plugin` allow arbitrary plugin registration with no authentication (plugin.rs, plugin_host.rs)

**Severity:** Medium-High  
**File:** `src-tauri/src/commands/plugin.rs`, lines ~25-40; `plugin_host.rs`, lines ~140-165  
**Category:** Authorization / Input Validation  
**Description:**  
Both commands allow registering a plugin with arbitrary `plugin_id`, `name`, and `version` with no authentication, authorization, or signature verification. A malicious frontend or another caller could register plugins, potentially enabling them to execute code or access data.

**Impact:** Unauthorized plugin registration, potential privilege escalation.

**Suggested Fix:** Require a capability token or user confirmation for plugin registration. Validate plugin manifests against a trusted registry or signature.

---

### 23. `agent_comms_token` exposes a sensitive token without any access control (agent.rs, mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/agent.rs`, lines ~8-10; `mod.rs`, lines ~1140-1150  
**Category:** Authorization  
**Description:**  
Any caller can invoke `agent_comms_token` and receive the comms token. There is no check that the caller is authenticated, authorized, or even a valid agent.

**Impact:** If the comms token is used for authentication, any part of the frontend (including potentially compromised code) can fetch it.

**Suggested Fix:** Restrict this command to privileged callers or remove it from the exposed Tauri command set and provide it only through secure internal channels.

---

### 24. `fs_write_file` does not limit content size, allowing unbounded writes (fs.rs, mod.rs)

**Severity:** Medium  
**File:** `src-tauri/src/commands/fs.rs`, lines ~33-42; `mod.rs`, lines ~310-325  
**Category:** Resource Exhaustion  
**Description:**  
`fs_write_file` accepts a `content: String` with no size limit and writes it directly to disk. A malicious frontend could write arbitrarily large files, filling up disk space.

**Impact:** Disk space exhaustion; potential denial of service.

**Suggested Fix:** Add a configurable maximum file size check before writing. Return an error if `content.len()` exceeds the limit.

---

### 25. `output_buffer_append` is a synchronous command that appends data without rate limiting (output.rs, mod.rs)

**Severity:** Low  
**File:** `src-tauri/src/commands/output.rs`, lines ~7-15; `mod.rs`, lines ~910-920  
**Category:** Resource Exhaustion  
**Description:**  
`output_buffer_append` is a synchronous Tauri command. A rapid-fire call from the frontend could overwhelm the output buffer, which may hold an unbounded amount of data.

**Impact:** Memory exhaustion if the output buffer grows without limit.

**Suggested Fix:** Make this an async command (to allow backpressure) and/or add a maximum buffer size for the output buffer itself.

---

## Recommendations Summary

1. **Unify path validation:** Ensure ALL file-system-touching commands use the same canonicalization-based validation. The write path (file creation) is the weakest point.
2. **Consolidate browser commands:** The `browser.rs` standalone commands should use the same `get_normalized_url()` and proxy logic as `mod.rs`.
3. **Add input length limits:** Nearly all string-based commands (store keys, file content, shell names, plugin IDs) should have maximum length checks.
4. **Add authorization model:** Commands that register plugins, get comms tokens, spawn agents, or execute tools need some form of caller authorization.
5. **Audit `athena_core` backend functions:** Many commands delegate to `athena_core` (search_code, shell integration, tool executor, swarm coordinator). The security of those backend functions was not covered in this audit. The command handlers trust the backends completely.
