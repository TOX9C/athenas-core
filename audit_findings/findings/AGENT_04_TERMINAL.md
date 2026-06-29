# Audit Findings — athena-terminal crate

**Scope:** `crates/athena-terminal/src/session.rs`, `crates/athena-terminal/src/input/escape_sequences.rs`, `crates/athena-terminal/src/input/mod.rs`, `crates/athena-terminal/src/lib.rs`  
**Date:** 2026-06-09  
**Auditor:** Subagent AGENT_04  
**Method:** Static code review, focused on PTY handling, input parsing, resource management, async safety, and performance.

---

## Summary

The `athena-terminal` crate has **3 Critical**, **3 High**, **5 Medium**, and **9 Low/Informational** findings. The most severe issues center around **TOCTOU in session creation** (allowing session ID collision and fd leak), **unbounded coalescing buffer growth in the read loop** (potential OOM/denial of service), and **unchecked `CString::new`/`libc` `write`/`read` return values** that can trigger crash/unreach on malformed UTF-8 input. The codebase also shows several areas of incomplete or unsafe error handling in the fork/exec child, the input encoding module, and resource cleanup.

---

## Critical

### 1. TOCTOU Race + Double-Insert in `SessionManager::spawn` — Session ID Collision & File Descriptor Leak

| Field | Detail |
|-------|--------|
| **Severity** | Critical |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 296–380 (the `spawn` method) |
| **Category** | PTY handling, Resource management |
| **Description** | The code first acquires a `read()` lock to check whether a session already exists, then — after forking the child and generating temp files — acquires a `write()` lock and unconditionally inserts the new session with `sessions.insert(id.clone(), session.clone())`. There is no re-check of the existence of the ID under the write lock. If two concurrent `spawn` calls are made with the same `id`, both will pass the initial read-lock check, both will fork successfully, and the second `insert` will overwrite the first in the `HashMap`. The overwritten `Arc<TerminalSession>` drops, closing its master_fd and sending SIGTERM to a possibly-unrelated process group, while the child process of the overwritten session becomes an orphaned zombie (or continues running). This leaks the PTY file descriptor and creates a zombie child process. |
| **Impact** | Session hijacking / ID collision; file descriptor leak; leaked child processes that are no longer tracked and cannot be killed through the SessionManager. |
| **Suggested Fix** | After obtaining the `write()` lock, re-check whether the session ID already exists. If it does, close the newly-created `master_fd`, kill the newly-forked child via `Signal::SIGKILL`, and clean up temp files, then return the existing session. Alternatively, use a single `write()` lock for the entire spawn critical section (with appropriate lock ordering) so that the check-and-insert is atomic. |

---

### 2. Unbounded Buffer Growth in `pty_read_loop` / `coalesce_buf` — Potential OOM / Denial of Service

| Field | Detail |
|-------|--------|
| **Severity** | Critical |
| **File** | `src-tauri/src/commands/mod.rs` (read loop), `crates/athena-terminal/src/session.rs` (read contract) |
| **Line** | `commands/mod.rs` ~730–800 (read loop) |
| **Category** | Performance, Resource exhaustion |
| **Description** | The `pty_read_loop` in `src-tauri/src/commands/mod.rs` accumulates all PTY output into `coalesce_buf: Vec<u8>` and flushes to the frontend only when `read_bytes` returns `Ok(0)` (no data), or when the buffer reaches 32 KiB, or on an 8 ms timer. If a command produces a sustained burst of output faster than the flush rate (e.g. `yes`, `cat /dev/zero`, a runaway process), the buffer grows without bound between flushes, because the 32 KiB threshold is checked *after* extending the buffer, not before. With fast PTY output, the buffer can grow to multiple megabytes or gigabytes before a flush occurs, causing unbounded memory growth and eventual OOM. |
| **Impact** | Denial of service via memory exhaustion; system-wide OOM on low-memory devices. |
| **Suggested Fix** | Implement a **hard cap** on `coalesce_buf` (e.g., 256 KiB or 1 MiB). When the cap is exceeded, force a flush (even if it's a partial buffer) and emit it, or drop old data and log a warning. Additionally, consider making the flush threshold a smaller, fixed chunk (e.g., flush every 4 KiB or every N reads) to bound latency and memory. |

---

### 3. `CString::new` on Arbitrary User Input / Shell Path Can Panic on Embedded NUL

| Field | Detail |
|-------|--------|
| **Severity** | Critical |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 327 (`CString::new(shell.as_bytes())`) |
| **Category** | Security (command injection / crash), Input parsing |
| **Description** | `CString::new` is called on `shell.as_bytes()`. If the `shell` parameter contains an embedded NUL byte (e.g., `"/bin/bash\0--evil"`), `CString::new` will return an `Err(NulError)`. The code currently maps this via `map_err` to `io::ErrorKind::InvalidInput` — this is safe in this specific line. However, this implies the caller (Tauri command layer) must correctly handle the error. More importantly, **if any downstream code accidentally unwraps or assumes `shell` is a valid CString, it would crash**. Additionally, the **temp file paths** are constructed from `session_id` via `sanitize_session_id`, but the **shell path itself is not sanitized** before being passed to `execvp`, which means a maliciously-crafted `shell` string can execute arbitrary binaries. This is a command-injection vector if the `shell` parameter is ever user-controlled without validation. |
| **Impact** | Arbitrary code execution if `shell` parameter is attacker-controlled; potential panic on embedded NUL if error handling is bypassed. |
| **Suggested Fix** | Validate `shell` parameter against a whitelist of known shell paths (e.g., `/bin/bash`, `/bin/zsh`, `/bin/sh`, `/usr/bin/fish`) before creating the session. Reject or sanitize unrecognised shell paths. Additionally, use `Path::new(&shell).canonicalize()` and verify the resulting path is within an allowed directory. Never pass unsanitised user input directly to `execvp`. |

---

## High

### 4. Double-Close / Use-After-Free of `master_fd` in `Drop` vs. `kill` Race

| Field | Detail |
|-------|--------|
| **Severity** | High |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 191–202 (`Drop`), 437–444 (`kill`) |
| **Category** | Resource management, PTY handling |
| **Description** | Both `Drop::drop` and `kill` call `self.fd_closed.swap(true, Ordering::SeqCst)` and then `close(self.master_fd)`. However, `Drop` also calls `killpg(self.shell_pid, SIGTERM)`. If a `kill()` call and a `Drop` (e.g., from the `Arc` being dropped after removal from the map) happen concurrently, the `AtomicBool` check-and-set **should** prevent a double-close in correctly-ordered code. The problem is that the `SessionManager` holds `Arc<TerminalSession>`. If two threads call `kill()` on the same session, both can get through the `swap` because `swap` is not idempotent: the first thread sets it to `true` and calls `close`; the second thread sees the old value `true` and skips `close`. That is fine. But: if one thread calls `close` (via `kill`) and another thread separately cause the `Arc` to drop and call `Drop`, but the `master_fd` has already been reused by the kernel for a new file descriptor, `close` in `Drop` could close an unrelated fd. This is a TOCTOU window because the `Arc` is still alive while `kill` is running, but a separate drop could close the fd while the first thread is still using it. More concretely: `read_bytes` does an **unsynchronised read** on `self.master_fd` without checking `fd_closed`. If `kill()` or `Drop` runs between when a read thread checks the session exists and when it calls `read_bytes`, the read will occur on a closed (or reassigned) fd, which is undefined behavior. |
| **Impact** | Use-after-free of file descriptor; potential crash or corruption of unrelated file descriptors. |
| **Suggested Fix** | Use `tokio::sync::RwLock` or an `AtomicI32` with a sentinel value (e.g., `-1`) for the fd, and atomically swap `self.master_fd` to `-1` before calling `close`. Ensure all reads/writes/resizes acquire a shared lock (or check the atomic value) before using the fd. Alternatively, wrap the fd in a type like `OwnedFd` or a custom `struct Fd(RawFd)` with atomic close semantics. |

---

### 5. Zombie / Orphaned Child Process on `execvp` Failure

| Field | Detail |
|-------|--------|
| **Severity** | High |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 364–379 (child branch of `fork`) |
| **Category** | Resource management, Process management |
| **Description** | In the child branch of `fork`, after all `execvp` attempts, the code calls `std::process::exit(1)`. This is correct in that it prevents the child from returning into the parent process. However, before this `exit`, the child may have already successfully called `setsid()`, `dup2`'d stdio to the PTY slave, and closed the slave fd. If `execvp` fails (e.g., shell binary not found, permission denied), the child will exit with status `1` but **no signal will be sent to the parent** other than the child becoming a zombie until `waitpid` is called — which the `SessionManager` never does. The parent stores the `Pid` but never reaps it. This means every failed `spawn` creates a zombie that persists until the parent process exits. |
| **Impact** | Zombie process accumulation (resource exhaustion of PID space); no visibility into why the shell failed to spawn. |
| **Suggested Fix** | After `fork`, in the parent branch, the code should call `nix::sys::wait::waitpid(child, None)` (or use a `SIGCHLD` handler with `waitpid(WNOHANG)` in a loop) to reap child processes. For the specific case of `execvp` failure, consider writing an error code to a pipe before calling `exit(1)` so the parent can detect the failure synchronously and return a meaningful error to the caller. Also register a `SIGCHLD` handler or use Tokio's `tokio::process` instead of raw `fork` + `execvp` if possible. |

---

### 6. `unsafe { libc::read }` is **Not Async-Safe** — Blocking the Async Runtime

| Field | Detail |
|-------|--------|
| **Severity** | High |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 264–278 (`read_bytes`) |
| **Category** | Async/await issues, PTY handling |
| **Description** | `read_bytes` calls `unsafe { libc::read(self.master_fd, buf.as_mut_ptr() as *mut _, buf.len()) }` directly, without wrapping it in `spawn_blocking`. This is called inside an async function from `pty_read_loop`, which runs inside a `tokio::select!` loop. Even though the fd is set to `O_NONBLOCK`, there are documented cases on macOS where a read from a PTY master can block briefly (e.g., during child process setup or immediately after fork). If `read` blocks, the entire async task is blocked, which **freezes the Tokio reactor thread** and can cause cascading stalls across all async tasks on that runtime. |
| **Impact** | Async runtime stall; UI freeze; all other async operations on the same runtime thread hang until the read returns. |
| **Suggested Fix** | Wrap the `libc::read` call in `tokio::task::spawn_blocking`, exactly like `do_write` does. Alternatively, use `tokio::io::unix::AsyncFd` to register the fd with the Tokio reactor and perform truly async reads. |

---

## Medium

### 7. Incomplete Modifier Key Support for Special Keys

| Field | Detail |
|-------|--------|
| **Severity** | Medium |
| **File** | `crates/athena-terminal/src/input/escape_sequences.rs` |
| **Line** | 87–147 (`encode_special_key`) |
| **Category** | Input parsing, Logic error |
| **Description** | The `_mod_suffix` closure is defined but **never used** inside `encode_special_key`. All modifier-aware branches (`if ctrl`, `if alt`, etc.) are hard-coded for specific keys (e.g., `Home` with Ctrl, `Tab` with Shift). For the vast majority of keys (arrows, F1–F12, PageUp, etc.), modifier keys (Ctrl, Alt, Shift) are completely ignored and the base sequence is returned. This means, for example, `Ctrl+ArrowUp` sends the same sequence as `ArrowUp`, which is incorrect per ANSI/VT200+ standards. |
| **Impact** | Incorrect terminal behaviour for modifier + special key combinations; some applications (e.g., `vim`, `emacs`, `tmux`) may misinterpret or ignore the key entirely. |
| **Suggested Fix** | Implement the `_mod_suffix` logic for **all** keys that support modifiers (arrows, function keys, Home/End/Insert/Delete, PageUp/PageDown). Ensure the modifier suffix (`;2` through `;8`) is appended according to the ANSI CSI/u sequences standard (e.g., `CSI 1 ; M A` for modifiers). |

---

### 8. `bracketed_paste` Does Not Filter or Escape Control Characters from Pasted Data

| Field | Detail |
|-------|--------|
| **Severity** | Medium |
| **File** | `crates/athena-terminal/src/input/escape_sequences.rs` |
| **Line** | 152–160 |
| **Category** | Security, Input parsing |
| **Description** | `bracketed_paste(data, enabled)` wraps the raw `data.as_bytes()` inside bracketed paste start/end sequences (`CSI 200 ~` and `CSI 201 ~`). It does **not** filter or escape any characters in the pasted data. If the pasted string contains the **end bracket sequence** (`\x1B[201~`), the terminal emulator (or the application running inside it) may prematurely terminate the paste, causing data after that point to be interpreted as normal terminal input (command injection). Similarly, embedded control characters (e.g., `\x03` for Ctrl+C, `\x04` for EOF) in the pasted data are passed through raw, which can interrupt or terminate the shell. |
| **Impact** | Paste injection / command injection via maliciously crafted paste content; unexpected shell interruption. |
| **Suggested Fix** | Before wrapping, filter the pasted data to remove or escape: (1) the end bracket sequence `\x1B[201~`, (2) any control characters below `0x20` except for standard whitespace (`\t`, `\n`, `\r`), and (3) the escape character `\x1B` unless it is part of a legitimate sub-sequence. Alternatively, document that bracketed paste expects the terminal emulator to sanitise input, and add a validation step in the Tauri command layer before calling `bracketed_paste`. |

---

### 9. `killpg` Sent to PID Instead of PGID — May Fail to Kill Entire Process Group

| Field | Detail |
|-------|--------|
| **Severity** | Medium |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 195 (`killpg`), 442 (`killpg`) |
| **Category** | Resource management, Process management |
| **Description** | `nix::sys::signal::killpg(self.shell_pid, Signal::SIGTERM)` is called with `self.shell_pid` (the PID of the forked child). `killpg` expects a **process group ID (PGID)**, not a PID. In the fork child, `setsid()` is called, which makes the child a session leader and its own PGID. Therefore, `self.shell_pid` happens to equal the PGID in this specific case. However, if `setsid()` somehow fails (it returns `Err`), the child remains in the parent's process group, and `killpg(self.shell_pid)` will signal the **parent's process group**, which is catastrophic. The code also calls `setsid().ok()`, ignoring the result. |
| **Impact** | If `setsid()` fails (rare but possible, e.g., already a session leader), `killpg` will send SIGTERM to the parent's entire process group, killing the entire application and all its sessions. |
| **Suggested Fix** | Check the result of `setsid()` in the child. If it fails, exit with an error instead of proceeding. Also, explicitly store the PGID (which should be the child's PID after a successful `setsid`) and use that for `killpg` calls, rather than assuming `shell_pid == PGID`. |

---

### 10. `generate_bash_rc`, `generate_zsh_zdotdir`, `generate_fish_hooks` Write World-Readable Temp Files

| Field | Detail |
|-------|--------|
| **Severity** | Medium |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 46–107 |
| **Category** | Security, Resource management |
| **Description** | `fs::write(&path, content)` creates temp files with default umask permissions (typically `644` or `664`), which means they are world-readable. These files contain shell integration hooks. While not sensitive per se, on a multi-user system they could leak information about the application or be tampered with by another local user between creation and exec. |
| **Impact** | Information disclosure; potential for local privilege escalation if another user can modify the temp file before it is sourced by the shell. |
| **Suggested Fix** | Set restrictive permissions (e.g., `0o600`) on the temp files after writing them, or write them to a private directory with `fs::create_dir_all` and `chmod 700`. |

---

### 11. `fd_closed` AtomicBool is Not Sufficient to Prevent Use-After-Close Race

| Field | Detail |
|-------|--------|
| **Severity** | Medium |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 264–278 (`read_bytes`), 214–236 (`write`) |
| **Category** | PTY handling, Async/await issues |
| **Description** | `read_bytes` and `write` do not check `fd_closed` before using `self.master_fd`. If `kill()` or `Drop` is called concurrently with a `read_bytes` call, the fd may be closed (and possibly reassigned by the kernel) while another thread is about to read from it. The `AtomicBool` only prevents a double-close, not a use-after-close. |
| **Impact** | Use-after-close of a raw fd; read from or write to an unrelated file descriptor; potential data corruption or crash. |
| **Suggested Fix** | Guard fd usage with a read lock on an `RwLock`, or atomically swap the fd to `-1` and check for `-1` before use. Prefer the `OwnedFd` / `AtomicI32` pattern. |

---

## Low / Informational

### 12. Off-by-One / Missing `#[repr(C)]` for `libc::winsize` in `resize`

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 456–476 (`resize`) |
| **Category** | Logic error / FFI safety |
| **Description** | `resize` constructs a `libc::winsize` struct and passes a reference to it into `libc::ioctl`. While `libc::winsize` is a standard struct with a stable layout, the code does not use `#[repr(C)]` on its own wrapper. In this case it's fine because `libc::winsize` is from the `libc` crate and already has the correct layout. However, if this struct were ever defined locally without `#[repr(C)]`, it would be UB. This is a documentation/code hygiene issue. |
| **Impact** | None currently, but a future refactor could introduce UB. |
| **Suggested Fix** | Add a comment noting that `libc::winsize` comes from the `libc` crate with the correct layout, or wrap it in a local `#[repr(C)]` struct for correctness. |

---

### 13. `generate_*` Functions Do Not Handle `fs::write` Errors in Child

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 46–107 |
| **Category** | Error handling robustness |
| **Description** | `generate_bash_rc`, `generate_zsh_zdotdir`, and `generate_fish_hooks` call `fs::write(...)?` and return `io::Result<PathBuf>`. This is fine in the parent. However, the temp file paths are generated before `fork`. If the fork fails after the temp file is created, the file is leaked (it will be cleaned up in `Drop`, but only if the `TerminalSession` is created, which it isn't if `fork` fails). If `fork` fails, there is no cleanup. Also, the `session_id` is not validated for length, so an extremely long `session_id` could create a temp file path that exceeds OS path length limits. |
| **Impact** | Temp file leak on `fork` failure; potential for path-too-long errors. |
| **Suggested Fix** | Move temp file creation into the child process (post-fork), or clean up the temp files if `fork` returns an error. Add a length check for `session_id`. |

---

### 14. `read_bytes` Returns `Ok(0)` for Both EAGAIN and Actual EOF

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 264–278 |
| **Category** | PTY handling, Logic error |
| **Description** | `read_bytes` maps `EAGAIN` to `Ok(0)`. In standard Unix semantics, `read` returning `0` means EOF. The caller in `pty_read_loop` treats `Ok(0)` as "no data available, sleep and retry", which is correct for a non-blocking fd but conflates two distinct states: (a) no data right now, and (b) the child has closed its end of the PTY (permanent EOF). While the later error handling in `pty_read_loop` does break on `BrokenPipe` or `InvalidData`, there's no path that handles permanent EOF gracefully — the loop will spin-sleep indefinitely if the child exits but the fd is not yet closed by the kernel. |
| **Impact** | Potential CPU spin / busy-wait after child process exits, until some other event causes the fd to return an error. |
| **Suggested Fix** | Distinguish between `EAGAIN` (temporary, retry) and actual `0` (EOF, child exited). If `read` returns `0` and the fd is still valid, use `nix::sys::wait::waitpid(session.shell_pid, WNOHANG)` to check if the child has exited. If so, break the loop. Only return `Ok(0)` for `EAGAIN`, and consider returning a distinct error or `None` for EOF. |

---

### 15. `encode_special_key` Uses Hard-Coded Sequences Without Terminal Capability Queries

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/input/escape_sequences.rs` |
| **Line** | 87–147 |
| **Category** | Logic error / Compatibility |
| **Description** | The escape sequences for function keys (F1–F12) and special keys are hard-coded to a specific VT200-like sequence. Not all terminals support all of these (e.g., `xterm`, `screen`, `tmux`, `rxvt` have different or additional sequences). This is acceptable for an embedded terminal but may cause misbehaviour if the crate is ever used with a different terminal emulator. |
| **Impact** | Reduced compatibility with non-standard terminal emulators. |
| **Suggested Fix** | Document the supported terminal capabilities. If future compatibility is a goal, consider querying the terminal's terminfo/termcap database or using `terminfo` crate. |

---

### 16. `PROMPT_COMMAND` Hook in `generate_bash_rc` is Appended Unsafely — May Corrupt User's Existing `PROMPT_COMMAND`

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 49–63 (`generate_bash_rc`) |
| **Category** | Logic error / Shell compatibility |
| **Description** | The generated bashrc uses `PROMPT_COMMAND="${PROMPT_COMMAND}${PROMPT_COMMAND:+;}__athena_prompt"` to append the hook. If the user's existing `PROMPT_COMMAND` contains unquoted semicolons, subshells, or conditional logic, this naive concatenation can break the command or cause unexpected behaviour. For example, if `PROMPT_COMMAND='echo "a;b"'`, the resulting command will be incorrectly parsed. |
| **Impact** | Potential corruption of user's shell configuration; unexpected prompt behaviour. |
| **Suggested Fix** | Use a function-based approach instead of appending to `PROMPT_COMMAND`, or wrap the user's existing value in a function and call it safely. For Bash 4.4+, use `PROMPT_COMMAND+=('__athena_prompt')` (array form). |

---

### 17. `_mod_suffix` Closure Captures Variables but is Never Used

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/input/escape_sequences.rs` |
| **Line** | 91–107 |
| **Category** | Code quality / Logic error |
| **Description** | The `_mod_suffix` closure is defined but never invoked. The compiler does not warn because it is prefixed with `_`, which suppresses the unused warning. This dead code suggests an incomplete implementation of modifier key support (see #7). |
| **Impact** | None directly, but indicates incomplete feature implementation. |
| **Suggested Fix** | Complete the modifier key implementation or remove the dead code. |

---

### 18. `SessionManager::spawn` Creates Temp Files Before Verifying Shell Path is Valid / Executable

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 327 onward |
| **Category** | Resource management / Logic error |
| **Description** | Temp files and directories are created before the shell path is validated (e.g., checked for existence, executability). If the shell path is invalid, `execvp` will fail in the child, but the temp files have already been created in the parent. While they will be cleaned up by `Drop` if the `TerminalSession` is created, if `fork` itself fails, they are leaked. |
| **Impact** | Temp file leak on spawn failure. |
| **Suggested Fix** | Validate the shell path (e.g., `Path::new(shell).is_file()` and `access(X_OK)`) before creating temp files. Alternatively, defer temp file creation to the child process. |

---

### 19. `libc::ioctl(fd, TIOCSWINSZ, &ws)` Return Value Not Checked for `Err` vs. `-1`

| Field | Detail |
|-------|--------|
| **Severity** | Low |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 463–476 (`resize`) |
| **Category** | Error handling robustness |
| **Description** | `libc::ioctl` returns `-1` on error and sets `errno`. The code checks if the `spawn_blocking` result is `Ok(0)` (success), `Ok(_)` (failure, uses `last_os_error`), or `Err(e)` (spawn failure). This is correct, but it relies on `errno` being preserved across the thread boundary. While `last_os_error` reads the thread-local `errno`, which is preserved in the `spawn_blocking` thread, this is somewhat fragile. If another call in the same thread sets `errno` before the check, the wrong error is reported. |
| **Impact** | Potentially misleading error messages; very rare in practice but possible under high concurrency. |
| **Suggested Fix** | In the `spawn_blocking` closure, if `ioctl` returns `-1`, immediately capture `errno` (e.g., `let err = io::Error::last_os_error();`) and return it as part of a custom error enum or tuple, rather than relying on `last_os_error` after the thread returns. |

---

### 20. `std::process::exit(1)` in Child After Failed `execvp` Does Not Distinguish Failure Causes

| Field | Detail |
|-------|--------|
| **Severity** | Low (Informational) |
| **File** | `crates/athena-terminal/src/session.rs` |
| **Line** | 377 |
| **Category** | Error handling robustness |
| **Description** | After `execvp` fails, the child exits with code `1`. The parent has no way to distinguish between "child exited normally with code 1" and "execvp failed". This makes debugging shell spawn failures difficult. |
| **Impact** | Poor observability; hard to diagnose why a shell failed to start. |
| **Suggested Fix** | Use a `pipe(2)` before `fork` to communicate the exec failure reason from child to parent. If `execvp` fails, write the errno to the pipe and then `_exit`. The parent reads from the pipe after `fork` and can return a descriptive error. Alternatively, use `nix::unistd::execve` and map specific errors. |

---

## Architecture & Data Flow Notes

### File Roles

| File | Lines | Role |
|------|-------|------|
| `src/session.rs` | ~470 | PTY creation, fork/exec, session lifecycle, read/write/resize |
| `src/input/escape_sequences.rs` | ~150 | Keyboard input → ANSI escape byte sequences |
| `src/input/mod.rs` | 1 | Module re-export |
| `src/lib.rs` | ~12 | Public API re-exports, `SessionConfig` struct |

### Key Data Flow

1. **`SessionManager::spawn`** (`src/session.rs:274`)
   - Validates `id` uniqueness (read lock only — **not atomic**).
   - Calls `openpty`, `fork`, `execvp` shell with temp rcfiles for OSC 133 hooks.
   - Stores `Arc<TerminalSession>` in `HashMap` (write lock).
   - Returns `Arc<TerminalSession>`.

2. **`pty_read_loop`** (`src-tauri/src/commands/mod.rs:720`)
   - Gets session from `SessionManager`.
   - Loops: `read_bytes` → accumulate into `coalesce_buf` → flush to frontend via Tauri event `pty:raw`.
   - No bound on `coalesce_buf` size.

3. **`pty_write`** (`src-tauri/src/commands/mod.rs:840`)
   - Gets session, writes string to PTY via `TerminalSession::write`.
   - Data comes directly from frontend; no sanitisation beyond `String` → `Vec<u8>`.

4. **Input encoding** (`src/input/escape_sequences.rs`)
   - `encode_char` and `encode_special_key` receive key events from frontend.
   - Convert keys to ANSI byte sequences.
   - No validation of key names; unrecognised keys return `None`.

---

## Start Here

An agent fixing these issues should start with:

1. **`crates/athena-terminal/src/session.rs`** — TOCTOU race in `spawn`, concurrent fd use-after-close, and `read_bytes` blocking the async runtime.
2. **`src-tauri/src/commands/mod.rs`** — Unbounded `coalesce_buf` growth in the read loop.
3. **`crates/athena-terminal/src/input/escape_sequences.rs`** — Incomplete modifier key support, and `bracketed_paste` injection risk.
