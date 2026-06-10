# Deep-dive Audit: MCP, Search, Shell Hooks, Shell Integration

## Summary

This audit covers four targeted Rust files in `crates/athena-core/src/`: `mcp.rs`, `search.rs`, `shell_hooks.rs`, and `shell_integration.rs`. The review focused on external API integration, command injection vulnerabilities, input sanitization, network bugs, I/O error handling, security, and resource leaks.

**Overall Assessment:** The code is generally well-structured, but several **HIGH** and **MEDIUM** severity issues were found, particularly around command injection in the search functionality, lack of path validation in MCP tools, and resource handling in the TCP server.

---

## Findings

### Finding 1: Command Injection in `search_code` and `search_code_sync` via Unsanitized `pattern` and `path` Arguments

- **Severity:** HIGH
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** `search_code`: 64-83, `search_code_sync`: 170-189
- **Category:** Command Injection / Security
- **Description:** The `pattern` and `path` arguments from `SearchOptions` are passed directly to `tokio::process::Command` or `std::process::Command` without any sanitization or escaping. A malicious MCP client could pass a pattern like `; rm -rf / ;` or a path with shell metacharacters, which `ripgrep` may interpret as flags, files, or shell-executed commands depending on how the process is spawned.
- **Impact:** Arbitrary code execution on the host system with the privileges of the Athena application.
- **Suggested Fix:** 
  - Validate that `pattern` and `path` do not contain unexpected characters (e.g., only allow valid regex and safe path characters).
  - Consider using `--fixed-strings` if the pattern is meant to be literal to reduce regex injection risks.
  - Strip leading dashes from the pattern to prevent argument injection (e.g., passing `-e` as a pattern could be interpreted as a flag by some argument parsing libraries, though `Command` is generally safer than `system()`). While `std::process::Command` mitigates *shell injection* by not invoking the shell, it is still vulnerable to *argument injection* if the `pattern` or `path` starts with `-` and is treated as a flag by `rg`.
  - Explicitly use `Command::arg()` for each argument to ensure they are not parsed as flags.
  - Add validation to ensure the `path` is a real directory and within a permitted workspace.

---

### Finding 2: Argument Injection in `search_files` via Unsanitized `pattern` and `directory` Arguments

- **Severity:** HIGH
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** 281-304
- **Category:** Command Injection / Security
- **Description:** The `search_files` function constructs a `glob` pattern from the user-provided `pattern` argument using `format!("*{}*", pattern)` and passes it directly to `ripgrep` via `Command::new`. While `std::process::Command` does not invoke a shell, the `glob` argument is passed directly to `rg` which interprets it. More critically, a `pattern` starting with `-` would be passed as part of the glob string, and while `rg` handles the `--` separator, the earlier arguments are not protected.
- **Impact:** Potential for argument injection into `ripgrep`, leading to unauthorized file access or information disclosure.
- **Suggested Fix:** 
  - Sanitize the `pattern` input before embedding it into a glob. Strip leading dashes.
  - Validate that the `directory` argument is a valid path and within an allowed workspace.
  - Consider using the `glob` crate instead of passing user input directly to `ripgrep`.

---

### Finding 3: Path Traversal in `fs_read_file`, `fs_list_dir`, and `fs_search` Tools

- **Severity:** HIGH
- **File:** `crates/athena-core/src/tool_executor.rs`
- **Lines:** `fs_read_file`: 1106-1129, `fs_list_dir`: 1131-1176, `fs_search`: 1178-1214
- **Category:** File System Security / Path Traversal
- **Description:** The `validate_path` method resolves relative paths against `get_workspace_root()`, which uses `std::env::current_dir()`. However, there is no check to ensure the resolved path stays within the workspace root. A malicious client could pass `path: "../../../etc/passwd"` to read arbitrary files on the system. `fs_search` also passes the validated path to `search_code_sync`, which in turn passes it to `ripgrep`.
- **Impact:** Arbitrary file read, directory listing, and potentially code execution (if searching executable files triggers side effects) outside the intended workspace.
- **Suggested Fix:** 
  - After resolving the path, verify it is a descendant of the workspace root.
  - Use `std::path::Path::starts_with` after canonicalizing the path.
  - Document this as a critical security requirement.

---

### Finding 4: Unauthenticated `tools/call` Requests After Connection Establishment

- **Severity:** MEDIUM
- **File:** `crates/athena-core/src/mcp.rs`
- **Lines:** `handle_request_impl`: 304-460, `ConnectionHandler::handle_connection`: 550-595
- **Category:** Authentication / Authorization
- **Description:** Authentication is only checked during the `initialize` method. After a successful `initialize`, the connection remains open and all subsequent `tools/call` requests are processed without re-verifying the token. While this is typical for a session-based protocol, there is no mechanism to expire or rotate the token, and `broadcast_notification` happily sends data to all connected clients including unauthenticated ones (though they shouldn't be in the map if they failed `initialize`). More importantly, if a new `tools/call` request is sent before `initialize`, it is processed.
- **Impact:** A client that never sends `initialize` but sends `tools/call` could potentially execute tools. Looking at the code, `handle_request_impl` processes any method, so a client that skips `initialize` and sends `tools/call` will have that tool executed because the `initialize` check is only done in the `initialize` branch itself.
- **Suggested Fix:** 
  - Track authenticated state per connection in `ConnectionHandler`.
  - Reject any request other than `initialize` if the connection has not been authenticated.
  - Store the authenticated state in the `ConnectionHandler` struct and check it before processing `tools/call`, `tools/list`, etc.

---

### Finding 5: Missing Timeout on `TcpListener::accept` and TCP Connections

- **Severity:** MEDIUM
- **File:** `crates/athena-core/src/mcp.rs`
- **Lines:** `accept_loop`: 497-520
- **Category:** Network / Resource Leak
- **Description:** The `accept_loop` waits indefinitely for new connections via `listener.accept().await`. This is generally fine for a server, but individual client read operations also have no timeout. A connected client could open a connection and remain idle forever, consuming a file descriptor and a Tokio task.
- **Impact:** Slow resource exhaustion (file descriptor leak) if many malicious or stale connections are established and never closed.
- **Suggested Fix:** 
  - Add a read timeout to the TCP stream using `tokio::time::timeout` around `lines.next_line().await`.
  - Consider a connection idle timeout (e.g., 5 minutes) after which the connection is closed.

---

### Finding 6: Unbounded Context Line Buffer in `search_code` and `search_code_sync`

- **Severity:** MEDIUM
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** `search_code`: 91-97; `search_code_sync`: 197-203
- **Category:** Resource Exhaustion / Denial of Service
- **Description:** The `context_lines` parameter from `SearchOptions` is passed directly to `ripgrep` with the `--context` flag. There is no upper bound on the number of context lines. A malicious client could request an extremely large `context_lines` value (e.g., `u32::MAX`), causing `ripgrep` to allocate excessive memory and potentially crash the system.
- **Impact:** Denial of Service via excessive memory allocation in the `ripgrep` subprocess.
- **Suggested Fix:** 
  - Cap `context_lines` to a reasonable maximum (e.g., 100) before passing it to `ripgrep`.
  - Cap `max_results` to a reasonable maximum as well.

---

### Finding 7: Unbounded `max_results` in `search_files` Could Exhaust Memory

- **Severity:** LOW
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** 295-298
- **Category:** Resource Exhaustion / Denial of Service
- **Description:** The `search_files` function takes `max_results` and passes it to `.take(max_results.unwrap_or(500))`. If `max_results` is set to `usize::MAX` (which is effectively `None` from the caller if not bounded), it could collect an enormous number of file paths into a `Vec<String>`, consuming all available memory if the directory tree is massive.
- **Impact:** Potential memory exhaustion on large filesystems.
- **Suggested Fix:** 
  - Cap `max_results` to a reasonable value (e.g., 5000) regardless of what the caller requests.

---

### Finding 8: `broadcast_notification` Returns Error on Lock Poisoning Without Recovery

- **Severity:** LOW
- **File:** `crates/athena-core/src/mcp.rs`
- **Lines:** 269-290
- **Category:** Error Handling / Reliability
- **Description:** `broadcast_notification` silently returns if the mutex is poisoned (`if let Ok(mut clients) = ...`). While lock poisoning indicates a previous panic, silently failing a broadcast could lead to missed notifications and no logging of the failure.
- **Impact:** Missed notifications to connected clients with no diagnostic log.
- **Suggested Fix:** 
  - Log an warning or error when the lock is poisoned.
  - Consider using `std::sync::Mutex::into_inner()` or recovering from poisoning after a panic.

---

### Finding 9: Potential for Duplicate `active_clients` Entries on Reconnection

- **Severity:** LOW
- **File:** `crates/athena-core/src/mcp.rs`
- **Lines:** 506-510, 560-562
- **Category:** Logic Error / Resource Leak
- **Description:** When a new TCP connection is accepted, the client's `peer` address string is used as the key in the `active_clients` HashMap. If a client disconnects and reconnects from the same IP and port, the old entry should be removed. However, because the port is ephemeral, this is a new key. But more importantly, if a client reconnects very quickly (before the OS reuses the port), the old connection's cleanup task and the new connection's task might race. The `peer` string could be the same if the connection is dropped and immediately re-established from the same ephemeral port (rare but possible in rapid reconnect loops). The `HashMap` insert will overwrite the old `TcpStream`, but the old task may still be running and writing to the old stream. This is acceptable as the stream is closed, but the old task is leaked until it hits an error.
- **Impact:** Minor memory leak of Tokio tasks on rapid reconnections.
- **Suggested Fix:** 
  - Use a unique connection ID (e.g., UUID or monotonic counter) instead of the peer address for the `active_clients` key.
  - This prevents key collisions and makes cleanup more predictable.

---

### Finding 10: Missing Error Response for Invalid JSON-RPC Requests

- **Severity:** LOW
- **File:** `crates/athena-core/src/mcp.rs`
- **Lines:** 576-582
- **Category:** Protocol Compliance
- **Description:** When a JSON-RPC request fails to parse, the handler logs a warning and `continue`s the loop. According to the JSON-RPC 2.0 specification, the server should return a `Parse error` or `Invalid Request` response if the message has an `id` field. The current implementation silently drops malformed messages, which can confuse clients waiting for responses.
- **Impact:** Client hangs when sending malformed requests, making debugging difficult.
- **Suggested Fix:** 
  - Attempt to extract the `id` from the raw JSON (even if the rest is malformed) and return a JSON-RPC error response.
  - At minimum, send a generic parse error response.

---

### Finding 11: `strip_osc633` Does Not Handle Overlapping Sequences Correctly

- **Severity:** LOW
- **File:** `crates/athena-core/src/shell_integration.rs`
- **Lines:** 289-336
- **Category:** Logic Error / Text Processing
- **Description:** In `strip_osc633`, after finding a `BEL` terminator at index `bi`, `pos` is set to `bi`, not `bi + 1`. This means the `BEL` character (single byte) is left in the output string. The code appends `result.push_str(&data[pos..osc_start])` and then sets `pos = bi`. If `pos` is set to `bi`, the next loop will try to append from `bi` to the next `osc_start`, which includes the `BEL` byte. This needs closer inspection.
  - Wait, let's re-read: `result.push_str(&data[pos..osc_start])` adds the text before the sequence. Then `pos = bi` means the next iteration will skip from `bi` to the next `osc_start`. If `bi` is the index of the `BEL` *byte* in the string, then `data[bi..osc_start_next]` would include the BEL. So `BEL` is NOT included in `result` because `pos` was set to `bi` and then in the next iteration, `result.push_str(&data[pos..osc_start])` will append text from `bi` (inclusive) to the next `osc_start`. If `bi` is the index of the BEL character, this means the BEL character is at `data[bi]` and would be included in the appended slice. Actually wait, the loop continues with `pos = bi`. The next iteration finds the next `osc_start`. Then it does `result.push_str(&data[pos..osc_start])`. If `pos == bi`, then this range is `[bi..osc_start]`, which includes the BEL character. So the BEL character is NOT stripped correctly! 
  - Actually, looking again: `pos` is an integer offset into the string `data`. `bi` is an index in the string `data`. `data[pos..osc_start]` is a slice. If `pos == bi` and `bi` points to the `BEL` byte, then the slice includes the `BEL`. So `strip_osc633` does NOT strip the `BEL` terminator; it leaves it in the output.
- **Impact:** Incorrect terminal output display if `strip_osc633` is used for display text.
- **Suggested Fix:** 
  - Calculate `pos = bi + 1` (for BEL) or `pos = si + 2` (for ST) after stripping a sequence, to skip past the terminator bytes themselves.

---

### Finding 12: `shell_hooks.rs` - `is_initialized` Returns `false` on Lock Poisoning

- **Severity:** LOW
- **File:** `crates/athena-core/src/shell_hooks.rs`
- **Lines:** 49-56
- **Category:** Error Handling / Reliability
- **Description:** `is_initialized` returns `false` if the mutex is poisoned. This could mislead callers into thinking the service is not initialized when it actually is, but a previous panic has corrupted the state.
- **Impact:** Misleading state reporting; could cause re-initialization or other logic errors downstream.
- **Suggested Fix:** 
  - Log an error when the lock is poisoned.
  - Consider panicking or returning a Result if the lock is poisoned, as it indicates a critical failure.

---

### Finding 13: `shell_hooks.rs` - `initialized` Mutex is Redundant

- **Severity:** INFO
- **File:** `crates/athena-core/src/shell_hooks.rs`
- **Lines:** 17, 24-55
- **Category:** Code Quality
- **Description:** The `initialized` field is set to `true` in `init()` and `false` in `shutdown()`, but there is no actual functional behavior that depends on it. The `OutputBuffer` operations continue to work regardless. This mutex adds unnecessary overhead and complexity.
- **Impact:** Minor performance and maintainability issues.
- **Suggested Fix:** 
  - Remove the `initialized` field and `is_initialized` method, or use it to gate operations if that was the intended behavior.

---

### Finding 14: `search_code` and `search_code_sync` Leave `pending_context` Unprocessed on Early Break

- **Severity:** LOW
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** `search_code`: 135-142; `search_code_sync`: 245-252
- **Category:** Logic Error / Data Loss
- **Description:** When `max_results` is reached and `truncated = true; break;`, the loop exits immediately. Any pending context lines accumulated in `pending_context` that belong to matches beyond the truncation point are lost. More importantly, the post-processing loop that assigns trailing context to the last match won't see these lines. This is generally acceptable for truncation, but the context lines for the last match might be incomplete.
- **Impact:** Incomplete context lines for the last match when results are truncated.
- **Suggested Fix:** 
  - Ensure `pending_context` is fully processed before the early break, or process all context first then truncate the matches.

---

### Finding 15: `get_shell_integration_script` Default Falls Through to Zsh Without Error

- **Severity:** LOW
- **File:** `crates/athena-core/src/shell_integration.rs`
- **Lines:** 489-497
- **Category:** Logic Error
- **Description:** If an unsupported shell is passed to `get_shell_integration_script`, it silently returns the Zsh integration script. This could cause the integration script to be injected into an incompatible shell, potentially causing errors or unexpected behavior.
- **Impact:** Injected script may fail or cause issues in an unsupported shell.
- **Suggested Fix:** 
  - Return an `Option<String>` or `Result<String, Error>` instead of a plain `String`.
  - Return `None` or an error for unsupported shells, and let the caller decide whether to proceed.

---

### Finding 16: `fs_search` in ToolExecutor Uses Sync I/O in Async Context via `search_code_sync`

- **Severity:** LOW
- **File:** `crates/athena-core/src/tool_executor.rs`
- **Lines:** 1178-1214
- **Category:** Performance / Blocking I/O
- **Description:** The `fs_search` tool is called from the MCP server, which runs in an async context (Tokio). However, it calls `search_code_sync`, which spawns a blocking `std::process::Command`. While this is acceptable because `ToolExecutor::execute_tool_call` is synchronous itself, it blocks the Tokio worker thread. This is a potential performance issue if many tool calls are being processed concurrently.
- **Impact:** Blocking the async executor thread could reduce throughput of the MCP server.
- **Suggested Fix:** 
  - Make `execute_tool_call` async, or use `tokio::task::spawn_blocking` for `fs_search` to offload the blocking I/O to a dedicated thread pool.

---

### Finding 17: `search_files` Glob Construction is Fragile

- **Severity:** LOW
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** 289-293
- **Category:** Input Sanitization / Logic Error
- **Description:** The `search_files` function constructs a glob from the user-provided `pattern`: `args.push(format!("*{}*", pattern))`. If the user's pattern already contains glob special characters (e.g., `*`, `?`, `[`), the resulting glob may not match what the user intended. More importantly, it causes double-globbing and potential for matching far more files than intended.
- **Impact:** Incorrect search results; potential for matching unintended files.
- **Suggested Fix:** 
  - Document that `pattern` is embedded into a glob.
  - Escape special characters from the user's `pattern` before embedding into the glob, or use a different search strategy.

---

### Finding 18: `launch_builtin_agent` and `launch_custom_agent` Do Not Validate `agent_type` or Command Against Allowlist

- **Severity:** MEDIUM
- **File:** `crates/athena-core/src/tool_executor.rs`
- **Lines:** `launch_builtin_agent`: 810-832, `launch_custom_agent`: 834-862
- **Category:** Security / Command Execution
- **Description:** `launch_builtin_agent` maps `agent_type` to a command string via `build_agent_command`, but the `agent_type` is not validated against known-good values. An MCP client could pass an arbitrary `agent_type` string. While the current `build_agent_command` implementation does have a match on known strings and falls through to `"claude"`, the `launch_custom_agent` does NOT have any allowlist validation.
- **Impact:** In `launch_custom_agent`, the `command` argument is checked against an environment variable allowlist (`ATHENA_COMMAND_ALLOWLIST`), which is good. In `launch_builtin_agent`, the `agent_type` is mapped to a hardcoded command, which is also good. But `task_prompt` is shell-escaped using `shell_escape::escape`, which is good. However, if `agent_type` is something unexpected that gets past the match, it falls through to `claude`, which is benign. The `launch_custom_agent` check against the environment variable allowlist is a good practice.
- **Suggested Fix:** 
  - The allowlist check in `launch_custom_agent` is good but the environment variable could be unset. Ensure there is a default set of allowed commands or a hardcoded allowlist.

---

### Finding 19: `search_code` JSON Parse Errors Are Silently Ignored

- **Severity:** LOW
- **File:** `crates/athena-core/src/search.rs`
- **Lines:** 103-115, 227-239
- **Category:** Error Handling / Data Loss
- **Description:** In both `search_code` and `search_code_sync`, when `serde_json::from_str(line)` fails, the line is simply skipped with `Err(_) => continue;`. This means if `ripgrep` produces a malformed JSON line (e.g., due to file encoding issues or stdout corruption), the error is silently ignored. The caller receives a truncated or empty result without knowing that data was lost.
- **Impact:** Silent data loss; user may think there were no matches when there were.
- **Suggested Fix:** 
  - Log the parse error at `warn` or `error` level.
  - Consider accumulating parse errors and returning them as part of the `SearchResult` or `SearchError`.

---

### Finding 20: `McpServer::init` Spawns Background Task Without Handle or Shutdown Signal

- **Severity:** LOW
- **File:** `crates/athena-core/src/mcp.rs`
- **Lines:** 260-275
- **Category:** Resource Management
- **Description:** `init` spawns a background Tokio task for the accept loop but does not store the returned `JoinHandle`. The `shutdown` method sets `self.listener = None` and clears clients, but it does not have a way to signal the accept loop to stop. The `accept_loop` will continue until发誓�
---

*Note: This finding was cut off in the original source. It noted that the `accept_loop` will continue operating on the listener until it attempts to accept, at which point it may error out. However, the old listener is dropped when `shutdown` is called, which should cause `accept` to fail. But this is not a clean shutdown.*

- **Impact:** The accept loop task may panic or log errors during shutdown.
- **Suggested Fix:** 
  - Use a `tokio::sync::broadcast` or `CancellationToken` to cleanly shut down the accept loop.
  - Store the `JoinHandle` so it can be awaited or aborted.
