//! Unix PTY backend (macOS + Linux): openpty + fork + exec, unchanged
//! semantics from the pre-refactor session.rs — code moved, not rewritten.
//! The Windows ConPTY counterpart implements the same surface.

use super::{SpawnSpec, Spawned};
use nix::pty::{openpty, Winsize};
use nix::sys::wait::{waitpid, WaitPidFlag};
use nix::unistd::{close, fork, setsid, ForkResult};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::io::{IntoRawFd, RawFd};
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::Mutex;
use tokio::sync::mpsc;

/// Result of the non-blocking write fast path.
pub enum WriteFast {
    /// Fully written inline.
    Done(usize),
    /// Partially written; `remaining` must be re-driven on the blocking pool.
    Partial { written: usize, remaining: Vec<u8> },
    /// Pipe full — retry the whole buffer on the blocking pool.
    WouldBlock,
}
/// Reap a child process group after signaling it.
///
/// Sends SIGTERM, gives the group a short grace window to exit, then
/// escalates to SIGKILL if needed. `waitpid(WNOHANG)` is polled throughout so
/// the child does not become a zombie — without this, `Drop`/`kill` would
/// signal the group but never reap it, leaking process-table entries over the
/// app's lifetime.
///
/// `pid` is the child to reap; `pgid` is the group to signal (they differ when
/// `setsid` failed and the child stayed in the parent's group — in that case
/// we must NOT `killpg`, but we still reap our own child).
pub fn reap_child(pid: i32, pgid: i32) {
    use nix::sys::signal::{killpg, Signal};
    use nix::sys::wait::WaitStatus;
    use nix::unistd::Pid;

    let pid = Pid::from_raw(pid);
    let pgid = Pid::from_raw(pgid);

    // Check liveness FIRST: if the child already exited and the OS recycled
    // its pgid, killpg would signal an innocent, unrelated process group.
    match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
        Ok(WaitStatus::StillAlive) => {} // genuinely running — proceed to signal
        _ => return,                     // already reaped or nothing to do
    }

    let _ = killpg(pgid, Signal::SIGTERM);

    // Grace window: poll for ~200ms for the child to exit.
    let reaped = (0..20).any(|_| match waitpid(pid, Some(WaitPidFlag::WNOHANG)) {
        Ok(WaitStatus::StillAlive) => {
            std::thread::sleep(std::time::Duration::from_millis(10));
            false
        }
        Ok(_) => true,                   // Exited / Signaled / etc. — reaped.
        Err(nix::Error::ECHILD) => true, // Already reaped elsewhere.
        Err(_) => true,                  // Treat other errors as "nothing more to do".
    });
    if reaped {
        return;
    }

    // Still alive after the grace window — SIGKILL and reap (blocking).
    let _ = killpg(pgid, Signal::SIGKILL);
    let _ = waitpid(pid, None);
}

/// Platform-owned PTY master descriptor.
///
/// Atomic FD sentinel semantics are unchanged from the pre-refactor session:
/// holds the master PTY fd, or `-1` once the fd has been closed. PTY
/// operations duplicate this fd into an `OwnedFd` while holding
/// `fd_lifecycle_lock`, so closing the master cannot recycle the integer
/// used by an in-flight syscall.
pub struct ProcessHandle {
    fd: AtomicI32,
    /// Serializes master-fd duplication with closing the master fd. Each
    /// operation owns its duplicate for the duration of its syscall, so this
    /// lock is not held across blocking I/O.
    fd_lifecycle_lock: Mutex<()>,
}

impl ProcessHandle {
    pub fn new(master_fd: i32) -> Self {
        Self {
            fd: AtomicI32::new(master_fd),
            fd_lifecycle_lock: Mutex::new(()),
        }
    }

    /// Duplicate the live master fd while synchronizing with close. The
    /// returned descriptor is independently owned and remains valid even if
    /// `close_fd` recycles the original master-fd number before the syscall
    /// using this descriptor begins.
    pub fn duplicate_master_fd(&self) -> io::Result<OwnedFd> {
        let _fd_guard = self
            .fd_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fd = self.fd.load(Ordering::Acquire);
        if fd < 0 {
            return Err(io::Error::new(io::ErrorKind::BrokenPipe, "fd closed"));
        }
        let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
        if duplicate < 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fcntl returned a newly-owned descriptor on success.
        Ok(unsafe { OwnedFd::from_raw_fd(duplicate) })
    }

    /// Atomically close the master fd. Idempotent: subsequent calls are no-ops
    /// because the sentinel `-1` is swapped back to itself. The first caller
    /// to swap wins and is responsible for the `libc::close` call. In-flight
    /// operations use independent owned duplicates and therefore remain safe
    /// if the kernel reuses the master-fd integer after this returns.
    ///
    /// Returns `true` if this call actually closed an open fd, `false` if
    /// the fd was already closed.
    pub fn close_fd_with_reason(&self, reason: &'static str) -> bool {
        let _fd_guard = self
            .fd_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old = self.fd.swap(-1, Ordering::AcqRel);
        if old >= 0 {
            // SAFETY: `old` was a valid fd owned by this session; we are
            // the first thread to swap it out, so no one else will close
            // it. The kernel may reuse the integer for an unrelated fd
            // after this returns — callers must reload the atomic to
            // observe the new `-1` sentinel.
            unsafe { libc::close(old as RawFd) };
            log::debug!("PTY master fd closed: reason={}", reason);
            true
        } else {
            false
        }
    }

    /// Write fast path ONLY: one non-blocking write attempt. Returns
    /// Done(written), Partial { written, remaining } when a large paste
    /// landed partially, or WouldBlock when the pipe is full — the caller
    /// re-drives Partial/WouldBlock on the blocking pool. Never parks, so
    /// it is safe to call inline from an async worker (keystroke path).
    pub fn write_fast(&self, data: &[u8]) -> io::Result<WriteFast> {
        let owned_fd = self.duplicate_master_fd()?;
        let fd = owned_fd.as_raw_fd();
        let first = unsafe { libc::write(fd, data.as_ptr() as *const _, data.len()) };
        if first >= 0 {
            let written = first as usize;
            if written == data.len() {
                return Ok(WriteFast::Done(written));
            }
            // Rare partial write of a large paste: the caller re-drives
            // the remainder on the blocking pool.
            return Ok(WriteFast::Partial {
                written,
                remaining: data[written..].to_vec(),
            });
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            // Interrupted or pipe full: continue on the blocking path, which
            // polls for POLLOUT and retries.
            // EWOULDBLOCK == EAGAIN on macOS/Linux, so a single pattern
            // covers both.
            Some(libc::EINTR) | Some(libc::EAGAIN) => Ok(WriteFast::WouldBlock),
            _ => Err(err),
        }
    }

    /// Slow path: (re)write `data` from a blocking context, polling POLLOUT
    /// when the PTY buffer is full. Bounded 30 s wait so a dead PTY surfaces
    /// a clean error instead of hanging forever. Only valid on a blocking
    /// pool thread; the session layer never calls it inline.
    pub fn write_slow(&self, data: &[u8]) -> io::Result<usize> {
        let owned_fd = self.duplicate_master_fd()?;
        self.write_blocked_fd(owned_fd, data, 0)
    }

    fn write_blocked_fd(
        &self,
        owned_fd: OwnedFd,
        data: &[u8],
        done: usize,
    ) -> io::Result<usize> {
        let buf = data[done..].to_vec();
        let fd = owned_fd.as_raw_fd();
        let mut total_written = 0usize;
        // The master fd is O_NONBLOCK (set at spawn time), so the kernel
        // returns EAGAIN/EWOULDBLOCK when the PTY pipe buffer fills mid-write.
        // When that happens we `poll(2)` for `POLLOUT` — which blocks this
        // blocking task until the reader drains space — then retry, instead
        // of dropping the remainder. Without this a large paste would be
        // truncated and its tail silently lost.
        while total_written < buf.len() {
            let written = unsafe {
                libc::write(
                    fd,
                    buf.as_ptr().add(total_written) as *const _,
                    buf.len() - total_written,
                )
            };
            if written < 0 {
                let err = io::Error::last_os_error();
                let code = err.raw_os_error();
                if code == Some(libc::EINTR) {
                    continue;
                }
                // Pipe full / would block: wait until the fd is writable.
                if code == Some(libc::EAGAIN) || code == Some(libc::EWOULDBLOCK) {
                    let mut pfd = libc::pollfd {
                        fd,
                        events: libc::POLLOUT,
                        revents: 0,
                    };
                    // Bounded wait so a dead PTY surfaces a clean error
                    // instead of hanging forever. 30s is far beyond any
                    // realistic paste drain time.
                    let pr =
                        unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, 30_000_i32) };
                    if pr < 0 {
                        let perr = io::Error::last_os_error();
                        if perr.raw_os_error() == Some(libc::EINTR) {
                            continue;
                        }
                        return Err(perr);
                    }
                    if pr == 0 {
                        // Timed out waiting for writability.
                        return Err(io::Error::new(io::ErrorKind::WouldBlock, err));
                    }
                    continue;
                }
                // EIO means the slave side is gone (dead PTY); retrying can
                // never succeed, so surface it as a terminal error instead
                // of letting callers spin. (The spawn handshake already
                // queues writes until the child has exec'd, so a fresh-PTY
                // EIO race does not reach this path.)
                return Err(err);
            }
            total_written += written as usize;
        }
        Ok(done + total_written)
    }

    /// Read raw bytes from the PTY master fd without parsing.
    /// Returns the number of bytes read (`0` means no data was available
    /// within `wait_ms`).
    ///
    /// F10: when the non-blocking fd returns EAGAIN, the call parks in
    /// `poll(2)` for up to `wait_ms` waiting for POLLIN readiness instead of
    /// returning immediately and letting the caller sleep-and-retry. An idle
    /// pane therefore costs one parked syscall per `wait_ms` window (no
    /// dup+read spin), and arriving data wakes the poll instantly — keystroke
    /// latency is bounded by the kernel wakeup, not the wait window.
    ///
    /// This is the entry point for raw byte consumers (e.g. an xterm.js
    /// subscriber) that want the bytes *before* the VTE parser rewrites them
    /// into cell deltas.
    pub fn read_bytes(&self, buf: &mut [u8], wait_ms: u64) -> io::Result<usize> {
        let owned_fd = self.duplicate_master_fd()?;
        // The read itself is O_NONBLOCK: it completes immediately (data,
        // EAGAIN, or error), so it never parks the calling thread. Only the
        // poll parks; the poll closure touches no caller memory (it receives
        // the fd by value as a lease), so an abandoned poll leaves at worst
        // an orphan that polls and closes an owned dup — harmless.
        let nbytes =
            unsafe { libc::read(owned_fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if nbytes >= 0 {
            return Ok(nbytes as usize);
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::WouldBlock && e.raw_os_error() != Some(libc::EAGAIN) {
            return Err(e);
        }
        // EAGAIN: park in poll(2) until POLLIN readiness or the window
        // expires. The closure returns a readiness verdict only — the
        // consuming read happens below, inline, where the `buf` borrow is
        // live and cancellation-safe.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Wait {
            Ready,
            TimedOut,
        }
        let ready = {
            let fd = owned_fd.as_raw_fd();
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let pr = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, wait_ms as i32) };
            if pr < 0 {
                let perr = io::Error::last_os_error();
                // EINTR: treat as timeout; the caller's loop retries.
                if perr.raw_os_error() == Some(libc::EINTR) {
                    Wait::TimedOut
                } else {
                    return Err(perr);
                }
            } else if pr > 0 && pfd.revents & libc::POLLIN != 0 {
                Wait::Ready
            } else {
                Wait::TimedOut
            }
        };
        if ready != Wait::Ready {
            return Ok(0);
        }
        // Readiness confirmed: consume inline. The poll consumed the first
        // lease, so take a fresh one — if the session closed while we
        // waited, duplicate_master_fd surfaces BrokenPipe and the caller's
        // loop terminates. A spurious EAGAIN (another reader won the race)
        // reports no data for this round.
        let lease = self.duplicate_master_fd()?;
        let nbytes =
            unsafe { libc::read(lease.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if nbytes >= 0 {
            return Ok(nbytes as usize);
        }
        let e2 = io::Error::last_os_error();
        if e2.kind() == io::ErrorKind::WouldBlock || e2.raw_os_error() == Some(libc::EAGAIN) {
            return Ok(0);
        }
        Err(e2)
    }


    /// Apply a window-size change to the PTY (TIOCSWINSZ). Synchronous; the
    /// session layer runs it on the blocking pool while holding its resize
    /// lock so an older ioctl cannot finish after a newer request.
    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        let ws = libc::winsize {
            ws_col: cols,
            ws_row: rows,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };
        let owned_fd = self.duplicate_master_fd()?;
        let rc = unsafe { libc::ioctl(owned_fd.as_raw_fd(), libc::TIOCSWINSZ, &ws) };
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    /// Raw master-fd number for foreground-probing (tcgetpgrp) by the
    /// athena-core agent-detection layer. Unix-only; the Windows backend
    /// returns None and detection falls back to the spawn pgid.
    pub fn raw_master_fd(&self) -> Option<i32> {
        let fd = self.fd.load(Ordering::Acquire);
        if fd >= 0 {
            Some(fd)
        } else {
            None
        }
    }
}

/// Spawn a dedicated blocking reader thread for a session and return a
/// channel receiver of `Ok(chunk)` / terminal `Err`.
///
/// The thread owns one dup of the master fd and blocks in `poll(2)` —
/// no wake-up machinery on the tokio blocking pool, no adaptive backoff
/// loop, and no per-read fd duplication. An idle pane costs exactly one
/// parked `poll` syscall and one 100 ms deadline tick (used to notice a
/// dropped receiver so the thread never outlives its consumer).
///
/// Termination:
/// - slave side closed / child reaped → read returns 0 or EIO →
///   thread exits → the receiver yields `None`;
/// - consumer dropped → the next send fails (or a deadline tick sees
///   `is_closed`) → thread exits;
/// - real errors are forwarded once, then the thread exits.
pub fn spawn_reader(
    handle: &std::sync::Arc<ProcessHandle>,
    thread_name: String,
) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::channel::<io::Result<Vec<u8>>>(32);
    // Kept in case the thread cannot even be spawned — the error is
    // delivered through the channel so the caller takes its normal
    // error path.
    let fallback_tx = tx.clone();
    let handle = std::sync::Arc::clone(handle);
    let spawned = std::thread::Builder::new()
        .name(thread_name)
        .spawn(move || {
            let owned_fd = match handle.duplicate_master_fd() {
                Ok(fd) => fd,
                Err(e) => {
                    let _ = tx.blocking_send(Err(e));
                    return;
                }
            };
            let fd = owned_fd.as_raw_fd();
            let mut buf = vec![0u8; 16 * 1024];
            loop {
                let mut pfd = libc::pollfd {
                    fd,
                    events: libc::POLLIN,
                    revents: 0,
                };
                let rc = unsafe { libc::poll(&mut pfd as *mut libc::pollfd, 1, 100) };
                if rc < 0 {
                    let e = io::Error::last_os_error();
                    if e.raw_os_error() == Some(libc::EINTR) {
                        continue;
                    }
                    let _ = tx.blocking_send(Err(e));
                    return;
                }
                if rc == 0 {
                    // Deadline tick: exit when the consumer is gone.
                    if tx.is_closed() {
                        return;
                    }
                    continue;
                }
                let nbytes = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut _, buf.len()) };
                if nbytes > 0 {
                    // Swap the read buffer out instead of copying each
                    // chunk: one fresh allocation per delivery, no memcpy.
                    let n = nbytes as usize;
                    let mut chunk = std::mem::replace(&mut buf, vec![0u8; 16 * 1024]);
                    chunk.truncate(n);
                    if tx.blocking_send(Ok(chunk)).is_err() {
                        return;
                    }
                    continue;
                }
                if nbytes == 0 {
                    // EOF: the child closed the slave end.
                    return;
                }
                let e = io::Error::last_os_error();
                match e.raw_os_error() {
                    // The master fd is O_NONBLOCK; a POLLIN read raced
                    // with another consumer. Retry.
                    Some(libc::EAGAIN) | Some(libc::EINTR) => continue,
                    // Slave side gone (kill or exit).
                    Some(libc::EIO) => return,
                    _ => {
                        let _ = tx.blocking_send(Err(e));
                        return;
                    }
                }
            }
        });
    match spawned {
        Ok(_handle_thread) => rx,
        Err(e) => {
            // Thread spawn failure: deliver the error through the channel
            // so the caller's loop takes its normal error path.
            let _ = fallback_tx.blocking_send(Err(io::Error::other(e)));
            rx
        }
    }
}


// ─── Spawn: startup-file helpers (verbatim from session.rs) ─────────────────

static STARTUP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Base name of a shell path ("/bin/zsh" → "zsh").
pub fn shell_name(shell: &str) -> &str {
    std::path::Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

/// Cleanup a private startup file/directory (best-effort, both file and dir
/// forms attempted because zsh uses a generated ZDOTDIR).
pub fn cleanup_startup_path(path: Option<&std::path::Path>) {
    if let Some(path) = path {
        let _ = std::fs::remove_dir_all(path);
        let _ = std::fs::remove_file(path);
    }
}

/// Quote a path for a shell `source` command without allowing path characters
/// to become shell syntax. Startup files are generated by Athena, but the
/// user's home directory may contain spaces or apostrophes.
pub fn shell_single_quote(path: &std::path::Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Preserve the user's normal interactive shell configuration while keeping
/// Athena's integration script in a private startup file. The private file is
/// the only file passed to the child shell, so Athena can load hooks silently;
/// sourcing the real rc file first restores Oh My Zsh, prompts, aliases, and
/// user PATH setup.
pub fn startup_script_with_user_config(shell: &str, integration_script: &str) -> String {
    let user_home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    let user_rcs: Vec<std::path::PathBuf> = match shell_name(shell) {
        "zsh" => {
            let dir = std::env::var_os("ZDOTDIR")
                .map(std::path::PathBuf::from)
                .or_else(|| user_home.clone());
            dir.into_iter()
                .flat_map(|dir| [dir.join(".zshenv"), dir.join(".zshrc")])
                .collect()
        }
        "bash" => user_home
            .map(|dir| vec![dir.join(".bashrc")])
            .unwrap_or_default(),
        _ => Vec::new(),
    };

    let source_user = user_rcs
        .into_iter()
        .filter(|path| path.is_file())
        .map(|path| {
            let quoted = shell_single_quote(&path);
            format!("if [[ -r {quoted} ]]; then source {quoted}; fi\n")
        })
        .collect::<String>();

    // A user's rc file may assume Oh My Zsh's conventional ZSH variable is
    // exported by an outer login shell. Athena launches zsh directly with
    // `-d -i`, so make the well-known install location explicit before
    // sourcing `.zshrc`. This prevents `source /oh-my-zsh.sh` and the follow-on
    // `compdef: command not found` cascade without modifying the user's files.
    let omz_bootstrap = if shell_name(shell) == "zsh" {
        "if [[ -z \"${ZSH:-}\" || ! -f \"${ZSH}/oh-my-zsh.sh\" ]]; then\n  unset ZSH\n  for __athena_omz in \"$HOME/.oh-my-zsh\" \"$HOME/.config/oh-my-zsh\" \"/opt/homebrew/share/oh-my-zsh\" \"/usr/local/share/oh-my-zsh\"; do\n    if [[ -f \"$__athena_omz/oh-my-zsh.sh\" ]]; then export ZSH=\"$__athena_omz\"; break; fi\n  done\n  unset __athena_omz\nfi\n"
    } else {
        ""
    };

    format!("{omz_bootstrap}{source_user}\n{integration_script}\n")
}

pub fn shell_flags(shell: &str) -> &'static [&'static str] {
    match shell_name(shell) {
        "zsh" => &["-f", "-i"],
        "bash" => &["--noprofile", "--norc", "-i"],
        "fish" => &["--no-config", "--interactive"],
        "sh" => &["-i"],
        _ => &[],
    }
}

/// Build a predictable terminal environment while preserving the user's
/// normal environment for PATH and tool discovery. `TERM` and `COLORTERM`
/// are essential for ANSI/true-colour output from agent CLIs.
pub fn child_environment_with_startup(startup_env: Option<(&str, &std::path::Path)>) -> Vec<CString> {
    let mut values = BTreeMap::new();
    for (key, value) in std::env::vars() {
        values.insert(key, value);
    }
    values.insert("TERM".to_string(), "xterm-256color".to_string());
    values.insert("COLORTERM".to_string(), "truecolor".to_string());
    values.insert("TERM_PROGRAM".to_string(), "Athena".to_string());
    // Advertise Kitty graphics support: agent harnesses (OMP et al.) detect
    // terminal image capability from KITTY_WINDOW_ID and emit inline image
    // escapes instead of "[Image]" placeholders. The frontend's
    // addon-kitty-graphics.js renders those sequences. Sessions rendered by
    // the backend cell grid (non-xterm) drop them, matching prior behavior.
    values.insert("KITTY_WINDOW_ID".to_string(), "athena".to_string());
    values.insert("ATHENA_SHELL_INTEGRATION".to_string(), "1".to_string());
    if let Some((key, path)) = startup_env {
        values.insert(key.to_string(), path.to_string_lossy().into_owned());
    }
    let home_bin = values.get("HOME").map(|home| format!("{home}/.local/bin"));
    let bun_bin = values.get("HOME").map(|home| format!("{home}/.bun/bin"));
    if let Some(path) = values.get_mut("PATH") {
        let mut prefix = Vec::new();
        for entry in [
            Some("/opt/homebrew/bin".to_string()),
            Some("/usr/local/bin".to_string()),
            home_bin,
            bun_bin,
        ]
        .into_iter()
        .flatten()
        {
            if !path.split(':').any(|current| current == entry) {
                prefix.push(entry);
            }
        }
        if !prefix.is_empty() {
            *path = format!("{}:{path}", prefix.join(":"));
        }
    }
    values
        .into_iter()
        .filter_map(|(key, value)| CString::new(format!("{key}={value}")).ok())
        .collect()
}

/// Spawn the shell on a fresh PTY. This is the pre-refactor
/// `spawn_with_startup_script` platform half, moved verbatim; the session-map
/// bookkeeping (duplicate-id handling) stays in session.rs. Blocking: the
/// error-pipe read waits for the child's exec verdict, so the session layer
/// must call this via `spawn_blocking`.
pub fn new_session(spec: SpawnSpec<'_>) -> io::Result<Spawned> {
    let SpawnSpec {
        shell,
        cwd,
        cols,
        rows,
        startup_script,
    } = spec;

    // Create a pipe for the child to report pre-exec errors.
    // FD_CLOEXEC ensures the pipe is closed automatically on a successful exec.
    let (err_read, err_write) =
        nix::unistd::pipe().map_err(|e| io::Error::other(e.to_string()))?;
    let err_read = err_read.into_raw_fd();
    let err_write = err_write.into_raw_fd();
    unsafe {
        let flags = libc::fcntl(err_read, libc::F_GETFD, 0);
        libc::fcntl(err_read, libc::F_SETFD, flags | libc::FD_CLOEXEC);
        let flags = libc::fcntl(err_write, libc::F_GETFD, 0);
        libc::fcntl(err_write, libc::F_SETFD, flags | libc::FD_CLOEXEC);
    }

    let winsize = Winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    let pty = match openpty(Some(&winsize), None) {
        Ok(pty) => pty,
        Err(e) => {
            let _ = close(err_read);
            let _ = close(err_write);
            return Err(io::Error::other(e.to_string()));
        }
    };
    let master_fd = pty.master.into_raw_fd();
    let slave_fd = pty.slave.into_raw_fd();

    // Load integration through the shell's startup mechanism rather than
    // writing the script into the interactive input stream. The latter is
    // echoed by zsh and leaves the terminal showing function definitions.
    let startup_cleanup_path = startup_script.and_then(|script| {
        let suffix = STARTUP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("athena-shell-{}-{suffix}", std::process::id()));
        let write_new = |path: &std::path::Path, contents: &str| -> io::Result<()> {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)?;
            file.write_all(contents.as_bytes())?;
            file.flush()
        };
        let zsh_or_bash_script = startup_script_with_user_config(shell, script);
        let result = match shell_name(shell) {
            "zsh" => {
                use std::os::unix::fs::DirBuilderExt;
                std::fs::DirBuilder::new()
                    // Create with 0o700 atomically — a create-chmod pair
                    // leaves a window at umask-derived permissions.
                    .mode(0o700)
                    .create(&base)
                    .and_then(|_| write_new(&base.join(".zshrc"), &zsh_or_bash_script))
                    .map(|_| base.clone())
            }
            "bash" => {
                let path = base.with_extension("bashrc");
                write_new(&path, &zsh_or_bash_script).map(|_| path)
            }
            "fish" => {
                let path = base.with_extension("fish");
                write_new(&path, script).map(|_| path)
            }
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "unsupported shell integration startup",
            )),
        };
        match result {
            Ok(path) => Some(path),
            Err(error) => {
                cleanup_startup_path(Some(&base));
                log::warn!("could not prepare shell integration startup file: {error}");
                None
            }
        }
    });

    let shell_cstr = match CString::new(shell.as_bytes()) {
        Ok(shell_cstr) => shell_cstr,
        Err(error) => {
            cleanup_startup_path(startup_cleanup_path.as_deref());
            let _ = close(master_fd);
            let _ = close(slave_fd);
            let _ = close(err_read);
            let _ = close(err_write);
            return Err(io::Error::new(io::ErrorKind::InvalidInput, error));
        }
    };
    // Pre-format the cwd as a CString BEFORE fork: between fork() and
    // execve the child may only call async-signal-safe functions, and
    // allocating a CString there can deadlock if another thread held the
    // allocator lock when the process forked.
    let cwd_cstr = match CString::new(cwd.as_bytes()) {
        Ok(cwd_cstr) => cwd_cstr,
        Err(error) => {
            cleanup_startup_path(startup_cleanup_path.as_deref());
            let _ = close(master_fd);
            let _ = close(slave_fd);
            let _ = close(err_read);
            let _ = close(err_write);
            return Err(io::Error::new(io::ErrorKind::InvalidInput, error));
        }
    };
    let mut args: Vec<CString> = vec![shell_cstr.clone()];
    match (shell_name(shell), startup_cleanup_path.as_deref()) {
        ("zsh", Some(_)) => args.extend(
            ["-d", "-i"]
                .into_iter()
                .map(|flag| CString::new(flag).expect("static shell flag has no NUL")),
        ),
        ("bash", Some(path)) => {
            args.extend(
                ["--noprofile", "--rcfile"]
                    .into_iter()
                    .map(|flag| CString::new(flag).expect("static shell flag has no NUL")),
            );
            args.push(
                CString::new(path.to_string_lossy().as_bytes()).expect("path has no NUL"),
            );
            args.push(CString::new("-i").expect("static shell flag has no NUL"));
        }
        ("fish", Some(path)) => {
            args.extend(
                ["--no-config", "--init-command"]
                    .into_iter()
                    .map(|flag| CString::new(flag).expect("static shell flag has no NUL")),
            );
            let command = format!("source '{}'", path.to_string_lossy().replace('\'', "\\'"));
            args.push(CString::new(command).expect("path has no NUL"));
            args.push(CString::new("--interactive").expect("static shell flag has no NUL"));
        }
        (_, Some(_)) | (_, None) => {
            args.extend(
                shell_flags(shell)
                    .iter()
                    .map(|flag| CString::new(*flag).expect("static shell flag has no NUL")),
            );
        }
    }
    let startup_env = match (shell_name(shell), startup_cleanup_path.as_deref()) {
        // Point zsh at the generated rc directory; that file explicitly
        // sources the user's real rc files and then Athena's hooks.
        ("zsh", Some(path)) => Some(("ZDOTDIR", path)),
        _ => None,
    };
    let environment = child_environment_with_startup(startup_env);

    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let _ = close(master_fd);
            let _ = close(err_read);

            // Create a new session. If this fails, the child process group
            // may still be the parent's PGID, which would make killpg(PID)
            // kill the parent process group. We must not silently ignore
            // this failure.
            if setsid().is_err() {
                let _ = unsafe { libc::write(err_write, [1u8].as_ptr() as *const _, 1) };
                let _ = close(err_write);
                // `_exit`, not `process::exit`: in a forked child the
                // latter runs libc exit handlers / atexit callbacks
                // (which may allocate or take locks held by another
                // thread at fork time).
                unsafe { libc::_exit(1) };
            }

            // Make this PTY the controlling terminal for the new session.
            // This is critical for interactive shells (readline line editing,
            // job control, SIGINT on Ctrl+C, etc.).
            unsafe {
                let _ = libc::ioctl(slave_fd, libc::TIOCSCTTY as libc::c_ulong, 0);
                let _ = libc::dup2(slave_fd, 0);
                let _ = libc::dup2(slave_fd, 1);
                let _ = libc::dup2(slave_fd, 2);
            }
            let _ = close(slave_fd);
            // `cwd_cstr` was allocated before fork; staying on raw
            // libc::chdir/_exit keeps the child strictly async-signal-safe.
            unsafe { libc::chdir(cwd_cstr.as_ptr()) };
            let _ = nix::unistd::execve(&shell_cstr, &args, &environment);
            // execve only returns on failure.
            let _ = unsafe { libc::write(err_write, [2u8].as_ptr() as *const _, 1) };
            let _ = close(err_write);
            unsafe { libc::_exit(1) };
        }
        Ok(ForkResult::Parent { child }) => {
            // OMP derives its terminal breadcrumb key from ttyname(0).
            // Capture the PTY slave path before closing the parent's copy
            // so the heartbeat can prefer an exact session mapping over
            // a newest-by-cwd approximation.
            let tty_path = unsafe {
                let ptr = libc::ttyname(slave_fd);
                if ptr.is_null() {
                    None
                } else {
                    std::ffi::CStr::from_ptr(ptr)
                        .to_str()
                        .ok()
                        .map(str::to_string)
                }
            };
            let _ = close(slave_fd);
            let _ = close(err_write);

            // Wait for the child to either report an error or succeed.
            // When exec succeeds the pipe is closed (FD_CLOEXEC),
            // returning 0 bytes (EOF). Normally this resolves instantly,
            // but a child stuck before exec would block a runtime worker
            // while holding the sessions write lock — the session layer
            // therefore runs this whole function on the blocking pool.
            let mut err_buf = [0u8; 1];
            let n = unsafe { libc::read(err_read, err_buf.as_mut_ptr() as *mut _, 1) };
            let _ = close(err_read);
            if n > 0 {
                // Child reported an error; reap it so no zombie leaks.
                let _ = waitpid(child, None);
            }
            let err_byte = err_buf[0];

            if n > 0 {
                // Child reported an error before exec.
                let err_msg = match err_byte {
                    1 => "setsid() failed in child process",
                    2 => "execvp() failed in child process",
                    _ => "child process setup failed",
                };
                let _ = close(master_fd);
                cleanup_startup_path(startup_cleanup_path.as_deref());
                return Err(io::Error::other(err_msg));
            }

            // Success: child exec'd and pipe was closed by FD_CLOEXEC.
            unsafe {
                let flags = libc::fcntl(master_fd, libc::F_GETFL, 0);
                libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }

            // Get the PGID explicitly. setsid() makes PGID == PID, but we
            // verify this here to avoid assuming a side-effect that may
            // have silently failed.
            let pgid = match nix::unistd::getpgid(Some(child)) {
                Ok(pgid) if pgid.as_raw() > 0 => pgid,
                _ => child,
            };

            Ok(Spawned {
                handle: ProcessHandle::new(master_fd),
                tty_path,
                pid: child.as_raw(),
                pgid: pgid.as_raw(),
                startup_cleanup_path,
            })
        }
        Err(e) => {
            // No child was forked: close every fd this spawn created so
            // they don't leak.
            let _ = close(master_fd);
            let _ = close(slave_fd);
            let _ = close(err_read);
            let _ = close(err_write);
            cleanup_startup_path(startup_cleanup_path.as_deref());
            Err(io::Error::other(e.to_string()))
        }
    }
}
