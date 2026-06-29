# Audit Findings: Utils, Types, Themes, and Config

## Summary
Deep-dive analysis of utility functions, type definitions, theme system, Tauri main entry point, and build configuration files. Issues span from security vulnerabilities in CSP and inline JS to logic bugs in circuit breaker and highlighter, missing `Eq` derives, and potential panic points.

---

## 1. HIGH — `unsafe-eval` in CSP Permits Code Injection

- **File**: `src-tauri/tauri.conf.json`
- **Line**: 21
- **Category**: Security / Tauri Capabilities
- **Description**: The Content Security Policy includes `'unsafe-eval'` in `script-src`. In a Tauri desktop app with access to local filesystem and shell plugins, allowing `eval()`-equivalent execution creates a significant attack surface. If an attacker can inject script content (e.g., via a compromised dependency, XSS in embedded web content, or malicious file opening), they can execute arbitrary code with desktop-level privileges.
- **Impact**: Critical. Arbitrary code execution in a privileged desktop context.
- **Suggested Fix**: Remove `'unsafe-eval'` if possible. If required for a specific library (e.g., some WASM bundlers), scope it more narrowly or find a CSP-compliant alternative. Document the justification if it must remain.

---

## 2. HIGH — Inline JavaScript in `wasm_bindgen` for Audio — CSP Bypass / XSS

- **File**: `frontend/src/utils/notification_sound.rs`
- **Lines**: 18-35
- **Category**: Security / Inline Scripting
- **Description**: The `play_ding()` function uses `#[wasm_bindgen(inline_js = ...)]` to inject raw JavaScript that creates an `AudioContext`. Inline JS blocks are hard to audit, bypass typical WASM sandboxing, and can be blocked by strict CSPs. If the `unsafe-eval` from the CSP finding above is ever removed, this inline block may break. More critically, using `inline_js` opens a pattern where arbitrary JS can be executed inside the WASM boundary.
- **Impact**: Moderate to High. Breaks CSP hardening and sets a dangerous precedent for mixing Rust/WASM logic with unauditable inline JS.
- **Suggested Fix**: Move the Web Audio API call to a dedicated `.js` file imported via `wasm_bindgen` extern declarations, or trigger the sound via a Tauri command / DOM event instead.

---

## 3. HIGH — Circuit Breaker: `execute()` Drops Mutex Guard Across `.await` Boundary (Potentially Blocking)

- **File**: `frontend/src/utils/circuit_breaker.rs`
- **Lines**: 241-255
- **Category**: Performance / Concurrency
- **Description**: The `execute()` method takes a `FnOnce() -> Result<T, E>` (not async) and calls `self.can_execute()` which locks a `Mutex`, then runs the closure synchronously. While the closure itself is sync, in a Dioxus/WASM context this is fine, but if this code is ever reused in an async context or if the closure performs I/O, holding the mutex across the operation is a recipe for contention. More importantly, the `CircuitBreaker` struct uses `std::sync::Mutex` (not `tokio::sync::Mutex`), which is NOT safe to hold across `.await` points. If `execute()` is ever made async, this will deadlock.
- **Impact**: Moderate. Currently sync-only in this codebase, but a future refactor to async will deadlock.
- **Suggested Fix**: Either keep the API strictly sync, or provide an `execute_async` variant that uses `tokio::sync::Mutex` and drops the guard before awaiting.

---

## 4. HIGH — `highlighter.rs` Line-Number Prefix Parsing is Brittle and Wrong for Short Lines

- **File**: `frontend/src/utils/highlighter.rs`
- **Lines**: Multiple occurrences across all language highlighters (e.g., Rust at line 117, JS at line 276, Python at line 467, etc.)
- **Category**: Logic Bug
- **Description**: Nearly every `highlight_*_line` function repeats this logic:

  ```rust
  if len >= 6 {
      let mut ok = true;
      for j in 0..5 {
          if !chars[j].is_ascii_digit() && chars[j] != ' ' {
              ok = false;
              break;
          }
      }
      if ok && chars[5] == ' ' {
          line_num_end = 6;
          output.push_str(&line[..line_num_end]);
      }
  }
  ```

  If `len` is exactly 6 and the string is `"12345 "`, `chars[5]` is `' '` — but `chars[0..5]` are all digits, so `ok` is true and `chars[5] == ' '` is true. This correctly treats `"12345 " as a line number prefix. However, if `len` is 5 (e.g., a 5-character line like "hello"), `len >= 6` is false, so it skips this block. If `len` is 6 but the 5th char is not a digit/space (e.g., "hello "), it false-positives if `chars[5]` happens to be `' '`. Most critically: if `len` is between 1 and 5, `char_at` is never called, but the code then proceeds to process the whole line including what might be a short line number like "1 ". This repeated brittle prefix parsing is duplicated across ~8 highlighters and can mangle short lines.
- **Impact**: Moderate. Short code lines (< 6 chars) may be incorrectly split or have their first few characters treated as a line number prefixModule,"  or short lines may have unexpected behavior.
- **Suggested Fix**: Extract a shared `strip_line_number_prefix(line: &str) -> (&str, Option<&str>)` helper. Verify the prefix pattern more strictly (e.g., `^\s*\d+\s`).

---

## 5. MEDIUM — `assistant_logger.rs` Uses `unwrap()` on `Mutex` Lock

- **File**: `frontend/src/utils/assistant_logger.rs`
- **Lines**: 109, 144, 164, 181, etc. (throughout)
- **Category**: Reliability / Panic Risk
- **Description**: All `self.inner.lock().unwrap()` calls will panic if the mutex is poisoned. In a WASM single-threaded environment, mutex poisoning from panicked threads is less likely, but this pattern should use `expect()` with a descriptive message, or handle the error gracefully.
- **Impact**: Low in WASM (single-threaded), but poor practice.
- **Suggested Fix**: Replace `.unwrap()` with `.expect("assistant logger mutex poisoned")` or use `Mutex` from `parking_lot` which does not poison.

---

## 6. MEDIUM — `fuzzy_search.rs`: `to_lowercase()` Called Repeatedly

- **File**: `frontend/src/utils/fuzzy_search.rs`
- **Lines**: 21-42
- **Category**: Performance
- **Description**: The `fuzzy_search` function computes `query.to_lowercase()` once, but for each item it computes `item.to_lowercase()` and inside the sort closure it computes `a.to_lowercase()` and `b.to_lowercase()` again. For large result sets, this is O(n²) lowercase operations.
- **Impact**: Moderate in large lists.
- **Suggested Fix**: Lowercase the query once and build a `Vec<(lowercased_item, original_item)>` for the sort step.

---

## 7. MEDIUM — `highlighter.rs`: No Apostrophe Escaping in `escape_html`

- **File**: `frontend/src/utils/highlighter.rs`
- **Lines**: 1-7
- **Category**: Security / XSS
- **Description**: The `escape_html` function escapes `&`, `<`, `>`, and `"`, but does NOT escape `'`. While this is generally acceptable when attribute values are quoted with double quotes, mixed quoting or unquoted attributes could still be exploited. More importantly, the `escape_text_nodes` function (line 17) also skips the single quote.
- **Impact**: Low. Most modern HTML uses double-quoted attributes.
- **Suggested Fix**: Add `'` → `&#39;` escaping.

---

## 8. MEDIUM — `highlighter.rs`: Massive Code Duplication

- **File**: `frontend/src/utils/highlighter.rs`
- **Lines**: All
- **Category**: Maintainability / Code Quality
- **Description**: The file is ~1860 lines with massive duplication across ~8 language highlighters. The line-number prefix stripping, string/comment/number parsing, and HTML escaping logic is copy-pasted with minor variations. This is bug-prone (e.g., the `highlight_html_line` comment handling is fragile) and makes the file extremely difficult to maintain.
- **Impact**: Moderate. Any bugfix needs to be applied in 8 places.
- **Suggested Fix**: Refactor into a shared tokenization engine or use a lightweight syntax highlighting crate like `syntect`.

---

## 9. MEDIUM — `themes/mod.rs`: `set_css_property` Uses `Function::new_no_args` — Dangerous

- **File**: `frontend/src/themes/mod.rs`
- **Lines**: 118-126, 127-134
- **Category**: Security / Code Injection
- **Description**: The `set_css_property` and `set_data_theme` functions build JavaScript code via string concatenation and execute it via `js_sys::Function::new_no_args`. While `value` has basic `'` and `"` escaping,, this is a dangerous pattern. If the `value` parameter can ever be influenced by user input (e.g., a custom theme color), it could inject arbitrary JavaScript.
- **Impact**: Moderate. Could lead to XSS if custom theme values are user-controlled.
- **Suggested Fix**: Use `web_sys::Document::set_property` or `set_attribute` directly instead of `eval`-like `Function` creation. Avoid any `eval` pattern in the frontend.

---

## 10. MEDIUM — `agent_commands.rs`: Hardcoded "dangerously-skip-permissions" Flag

- **File**: `frontend/src/utils/agent_commands.rs`
- **Lines**: 6, 17-21
- **Category**: Security / Policy Bypass
- **Description**: The code includes a `CLAUDE_SKIP_PERMISSIONS_FLAG` constant and a `bypass` parameter that allows unconditionally bypassing Claude Code's permission system. If the `bypass` parameter can ever be controlled by a user-facing setting or malicious input, it opens an attack path.
- **Impact**: Moderate to High. Permission bypass in a CLI tool could allow destructive operations.
- **Suggested Fix**: The `bypass` boolean should be tightly controlled, never exposed in UI without warnings, and the default should be `false`. Add logging/auditing when bypass is used.

---

## 11. LOW — `circuit_breaker.rs`: `failure_count` Not Reset on `record_success`

- **File**: `frontend/src/utils/circuit_breaker.rs`
- **Lines**: 260-261
- **Category**: Logic Bug
- **Description**: When `record_success()` is called in the `Closed` state, only `consecutive_failures` is reset. The `failure_count` (which is the count of failures in the current monitoring window) is NOT reset, so the circuit can trip based on stale failures. While `prune_failures` is called in `record_failure`, it's NOT called in `record_success`, so old failures can still trip the breaker.
- **Impact**: Low to Moderate. The circuit breaker may trip too aggressively after a recovery.
- **Suggested Fix**: Call `prune_failures` in `record_success`, or reset `failure_count` when transitioning from HalfOpen to Closed.

---

## 12. LOW — `platform_utils.rs`: No Fallback for `web_sys::window()` Failure

- **File**: `frontend/src/utils/platform_utils.rs`
- **Lines**: 12-20
- **Category**: Reliability
- **Description**: If `web_sys::window()` returns `None` (e.g., in a non-browser environment or worker thread), `is_mac()` returns `false`, which biases the default shell to `/bin/bash` or `cmd.exe`. More critically, `get_default_shell()` calls `web_sys::window()` independently without checking `is_mac()`, causing an additional user agent check. Neither function is memoized, resulting in repeated DOM calls.
- **Impact**: Low. Unnecessary DOM queries.
- **Suggested Fix**: Cache the user agent string in a `Lazy` block. Unify platform detection.

---

## 14. LOW — `fuzzy_search.rs` `get_entries` Filter Logic Fails for Empty `limit`

- **File**: `frontend/src/utils/fuzzy_search.rs` (actually in `assistant_logger.rs`)
- **File**: `frontend/src/utils/assistant_logger.rs`
- **Lines**: 155-160
- **Category**: Logic Bug
- **Description**: The `get_entries` method with a `limit` uses `results.len().saturating_sub(limit) / split_off(start)`. If `limit` is greater than the result length, `start` can be > 0, which drops the oldest entries. This is semantically correct as a "last N", but the API naming (`limit`) does not make it clear it's a "last N" not a "first N". Confusing API.
- **Impact**: Low. Wrong items might be returned.
- **Suggested Fix**: Make the limit behavior explicit: limit should probably return `results.into_iter().take(limit).collect()`. The current code returns the *last* `limit` entries, not the *first*.

---

## 15. LOW — `command_parser.rs` Potential for Catastrophic Backtracking

- **File**: `frontend/src/utils/command_parser.rs`
- **Lines**: 11-22
- **Category**: Performance / Security
- **Description**: The `ANSI_STRIP_RE` is a complex regex with multiple alternations. The `[\x00-\x08\x0b\x0c\x0e-\x1a]` pattern could trigger catastrophic backtracking on specially crafted input or long strings.
- **Impact**: Low. WASM context limits the impact.
- **Suggested Fix**: Simplify the regex or use a state machine parser. The `regex` crate has some backtracking limits, but long strings could still cause hangs.

---

## 16. LOW — `plugin.rs` `PluginEventPayload` Has Many `Option` Fields — API Brittleness
- **File**: `frontend/src/types/plugin.rs`
- **Category**: Design
- **Description**: `PluginEventPayload` has ~20 `Option` fields. This makes it very easy to create invalid payloads by missing a required field. All fields being optional means no compile-time validation.
- **Impact**: Low.
- **Suggested Fix**: Separate into event-specific structs (`NotificationPayload`, `TaskPayload`, etc.) and use an enum.

---

## 17. LOW — `tauri.conf.json` `frame-src` is Too Permissive
- **File**: `src-tauri/tauri.conf.json`
- **Line**: 22
- **Category**: Security
- **Description**: `frame-src 'self' *` allows framing any origin. This can enable clickjacking if the app ever displays external embedded content.
- **Impact**: Low.
- **Suggested Fix**: Restriction to specific, trusted origins.

---

## 18. LOW — `tauri-plugin-webdriver-automation` Local Patch
- **File**: `Cargo.toml` (root)
- **Lines**: 39-40
- **Category**: Build / Supply Chain
- **Description**: A local patch is used for `tauri-plugin-webdriver-automation`. Patches are fine, but the `Cargo.lock` should be verified to ensure the patch is actually being applied and isn't accidentally pulling from crates.io.
- **Impact**: Low.
- **Suggested Fix**: Verify in `Cargo.lock` that the patch is active.

---

## 19. LOW — `main.rs` `store_api_key` and `clear_api_key` Security
- **File**: `src-tauri/src/main.rs`
- **Lines**: 150-151
- **Category**: Security
- **Description**: API key storage commands are registered. Without reading the command implementations, we cannot verify if they use proper keychain integration or if they store keys in plaintext.
- **Impact**: Unknown.
- **Suggested Fix**: Ensure `store_api_key` uses the `keyring` crate and never logs or serializes the key.

---

## 20. LOW — `escape_html` Escapes After Tag Construction (Incorrect Application)
- **File**: `frontend/src/utils/highlighter.rs`
- **Category**: Logic / Security
- **Description**: The `escape_text_nodes` function (lines 17-41) is applied AFTER the syntax highlighter has already inserted `<span>` tags. This means the inner content of the `<span>`'ed code is NOT escaped, which could allow XSS if the source code contains `</span><script>...</script>`. The current code relies on the highlighter not emitting raw HTML from code content, but this is a fragile invariant.
- **Impact**: Low. The highlighter is applied to user code but in an internal app context.
- **Suggested Fix**: Apply `escape_html` to the raw code string BEFORE it sees the highlighter, then post-process to strip escaping from highlighted tokens.

