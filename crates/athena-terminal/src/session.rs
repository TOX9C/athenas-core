use crate::ansi::handler::{apply_ops_with_responses, AnsiHandler};
use crate::grid::CellDelta;
use crate::grid::Grid;
use log::info;
use nix::pty::{openpty, Winsize};
use nix::sys::wait::{waitpid, WaitPidFlag};
use nix::unistd::{close, fork, setsid, ForkResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::{BTreeMap, VecDeque};
use std::ffi::CString;
use std::io::{self, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::io::{IntoRawFd, RawFd};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{Mutex, RwLock};
use vte::Parser;

/// Reap a child process group after signaling it.
///
/// Sends SIGTERM, gives the group a short grace window to exit, then escalates
/// to SIGKILL if needed. `waitpid(WNOHANG)` is polled throughout so the child
/// does not become a zombie — without this, `Drop`/`kill` would signal the
/// group but never reap it, leaking process-table entries over the app's
/// lifetime.
///
/// `pid` is the child to reap; `pgid` is the group to signal (they differ when
/// `setsid` failed and the child stayed in the parent's group — in that case we
/// must NOT `killpg`, but we still reap our own child).
fn reap_process_group(pid: nix::unistd::Pid, pgid: nix::unistd::Pid) {
    use nix::sys::signal::{killpg, Signal};
    use nix::sys::wait::WaitStatus;

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

/// Status of a PTY session lifecycle.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum PtyStatus {
    Spawning,
    Ready,
    Exited,
}

/// Complete update delta for the frontend.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TerminalUpdate {
    pub session_id: String,
    pub deltas: Vec<CellDelta>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub rows: usize,
    pub cols: usize,
    /// Whether the cursor should be rendered.
    pub cursor_visible: bool,
}

/// A PTY session combining a shell process, PTY file descriptor, and terminal grid.
static STARTUP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(test)]
#[derive(Clone)]
struct ResizeTestProbe {
    acquired: Arc<tokio::sync::Notify>,
    continue_after_acquire: Arc<tokio::sync::Notify>,
}

#[cfg(test)]
impl ResizeTestProbe {
    fn new() -> Self {
        Self {
            acquired: Arc::new(tokio::sync::Notify::new()),
            continue_after_acquire: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

pub struct TerminalSession {
    pub id: String,
    pub grid: Arc<Mutex<Grid>>,
    /// Atomic FD sentinel. Holds the master PTY fd, or `-1` once the fd has
    /// been closed. PTY operations duplicate this fd into an `OwnedFd` while
    /// holding `fd_lifecycle_lock`, so closing the master cannot recycle the
    /// integer used by an in-flight syscall.
    pub master_fd: AtomicI32,
    /// Serializes master-fd duplication with closing the master fd. Each
    /// operation owns its duplicate for the duration of its syscall, so this
    /// lock is not held across blocking I/O.
    fd_lifecycle_lock: std::sync::Mutex<()>,
    pub shell_pid: nix::unistd::Pid,
    /// The process group ID of the shell. Explicitly stored because it is only
    /// equal to `shell_pid` if `setsid()` succeeded. If `setsid()` failed, the
    /// shell remains in the parent's process group and `killpg(shell_pid)` would
    /// kill the parent process group.
    pub pgid: nix::unistd::Pid,
    pub shell: String,
    pub cwd: String,
    /// Slave TTY path used by harnesses such as OMP to associate a terminal
    /// with their durable session breadcrumb. Best-effort; some platforms or
    /// multiplexer layers do not expose a path.
    pub tty_path: Option<String>,
    pub status: Mutex<PtyStatus>,
    /// Whether this session is rendered by xterm.js on the frontend.
    /// When true, the `terminal:data` event (cell deltas) is skipped
    /// because xterm.js parses raw ANSI bytes itself.
    pub is_xterm: AtomicBool,
    /// When true, the PTY read loop keeps reading from the fd (so the shell
    /// process doesn't block on a full pipe) but suppresses `pty:raw` event
    /// emission. Accumulated bytes are flushed as a single burst when the
    /// flag is cleared. This closes the stream-gap desync during xterm.js
    /// remount (pane swap): the old mount pauses the backend before unlisten,
    /// and the new mount unpauses after re-subscribe + snapshot replay.
    pub raw_paused: AtomicBool,
    /// Monotonically increasing xterm listener generation. A teardown may only
    /// pause the generation it detached; this prevents an old async teardown
    /// from pausing a newer remounted listener.
    pub listener_generation: AtomicU64,
    /// Serializes the generation check and raw-pause update. Without this
    /// critical section, a stale detach could observe its generation, then a
    /// newer attach could unpause, and finally the stale detach could pause the
    /// new listener.
    listener_lifecycle_lock: std::sync::Mutex<()>,
    /// Owner token for the currently attached frontend mount. Generations
    /// protect ordering; the owner token also protects against a late attach
    /// from an abandoned mount claiming the replacement mount's generation.
    listener_owner: std::sync::Mutex<Option<String>>,
    /// Marks the current owner as detached. A late attach from that same
    /// owner must be rejected even while output is paused; a new mount owner
    /// may claim the paused session.
    listener_owner_detached: std::sync::Mutex<bool>,
    /// Owner allowed to claim a startup-paused session before the first
    /// listener attaches. This prevents a delayed attach from an abandoned
    /// mount stealing the pause from the replacement mount.
    pending_listener_owner: std::sync::Mutex<Option<String>>,
    /// Safety deadline for a startup pause. If xterm initialization fails
    /// before the listener handshake, the PTY must eventually resume output.
    startup_pause_deadline: std::sync::Mutex<Option<Instant>>,
    /// Owner tokens cancelled before their delayed attach arrived. These
    /// tombstones prevent a stale attach from reclaiming an expired/cancelled
    /// startup pause before the replacement mount can attach.
    rejected_startup_owner: std::sync::Mutex<Option<String>>,
    /// Ensures only one background reader consumes a session's PTY master.
    /// `SessionManager::spawn` intentionally returns an existing session for
    /// duplicate IDs, so callers must claim the read loop separately.
    reader_started: AtomicBool,
    /// Optional private startup file/directory used to load shell integration
    /// before the interactive prompt. Keeping this out of the PTY input stream
    /// prevents zsh from echoing the generated hook definitions to the user.
    startup_cleanup_path: Option<std::path::PathBuf>,
    pub pending_writes: Mutex<VecDeque<Vec<u8>>>,
    /// Serialize writes to the PTY master.
    ///
    /// xterm.js emits one `onData` event per key (and interactive TUIs often
    /// switch the PTY into raw mode). The frontend forwards those events over
    /// asynchronous IPC, so multiple writes can otherwise reach the PTY at
    /// the same time and complete out of order. Keeping one writer in flight
    /// preserves the byte order the user generated.
    write_lock: Mutex<()>,
    /// Serializes PTY window-size updates. Without this lock, an older
    /// `TIOCSWINSZ` task can finish after a newer request and leave the shell
    /// at dimensions that no longer match xterm.js.
    resize_lock: Mutex<()>,
    #[cfg(test)]
    resize_test_probe: std::sync::Mutex<Option<ResizeTestProbe>>,
    /// Persistent VTE parser state.
    ///
    /// VTE's `Parser` is a state machine that tracks partial escape sequences
    /// (e.g. a CSI sequence split across two `read()` calls: `ESC[` arrives in
    /// one call, the parameter bytes `38;2;10;10;10m` arrive in the next).
    ///
    /// This field MUST persist for the lifetime of the session. Recreating the
    /// parser on every read — as the previous free-function `read_and_parse`
    /// did — destroys in-flight state and causes parameter bytes to fall
    /// through to the `Perform::print` handler, which then renders raw CSI
    /// digits and semicolons as visible cell text (the "leaking ANSI codes"
    /// visual glitch).
    parser: Mutex<Parser>,
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        self.close_fd_with_reason("session_drop");
        // Signal AND reap. Without `waitpid` the signaled shell becomes a
        // zombie until process exit; over a long app session that leaks the
        // process table. The grace-then-SIGKILL escalation also handles
        // defiant shells that ignore SIGTERM.
        reap_process_group(self.shell_pid, self.pgid);
        cleanup_startup_path(self.startup_cleanup_path.as_deref());
    }
}

impl TerminalSession {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        master_fd: RawFd,
        shell_pid: nix::unistd::Pid,
        pgid: nix::unistd::Pid,
        shell: String,
        cwd: String,
        cols: usize,
        rows: usize,
    ) -> Self {
        Self::new_with_startup(
            id, master_fd, shell_pid, pgid, shell, cwd, None, cols, rows, None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_with_startup(
        id: String,
        master_fd: RawFd,
        shell_pid: nix::unistd::Pid,
        pgid: nix::unistd::Pid,
        shell: String,
        cwd: String,
        tty_path: Option<String>,
        cols: usize,
        rows: usize,
        startup_cleanup_path: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            id,
            grid: Arc::new(Mutex::new(Grid::new(cols, rows))),
            master_fd: AtomicI32::new(master_fd),
            fd_lifecycle_lock: std::sync::Mutex::new(()),
            shell_pid,
            pgid,
            shell,
            cwd,
            tty_path,
            status: Mutex::new(PtyStatus::Spawning),
            is_xterm: AtomicBool::new(false),
            raw_paused: AtomicBool::new(false),
            listener_generation: AtomicU64::new(0),
            listener_lifecycle_lock: std::sync::Mutex::new(()),
            listener_owner: std::sync::Mutex::new(None),
            listener_owner_detached: std::sync::Mutex::new(false),
            pending_listener_owner: std::sync::Mutex::new(None),
            rejected_startup_owner: std::sync::Mutex::new(None),
            reader_started: AtomicBool::new(false),
            startup_cleanup_path,
            startup_pause_deadline: std::sync::Mutex::new(None),
            pending_writes: Mutex::new(VecDeque::new()),
            write_lock: Mutex::new(()),
            resize_lock: Mutex::new(()),
            #[cfg(test)]
            resize_test_probe: std::sync::Mutex::new(None),
            parser: Mutex::new(Parser::new()),
        }
    }

    /// Duplicate the live master fd while synchronizing with close. The
    /// returned descriptor is independently owned and remains valid even if
    /// `close_fd` recycles the original master-fd number before the syscall
    /// using this descriptor begins.
    fn duplicate_master_fd(&self) -> io::Result<OwnedFd> {
        let _fd_guard = self
            .fd_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let fd = self.master_fd.load(Ordering::Acquire);
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

    #[cfg(test)]
    fn install_resize_test_probe(&self, probe: ResizeTestProbe) {
        *self
            .resize_test_probe
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(probe);
    }

    /// Atomically close the master fd. Idempotent: subsequent calls are no-ops
    /// because the sentinel `-1` is swapped back to itself. The first caller
    /// to swap wins and is responsible for the `libc::close` call. In-flight
    /// operations use independent owned duplicates and therefore remain safe
    /// if the kernel reuses the master-fd integer after this returns.
    ///
    /// Returns `true` if this call actually closed an open fd, `false` if
    /// the fd was already closed.
    #[cfg(test)]
    fn close_fd(&self) -> bool {
        self.close_fd_with_reason("test")
    }

    fn close_fd_with_reason(&self, reason: &'static str) -> bool {
        let _fd_guard = self
            .fd_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let old = self.master_fd.swap(-1, Ordering::AcqRel);
        if old >= 0 {
            // SAFETY: `old` was a valid fd owned by this session; we are
            // the first thread to swap it out, so no one else will close
            // it. The kernel may reuse the integer for an unrelated fd
            // after this returns — callers must reload the atomic to
            // observe the new `-1` sentinel.
            unsafe { libc::close(old as RawFd) };
            log::debug!(
                "PTY master fd closed: session={} reason={}",
                self.id,
                reason
            );
            true
        } else {
            false
        }
    }

    /// Write data to the PTY master fd.
    /// If the session is not yet ready, data is queued for later.
    pub async fn write(&self, data: &[u8]) -> io::Result<usize> {
        // Hold the writer lock through the pending-write check and the actual
        // PTY write. This makes the Spawning → Ready transition and all live
        // key writes one ordered stream rather than concurrent blocking tasks.
        let _write_guard = self.write_lock.lock().await;
        {
            // Keep lock order consistent with `mark_ready` (status, then
            // pending) so a readiness transition cannot deadlock with input.
            let status = self.status.lock().await;
            if *status == PtyStatus::Spawning {
                drop(status);
                self.pending_writes.lock().await.push_back(data.to_vec());
                return Ok(data.len());
            }
        }
        self.do_write(data).await
    }

    /// Internal: perform the actual write to the PTY master fd.
    async fn do_write(&self, data: &[u8]) -> io::Result<usize> {
        let owned_fd = self.duplicate_master_fd()?;
        // Keystroke fast path: the master is O_NONBLOCK and writable for
        // virtually every interactive write, so attempt the write inline
        // instead of paying a `spawn_blocking` dispatch plus a buffer copy
        // per keystroke. The fd is this call's own dup'd lease, so close-race
        // safety is identical to the blocking path.
        let fd = owned_fd.as_raw_fd();
        let first = unsafe { libc::write(fd, data.as_ptr() as *const _, data.len()) };
        if first >= 0 {
            let written = first as usize;
            if written == data.len() {
                return Ok(written);
            }
            // Rare partial write of a large paste: finish on the blocking
            // pool where polling POLLOUT is allowed.
            return self.do_write_blocked(owned_fd, data, written).await;
        }
        let err = io::Error::last_os_error();
        match err.raw_os_error() {
            // Interrupted or pipe full: continue on the blocking path, which
            // polls for POLLOUT and retries.
            // EWOULDBLOCK == EAGAIN on macOS/Linux, so a single pattern
            // covers both.
            Some(libc::EINTR) | Some(libc::EAGAIN) => {
                self.do_write_blocked(owned_fd, data, 0).await
            }
            _ => Err(err),
        }
    }

    /// Slow path for `do_write`: write `data[done..]` from the blocking pool,
    /// polling POLLOUT when the PTY buffer is full. `owned_fd` is the
    /// caller's fd lease, moved in so close-race safety is unchanged.
    async fn do_write_blocked(
        &self,
        owned_fd: OwnedFd,
        data: &[u8],
        done: usize,
    ) -> io::Result<usize> {
        let buf = data[done..].to_vec();
        tokio::task::spawn_blocking(move || {
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
                    // EIO on a fresh PTY usually means the child hasn't finished exec yet;
                    // treat as WouldBlock so callers can retry.
                    if code == Some(5) {
                        return Err(io::Error::new(io::ErrorKind::WouldBlock, err));
                    }
                    return Err(err);
                }
                total_written += written as usize;
            }
            Ok(done + total_written)
        })
        .await
        .map_err(io::Error::other)?
    }

    /// Begin a bounded startup pause before the frontend's raw listener exists.
    ///
    /// The owner is optional for legacy callers that do not use xterm startup
    /// handshakes. When present, only that mount may claim the paused session.
    pub fn begin_startup_pause(&self, owner: Option<String>) {
        let _lifecycle_guard = self
            .listener_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = owner;
        *self
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(Instant::now() + Duration::from_secs(5));
        *self
            .rejected_startup_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.raw_paused.store(true, Ordering::Release);
    }

    /// Release an abandoned startup pause. Returns true when this call
    /// actually resumed output so the read loop can flush its coalesced burst.
    pub fn expire_startup_pause(&self) -> bool {
        let _lifecycle_guard = self
            .listener_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let expired = self
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some_and(|deadline| Instant::now() >= deadline);
        if !expired || !self.raw_paused.load(Ordering::Acquire) {
            return false;
        }
        let pending_owner = self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone();
        *self
            .rejected_startup_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = pending_owner;
        *self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        self.raw_paused.store(false, Ordering::Release);
        true
    }

    /// Register a new frontend raw-output listener generation and resume output.
    ///
    /// The generation is allocated before clearing `raw_paused`, so an older
    /// teardown racing this attach can only observe a stale generation and is
    /// ignored by `detach_listener`.
    pub fn attach_listener(&self, owner: String, replace_current: bool) -> Option<u64> {
        let _lifecycle_guard = self
            .listener_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut current_owner = self
            .listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut owner_detached = self
            .listener_owner_detached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut pending_owner = self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut pause_deadline = self
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut paused = self.raw_paused.load(Ordering::Acquire);
        if replace_current && (current_owner.is_some() || pending_owner.is_some()) {
            // A remount explicitly supersedes any older attach still in
            // flight. The lifecycle lock makes replacement and attachment
            // one atomic handoff.
            *current_owner = None;
            *owner_detached = false;
            *pending_owner = Some(owner.clone());
            *self
                .rejected_startup_owner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        }
        if paused && pause_deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            *pending_owner = None;
            *pause_deadline = None;
            self.raw_paused.store(false, Ordering::Release);
            paused = false;
        }
        if current_owner.is_none()
            && pending_owner
                .as_deref()
                .is_some_and(|pending| pending != owner.as_str())
        {
            // A delayed attach from an abandoned startup mount cannot claim
            // the replacement mount's pending pause.
            return None;
        }
        if current_owner.is_none()
            && pending_owner.is_none()
            && self
                .rejected_startup_owner
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .as_deref()
                == Some(owner.as_str())
        {
            // This owner was explicitly cancelled or expired before its
            // attach arrived; do not let the stale task reclaim the session.
            return None;
        }
        if current_owner.as_deref() == Some(owner.as_str()) && *owner_detached && paused {
            // This owner already detached. Its attach response arrived late;
            // do not let it reclaim a paused session needed by a remount.
            return None;
        }
        if current_owner
            .as_deref()
            .is_some_and(|current| current != owner)
            && !paused
        {
            // A different owner is already live. This is a late attach from an
            // abandoned mount; rejecting it keeps it from stealing ownership
            // from the replacement mount.
            return None;
        }
        if current_owner.as_deref() == Some(owner.as_str()) && !paused {
            // Same-owner reattach (duplicate attach call): keep the existing
            // generation; a remount of the same owner is idempotent.
            return Some(self.listener_generation.load(Ordering::Acquire));
        }
        let generation = self.listener_generation.fetch_add(1, Ordering::AcqRel) + 1;
        *current_owner = Some(owner);
        *owner_detached = false;
        *pending_owner = None;
        *pause_deadline = None;
        *self
            .rejected_startup_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        // Unpause only after the ownership handshake is complete: the first
        // bytes the read loop releases must already belong to the new owner.
        self.raw_paused.store(false, Ordering::Release);
        Some(generation)
    }

    /// Cancel a startup listener lease before its first attach arrives.
    /// Generation zero is reserved for this pre-attach cleanup path.
    pub fn cancel_startup_pause(&self, owner: &str) -> bool {
        let _lifecycle_guard = self
            .listener_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pending_matches = self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_deref()
            == Some(owner);
        let current_matches = self
            .listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_deref()
            == Some(owner);
        if !pending_matches && !current_matches {
            return false;
        }
        if current_matches {
            *self
                .listener_owner_detached
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
            self.raw_paused.store(true, Ordering::Release);
        } else {
            self.raw_paused.store(false, Ordering::Release);
        }
        *self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .rejected_startup_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(owner.to_string());
        true
    }

    /// Pause output only when both the generation and mount owner are still
    /// current. Returns false for stale or abandoned teardown work.
    pub fn detach_listener(&self, owner: &str, generation: u64) -> bool {
        let _lifecycle_guard = self
            .listener_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if self.listener_generation.load(Ordering::Acquire) != generation {
            return false;
        }
        let current_owner = self
            .listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if current_owner.as_deref() != Some(owner) {
            return false;
        }
        *self
            .listener_owner_detached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        *self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        *self
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        drop(current_owner);
        // Pause so in-flight flushes cannot emit to a detached listener; the
        // read loop only flushes while `raw_paused` is false.
        self.raw_paused.store(true, Ordering::Release);
        true
    }

    /// Return whether a resize request belongs to a live or pending frontend
    /// mount. Unowned callers remain supported for relay/legacy paths.
    pub fn can_resize_for_owner(&self, owner: Option<&str>) -> bool {
        let Some(owner) = owner else {
            return true;
        };
        let _lifecycle_guard = self
            .listener_lifecycle_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let current_owner = self
            .listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let pending_owner = self
            .pending_listener_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let owner_detached = *self
            .listener_owner_detached
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        pending_owner.as_deref() == Some(owner)
            || (current_owner.as_deref() == Some(owner) && !owner_detached)
            // A remount has detached the current owner and keeps output
            // paused while the replacement measures its container. Permit
            // that replacement's initial resize before it calls attach;
            // attach_listener still performs the authoritative ownership
            // handoff under the lifecycle lock.
            || (self.raw_paused.load(Ordering::Acquire)
                && owner_detached
                && current_owner.as_deref() != Some(owner))
    }

    /// Claim ownership of the session's background PTY reader.
    ///
    /// Multiple callers can receive the same `Arc<TerminalSession>` when a
    /// pane is remounted or a duplicate spawn request arrives. Only the first
    /// caller may start `pty_read_loop`; two readers on one PTY would split or
    /// duplicate the shell's echoed bytes.
    pub fn try_claim_read_loop(&self) -> bool {
        !self.reader_started.swap(true, Ordering::AcqRel)
    }

    /// Release a read-loop claim when startup could not actually launch a
    /// reader (for example, before the Tauri app handle is available).
    pub fn release_read_loop(&self) {
        self.reader_started.store(false, Ordering::Release);
    }

    /// Mark the session as ready and flush any pending writes.
    pub async fn mark_ready(&self) {
        // Match `write`: pending startup bytes must be flushed before any
        // later interactive input can overtake them.
        let _write_guard = self.write_lock.lock().await;
        let mut status = self.status.lock().await;
        *status = PtyStatus::Ready;
        drop(status);

        let mut pending = self.pending_writes.lock().await;
        while let Some(data) = pending.pop_front() {
            let _ = self.do_write(&data).await;
        }
    }

    /// Mark the session terminal after its PTY reader observes process exit.
    /// The session remains addressable long enough for the frontend to receive
    /// the exit event, while a later spawn can atomically replace it.
    pub async fn mark_exited(&self) {
        let mut status = self.status.lock().await;
        *status = PtyStatus::Exited;
    }
}

/// Manages all active PTY sessions.
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<TerminalSession>>>>,
}

/// Return flags that keep the app terminal independent from broken or
/// machine-specific interactive shell startup files. Athena owns the PTY
/// environment, so `.zshrc`/`.bashrc` failures must not prevent a usable shell.
fn shell_name(shell: &str) -> &str {
    std::path::Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

fn cleanup_startup_path(path: Option<&std::path::Path>) {
    if let Some(path) = path {
        let _ = std::fs::remove_dir_all(path);
        let _ = std::fs::remove_file(path);
    }
}

/// Quote a path for a shell `source` command without allowing path characters
/// to become shell syntax. Startup files are generated by Athena, but the
/// user's home directory may contain spaces or apostrophes.
fn shell_single_quote(path: &std::path::Path) -> String {
    format!("'{}'", path.to_string_lossy().replace('\'', "'\\''"))
}

/// Preserve the user's normal interactive shell configuration while keeping
/// Athena's integration script in a private startup file. The private file is
/// the only file passed to the child shell, so Athena can load hooks silently;
/// sourcing the real rc file first restores Oh My Zsh, prompts, aliases, and
/// user PATH setup.
fn startup_script_with_user_config(shell: &str, integration_script: &str) -> String {
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

fn shell_flags(shell: &str) -> &'static [&'static str] {
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
fn child_environment_with_startup(startup_env: Option<(&str, &std::path::Path)>) -> Vec<CString> {
    let mut values = BTreeMap::new();
    for (key, value) in std::env::vars() {
        values.insert(key, value);
    }
    values.insert("TERM".to_string(), "xterm-256color".to_string());
    values.insert("COLORTERM".to_string(), "truecolor".to_string());
    values.insert("TERM_PROGRAM".to_string(), "Athena".to_string());
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

impl SessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn spawn(
        &self,
        id: String,
        shell: &str,
        cwd: &str,
        cols: u16,
        rows: u16,
    ) -> io::Result<Arc<TerminalSession>> {
        self.spawn_with_startup_script(id, shell, cwd, cols, rows, None)
            .await
    }

    pub async fn spawn_with_startup_script(
        &self,
        id: String,
        shell: &str,
        cwd: &str,
        cols: u16,
        rows: u16,
        startup_script: Option<&str>,
    ) -> io::Result<Arc<TerminalSession>> {
        info!(
            "Spawning PTY session: id={}, shell={}, cwd={}",
            id, shell, cwd
        );

        // P1-2 / Task 4.3 atomic check-insert fix.
        //
        // Hold the `write()` lock for the ENTIRE critical section so that the
        // "does this id exist?" check and the "insert new session" write are
        // atomic with respect to every other `spawn` call. Without this, two
        // concurrent callers using the same `id` could both pass the read-lock
        // check, both fork(), and the second `insert` would overwrite the
        // first — closing its master_fd, sending SIGTERM to a possibly
        // unrelated process group, and orphaning the child as a zombie.
        //
        // This serializes concurrent `spawn` calls (even on different ids) for
        // the duration of a fork+exec+error-pipe-wait. That is acceptable:
        // spawns are rare user actions, the work is short, and the alternative
        // (per-id locks or an in-flight spawns set) adds complexity for a
        // non-hot path.
        //
        // The `tokio::sync::RwLock` write guard is not held across an `.await`
        // that yields; the only `.await` in this function is the initial
        // `write()` acquisition, after which we hold the guard until the
        // function returns. The blocking `libc::read` on the error pipe is
        // short-lived (returns 0 on success via FD_CLOEXEC EOF, or 1 byte on
        // child pre-exec error), so it does not stall the runtime worker.
        let mut sessions = self.sessions.write().await;
        if let Some(existing) = sessions.get(&id).cloned() {
            let is_exited = {
                let status = existing.status.lock().await;
                *status == PtyStatus::Exited
            };
            if is_exited {
                // A natural PTY exit leaves the old Arc alive until its reader
                // task finishes. Remove only the map entry here; the old task
                // retains ownership of its fd/process cleanup, and this new
                // spawn gets a fresh reader claim and PTY.
                sessions.remove(&id);
            } else {
                info!("PTY session {} already exists, returning existing", id);
                return Ok(existing);
            }
        }

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

        let pty = openpty(Some(&winsize), None).map_err(|e| io::Error::other(e.to_string()))?;
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
                "zsh" => std::fs::create_dir(&base)
                    .and_then(|_| {
                        #[cfg(unix)]
                        {
                            use std::os::unix::fs::PermissionsExt;
                            std::fs::set_permissions(
                                &base,
                                std::fs::Permissions::from_mode(0o700),
                            )?;
                        }
                        write_new(&base.join(".zshrc"), &zsh_or_bash_script)
                    })
                    .map(|_| base.clone()),
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
                    std::process::exit(1);
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
                let _ = nix::unistd::chdir(std::path::Path::new(cwd));
                let _ = nix::unistd::execve(&shell_cstr, &args, &environment);
                // execve only returns on failure.
                let _ = unsafe { libc::write(err_write, [2u8].as_ptr() as *const _, 1) };
                let _ = close(err_write);
                std::process::exit(1);
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
                // When execvp succeeds the pipe is closed (FD_CLOEXEC),
                // returning 0 bytes (EOF).
                let mut err_buf = [0u8; 1];
                let n = unsafe { libc::read(err_read, err_buf.as_mut_ptr() as *mut _, 1) };
                let _ = close(err_read);

                if n > 0 {
                    // Child reported an error before exec.
                    let err_msg = match err_buf[0] {
                        1 => "setsid() failed in child process",
                        2 => "execvp() failed in child process",
                        _ => "child process setup failed",
                    };
                    let _ = nix::sys::wait::waitpid(child, None);
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

                let session = Arc::new(TerminalSession::new_with_startup(
                    id.clone(),
                    master_fd,
                    child,
                    pgid,
                    shell.to_string(),
                    cwd.to_string(),
                    tty_path,
                    cols as usize,
                    rows as usize,
                    startup_cleanup_path.clone(),
                ));

                // Defensive re-check: with the write lock held continuously
                // from the top of the function, this branch is unreachable
                // (no other `spawn` task could have inserted the same id).
                // We keep it as a safety net in case the lock scope is ever
                // refactored (e.g. to swap in a per-id lock or in-flight set)
                // and a future contributor re-introduces a release point
                // between the initial check and this insert.
                if let Some(existing) = sessions.get(&id).cloned() {
                    session.close_fd_with_reason("duplicate_session");
                    let _ =
                        nix::sys::signal::killpg(session.pgid, nix::sys::signal::Signal::SIGTERM);
                    let _ =
                        nix::sys::wait::waitpid(child, Some(nix::sys::wait::WaitPidFlag::WNOHANG));
                    return Ok(existing);
                }

                sessions.insert(id.clone(), session.clone());
                Ok(session)
            }
            Err(e) => {
                cleanup_startup_path(startup_cleanup_path.as_deref());
                Err(io::Error::other(e.to_string()))
            }
        }
    }

    pub async fn get_session(&self, id: &str) -> Option<Arc<TerminalSession>> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn kill(&self, id: &str) -> io::Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(id) {
            session.close_fd_with_reason("explicit_kill");
            // Reap on kill too (mirrors Drop) so explicit kills don't leak
            // zombies either.
            reap_process_group(session.shell_pid, session.pgid);
        }
        Ok(())
    }

    /// Gracefully shut down every live PTY session.
    ///
    /// Used on app exit (`RunEvent::ExitRequested`) so that foreground
    /// processes (e.g. `claude`, `codex`) started inside shells are
    /// interrupted and reaped rather than orphaned when the app process
    /// exits. Without this, the child shells + their `claude`/`codex`
    /// children are reparented to launchd and the OS just closes their fds.
    ///
    /// Two-phase, "Ctrl+C then kill":
    ///   1. Write Ctrl+C (`\x03`) to each session's master fd. Because the
    ///      PTY is the child's controlling terminal (set up via `TIOCSCTTY`
    ///      at spawn), the kernel line discipline translates the INTR byte
    ///      into SIGINT to the session's *foreground* process group — which
    ///      correctly targets `claude` even when the shell has placed it in
    ///      a separate job-control process group. This gives the agent a
    ///      chance to run its exit handler, flush its session file, and
    ///      print its resume line (already captured by the frontend scanner
    ///      from the live `pty:raw` stream, so it is persisted before the
    ///      kill).
    ///   2. After a short grace window, force-close + reap every session via
    ///      `reap_process_group` (SIGTERM → grace → SIGKILL → waitpid), so
    ///      no survivors are orphaned and no zombies linger.
    ///
    /// The sessions map is drained first so each `Arc<TerminalSession>` is
    /// reaped exactly here; a later `Drop` (when the read loop releases its
    /// own clone) finds an already-reaped child and no-ops on `ECHILD`.
    pub async fn shutdown_all(&self) {
        // Drain the map so Drop's reaping can't race our explicit reaping.
        // The read loop still holds its own Arc clone, so the session isn't
        // dropped yet — it lives until close_fd() makes the loop observe EOF.
        let sessions: Vec<Arc<TerminalSession>> = {
            let mut sessions = self.sessions.write().await;
            sessions.drain().map(|(_, v)| v).collect::<Vec<_>>()
        };
        if sessions.is_empty() {
            return;
        }
        info!(
            "shutdown_all: gracefully interrupting {} PTY session(s)",
            sessions.len()
        );

        // Phase 1 — graceful, in-band exit. Write `/exit` + Enter so the
        // foreground agent (claude/codex/…) runs its OWN exit handler, flushes
        // its session file, and prints its resume line — which the read loop
        // then emits as `pty:raw` for the frontend scanner AND appends to the
        // backend OutputBuffer (used by the app-exit capture path). This is
        // strictly gentler than SIGINT and gives the resume id the best chance
        // to appear before we tear things down. Best-effort: ignore write
        // errors (the fd may already be closed, or the child may have exited).
        for session in &sessions {
            let _ = session.write(b"/exit\r").await;
        }

        // Let the agents process `/exit`, flush state, and print their resume
        // line. ~700 ms covers claude/codex exit handlers; the read loop's
        // 8 ms flush interval guarantees the bytes reach listeners in time.
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;

        // Phase 2 — SIGINT fallback. Any agent that ignored `/exit` (or whose
        // shell has already returned to a prompt with no agent running) gets a
        // Ctrl+C. Because the PTY is the controlling terminal, the kernel line
        // discipline turns `\x03` into SIGINT for the foreground process group.
        for session in &sessions {
            let _ = session.write(b"\x03").await;
        }

        // Brief grace for the SIGINT to take effect before we escalate to
        // SIGKILL. Kept short so app quit doesn't feel sluggish.
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        // Phase 3 — force reap. close_fd first so the read loop observes EOF
        // and exits cleanly, then signal+reap the process group.
        for session in sessions {
            session.close_fd_with_reason("shutdown_all");
            reap_process_group(session.shell_pid, session.pgid);
        }
        info!("shutdown_all: all PTY sessions reaped");
    }

    pub async fn write(&self, id: &str, data: &[u8]) -> io::Result<usize> {
        if let Some(session) = self.get_session(id).await {
            session.write(data).await
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Session not found"))
        }
    }

    pub async fn mark_session_ready(&self, id: &str) {
        if let Some(session) = self.get_session(id).await {
            session.mark_ready().await;
        }
    }

    pub async fn resize(
        &self,
        id: &str,
        cols: u16,
        rows: u16,
        owner: Option<&str>,
    ) -> io::Result<()> {
        if let Some(session) = self.get_session(id).await {
            // Serialize the complete ioctl + grid update so the dimensions
            // observed by the backend parser cannot move backward while a
            // newer resize is being applied.
            let _resize_guard = session.resize_lock.lock().await;

            #[cfg(test)]
            let resize_test_probe = {
                let mut probe = session
                    .resize_test_probe
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                probe.take()
            };

            #[cfg(test)]
            if let Some(probe) = resize_test_probe {
                probe.acquired.notify_one();
                probe.continue_after_acquire.notified().await;
            }

            if !session.can_resize_for_owner(owner) {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "stale PTY resize owner",
                ));
            }
            let ws = libc::winsize {
                ws_row: rows,
                ws_col: cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            let owned_fd = session.duplicate_master_fd()?;
            let result = tokio::task::spawn_blocking(move || unsafe {
                libc::ioctl(owned_fd.as_raw_fd(), libc::TIOCSWINSZ, &ws)
            })
            .await;
            match result {
                Ok(0) => {
                    let mut grid = session.grid.lock().await;
                    grid.resize(cols as usize, rows as usize);
                    Ok(())
                }
                Ok(_) => Err(io::Error::last_os_error()),
                Err(e) => Err(io::Error::other(e)),
            }
        } else {
            Err(io::Error::new(io::ErrorKind::NotFound, "Session not found"))
        }
    }

    pub async fn has_session(&self, id: &str) -> bool {
        let sessions = self.sessions.read().await;
        sessions.contains_key(id)
    }

    /// Synchronous, non-blocking session existence check.
    ///
    /// Uses `try_read` so it never blocks (and therefore never panics with
    /// "Cannot start a runtime from within a runtime" when called from a
    /// tokio worker thread, unlike an async variant driven via `block_on`).
    /// Returns `false` if the read lock is contended — callers should treat
    /// that as "session not confirmed" and fall back to the async path or the
    /// active-sessions cache. This is correct for the only caller
    /// (`TauriEventSender::has_session`), which already has a fast-path
    /// cache and uses this as a best-effort secondary check.
    pub fn has_session_sync(&self, id: &str) -> bool {
        match self.sessions.try_read() {
            Ok(sessions) => sessions.contains_key(id),
            Err(_) => false,
        }
    }

    pub async fn list_sessions(&self) -> Vec<String> {
        let sessions = self.sessions.read().await;
        sessions.keys().cloned().collect()
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Read and parse bytes from the PTY, applying updates to the grid.
/// Returns TerminalUpdate if there were cell changes.
impl TerminalSession {
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
    #[allow(dead_code)] // kept for the F10 poll-semantics tests; the read loop uses spawn_reader
    pub async fn read_bytes(&self, buf: &mut [u8], wait_ms: u64) -> io::Result<usize> {
        let owned_fd = self.duplicate_master_fd()?;
        // The read itself is O_NONBLOCK: it completes immediately (data,
        // EAGAIN, or error), so it never parks the runtime worker and —
        // critically for select! cancellation — it finishes before any
        // await point. Only the poll parks; the poll closure touches no
        // caller memory (it receives the fd by value as a lease), so a
        // dropped future leaves at worst an orphan that polls and closes an
        // owned dup — harmless.
        let nbytes =
            unsafe { libc::read(owned_fd.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
        if nbytes >= 0 {
            return Ok(nbytes as usize);
        }
        let e = io::Error::last_os_error();
        if e.kind() != io::ErrorKind::WouldBlock && e.raw_os_error() != Some(libc::EAGAIN) {
            return Err(e);
        }
        // EAGAIN: park in poll(2) on a blocking thread until POLLIN
        // readiness or the window expires. Runs off the worker so a
        // multi-pane idle fleet cannot starve the runtime (Tauri commands
        // share it); same pattern as `do_write`. The closure returns a
        // readiness verdict only — the consuming read happens below, inline,
        // where the `buf` borrow is live and cancellation-safe.
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        enum Wait {
            Ready,
            TimedOut,
        }
        let ready = tokio::task::spawn_blocking(move || {
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
                return if perr.raw_os_error() == Some(libc::EINTR) {
                    Ok(Wait::TimedOut)
                } else {
                    Err(perr)
                };
            }
            if pr > 0 && pfd.revents & libc::POLLIN != 0 {
                Ok(Wait::Ready)
            } else {
                Ok(Wait::TimedOut)
            }
        })
        .await
        .map_err(io::Error::other)??;
        if ready != Wait::Ready {
            return Ok(0);
        }
        // Readiness confirmed: consume inline. The poll closure consumed the
        // first lease, so take a fresh one — if the session closed while we
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

    /// Spawn a dedicated blocking reader thread for this session and return
    /// a channel receiver of `Ok(chunk)` / terminal `Err`.
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
        self: &std::sync::Arc<Self>,
    ) -> tokio::sync::mpsc::Receiver<io::Result<Vec<u8>>> {
        let (tx, rx) = tokio::sync::mpsc::channel::<io::Result<Vec<u8>>>(8);
        let session = std::sync::Arc::clone(self);
        let thread_name = format!("pty-reader-{}", session.id);
        // Kept in case the thread cannot even be spawned — the error is
        // delivered through the channel so the caller takes its normal
        // error path.
        let fallback_tx = tx.clone();
        let spawned = std::thread::Builder::new()
            .name(thread_name)
            .spawn(move || {
                let owned_fd = match session.duplicate_master_fd() {
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
                        if tx
                            .blocking_send(Ok(buf[..nbytes as usize].to_vec()))
                            .is_err()
                        {
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
            Ok(_handle) => rx,
            Err(e) => {
                // Thread spawn failure: deliver the error through the channel
                // so the caller's loop takes its normal error path.
                let _ = fallback_tx.blocking_send(Err(io::Error::other(e)));
                rx
            }
        }
    }

    /// Feed raw bytes through the persistent VTE parser and apply the
    /// resulting ops to the grid. Returns a `TerminalUpdate` with cell
    /// deltas only if the grid state actually changed.
    pub async fn parse_bytes(&self, data: &[u8]) -> io::Result<Option<TerminalUpdate>> {
        Ok(self.parse_bytes_with_responses(data).await?.0)
    }

    /// Parse PTY bytes and return any terminal-protocol responses (currently
    /// DSR cursor-position replies) alongside the optional grid update.
    pub async fn parse_bytes_with_responses(
        &self,
        data: &[u8],
    ) -> io::Result<(Option<TerminalUpdate>, Vec<Vec<u8>>)> {
        if data.is_empty() {
            return Ok((None, Vec::new()));
        }

        // Feed bytes through the persistent parser; the handler is a local
        // sink for completed ops — VTE's state lives in the parser.
        let mut parser_guard = self.parser.lock().await;
        let mut handler = AnsiHandler::new();
        parser_guard.advance(&mut handler, data);
        let ops = handler.ops();
        drop(parser_guard);

        let mut grid_guard = self.grid.lock().await;
        let responses = apply_ops_with_responses(&mut grid_guard, ops);
        let deltas = grid_guard.dirty_deltas();
        let cursor_row = grid_guard.cursor.row;
        let cursor_col = grid_guard.cursor.col;
        let rows = grid_guard.rows_count;
        let cols = grid_guard.cols;

        let update = if !deltas.is_empty() {
            let update = TerminalUpdate {
                session_id: String::new(),
                deltas,
                cursor_row,
                cursor_col,
                rows,
                cols,
                cursor_visible: grid_guard.cursor_visible,
            };
            grid_guard.clear_dirty();
            Some(update)
        } else {
            None
        };
        drop(grid_guard);
        Ok((update, responses))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serializes tests that recycle raw fd numbers (dup2) — concurrent
    /// tests opening/closing descriptors would race on the recycled number.
    static FD_RECYCLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn spawn_reader_delivers_echoed_bytes() {
        let manager = SessionManager::new();
        let session = manager
            .spawn("reader-test".to_string(), "/bin/cat", "/", 80, 24)
            .await
            .expect("spawn /bin/cat");
        let session = std::sync::Arc::new(session);
        let mut rx = session.spawn_reader();
        // Writes queue until the child has exec'd; /bin/cat can't echo what
        // never reaches the fd.
        session.mark_ready().await;
        session
            .write(b"reader-probe\n")
            .await
            .expect("write probe bytes");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut collected = Vec::new();
        loop {
            match tokio::time::timeout_at(deadline.into(), rx.recv()).await {
                Ok(Some(Ok(chunk))) => {
                    collected.extend_from_slice(&chunk);
                    if collected
                        .windows(12)
                        .any(|w| w == b"reader-probe".as_slice())
                    {
                        break;
                    }
                }
                Ok(Some(Err(e))) => panic!("reader error: {e}"),
                Ok(None) => panic!("reader closed early, collected {} bytes", collected.len()),
                Err(_) => panic!("timeout: collected {} bytes", collected.len()),
            }
        }
        assert!(String::from_utf8_lossy(&collected).contains("reader-probe"));
    }

    #[test]
    fn zsh_startup_bootstrap_rejects_stale_omz_path() {
        let script = startup_script_with_user_config("/bin/zsh", "echo athena");
        assert!(script.contains("${ZSH}/oh-my-zsh.sh"));
        assert!(script.contains("$HOME/.oh-my-zsh"));
        assert!(script.contains("echo athena"));
    }

    #[test]
    fn shell_flags_disable_user_startup_files() {
        assert_eq!(shell_flags("/bin/zsh"), &["-f", "-i"]);
        assert_eq!(shell_flags("/bin/bash"), &["--noprofile", "--norc", "-i"]);
        assert_eq!(
            shell_flags("/usr/bin/fish"),
            &["--no-config", "--interactive"]
        );
    }

    #[test]
    fn shell_integration_token_is_one_shot_and_retryable() {
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let session = TerminalSession::new(
            "integration-token".to_string(),
            read_end.into_raw_fd(),
            nix::unistd::Pid::from_raw(1),
            nix::unistd::Pid::from_raw(1),
            "/bin/zsh".to_string(),
            "/".to_string(),
            80,
            24,
        );
        assert!(session.startup_cleanup_path.is_none());
        session.close_fd();
    }

    #[test]
    fn child_environment_requests_color_terminal() {
        let env = child_environment_with_startup(None);
        let values: Vec<&str> = env.iter().filter_map(|value| value.to_str().ok()).collect();
        assert!(values.contains(&"TERM=xterm-256color"));
        assert!(values.contains(&"COLORTERM=truecolor"));
        if let Ok(home) = std::env::var("HOME") {
            let path = values.iter().find_map(|value| value.strip_prefix("PATH="));
            assert!(path.is_some_and(|path| {
                path.split(':')
                    .any(|entry| entry == format!("{home}/.bun/bin"))
            }));
        }
    }

    #[tokio::test]
    async fn spawn_with_same_id_returns_existing() {
        let manager = SessionManager::new();

        // First spawn
        let session1 = manager
            .spawn("dup_id".to_string(), "/bin/sh", "/", 80, 24)
            .await
            .expect("first spawn should succeed");

        // Second spawn with same ID – TOCTOU should detect the existing session.
        let session2 = manager
            .spawn("dup_id".to_string(), "/bin/sh", "/", 80, 24)
            .await
            .expect("second spawn should return existing");

        assert!(Arc::ptr_eq(&session1, &session2));
        assert_eq!(manager.list_sessions().await.len(), 1);
        assert!(manager.has_session("dup_id").await);
    }

    #[tokio::test]
    async fn spawn_replaces_an_exited_session() {
        let manager = SessionManager::new();
        let first = manager
            .spawn("respawn_id".to_string(), "/bin/sh", "/", 80, 24)
            .await
            .expect("first spawn should succeed");
        first.mark_exited().await;

        let replacement = manager
            .spawn("respawn_id".to_string(), "/bin/sh", "/", 80, 24)
            .await
            .expect("respawn should replace exited session");

        assert!(!Arc::ptr_eq(&first, &replacement));
        assert_eq!(
            manager.list_sessions().await,
            vec!["respawn_id".to_string()]
        );
        let _ = manager.kill("respawn_id").await;
    }

    #[tokio::test]
    async fn spawn_concurrent_same_id_races_to_single_session() {
        let manager = SessionManager::new();

        let f1 = manager.spawn("race_id".to_string(), "/bin/sh", "/", 80, 24);
        let f2 = manager.spawn("race_id".to_string(), "/bin/sh", "/", 80, 24);

        let (r1, r2) = tokio::join!(f1, f2);
        let s1 = r1.expect("first spawn should succeed");
        let s2 = r2.expect("second spawn should return existing");

        assert!(Arc::ptr_eq(&s1, &s2));
        assert_eq!(manager.list_sessions().await.len(), 1);
    }

    /// Task 4.3 atomic check-insert: 5 concurrent spawns with the same id must
    /// produce exactly one underlying PTY, and all 5 callers must receive the
    /// same `Arc`. Without the write-lock-held-throughout fix, races between
    /// the read-lock check and the write-lock insert would let multiple
    /// callers fork(); the second `insert` would then overwrite the first,
    /// dropping its `Arc<TerminalSession>` and sending SIGTERM to an
    /// unrelated process group.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn spawn_5_concurrent_same_id_yields_single_pty() {
        let manager = SessionManager::new();

        // Launch all 5 spawns before awaiting any of them, so they actually
        // race on the write lock instead of serializing in the test harness.
        let handles = (0..5)
            .map(|i| {
                let mgr = &manager;
                // Use a distinct id per task to keep the test focused on
                // intra-id races only after the dup_id was inserted first.
                async move {
                    mgr.spawn(format!("race_5_id_{i}"), "/bin/sh", "/", 80, 24)
                        .await
                }
            })
            .collect::<Vec<_>>();

        // Now spawn the actual 5-way race on a single id, after the warm-up.
        // (The above warm-up is just to make sure the manager is exercised
        // before the timing-sensitive race below; not strictly required, but
        // it makes the test less dependent on scheduler quirks.)
        drop(handles);

        let manager = std::sync::Arc::new(manager);
        let mut joins = Vec::new();
        for _ in 0..5 {
            let m = manager.clone();
            joins.push(tokio::spawn(async move {
                m.spawn("same_id".to_string(), "/bin/sh", "/", 80, 24).await
            }));
        }

        let mut sessions: Vec<Arc<TerminalSession>> = Vec::with_capacity(5);
        for j in joins {
            let s = j
                .await
                .expect("join should not panic")
                .expect("spawn should succeed");
            sessions.push(s);
        }

        // All 5 callers must see the same allocation. If even one caller
        // forked()'d independently, its Arc would differ.
        let first = &sessions[0];
        for (i, s) in sessions.iter().enumerate() {
            assert!(
                Arc::ptr_eq(first, s),
                "caller {i} received a different Arc — concurrent spawn leaked an orphan PTY"
            );
        }

        // Exactly one entry in the session map.
        let listed = manager.list_sessions().await;
        assert_eq!(
            listed.len(),
            1,
            "expected exactly one session, got {listed:?}"
        );
        assert!(manager.has_session("same_id").await);
    }

    #[tokio::test]
    async fn spawn_nonexistent_shell_fails_cleanly() {
        let manager = SessionManager::new();

        let result = manager
            .spawn("fail_id".to_string(), "/nonexistent/shell", "/", 80, 24)
            .await;

        assert!(result.is_err(), "expected spawn to fail with invalid shell");
        let err = result.err().expect("already checked");
        assert!(err.to_string().contains("execvp"));
        assert!(!manager.has_session("fail_id").await);
    }

    #[test]
    fn listener_generation_ignores_stale_detach() {
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let foreign_pgid = nix::unistd::Pid::from_raw(1);
        let session = TerminalSession::new(
            "listener-generation".to_string(),
            read_end.into_raw_fd(),
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        let first = session
            .attach_listener("owner-a".to_string(), false)
            .expect("first owner should attach");
        assert!(session
            .attach_listener("owner-b".to_string(), false)
            .is_none());
        assert!(!session.raw_paused.load(Ordering::Acquire));
        assert!(session.detach_listener("owner-a", first));
        assert!(session.raw_paused.load(Ordering::Acquire));
        let second = session
            .attach_listener("owner-b".to_string(), false)
            .expect("paused session should accept replacement owner");
        assert!(second > first);
        assert!(!session.raw_paused.load(Ordering::Acquire));
        assert!(!session.detach_listener("owner-a", second));
        assert!(!session.raw_paused.load(Ordering::Acquire));
        assert!(session.detach_listener("owner-b", second));
        assert!(session
            .attach_listener("owner-b".to_string(), false)
            .is_none());
        assert!(session.raw_paused.load(Ordering::Acquire));
        let third = session
            .attach_listener("owner-c".to_string(), false)
            .expect("new owner should reclaim paused session");
        assert!(third > second);
        assert!(!session.raw_paused.load(Ordering::Acquire));
        assert!(session.detach_listener("owner-c", third));

        // A replacement may supersede an old owner even if that owner already
        // attached and cleared the pause before the replacement arrived.
        let fourth = session
            .attach_listener("owner-d".to_string(), true)
            .expect("replacement owner should supersede the old live owner");
        assert!(fourth > third);
        assert!(!session.detach_listener("owner-c", fourth));
        assert!(session.detach_listener("owner-d", fourth));
        session.close_fd();
    }

    #[test]
    fn resize_owner_gate_accepts_current_and_pending_mounts_only() {
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let foreign_pgid = nix::unistd::Pid::from_raw(1);
        let session = TerminalSession::new(
            "resize-owner-gate".to_string(),
            read_end.into_raw_fd(),
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        // Legacy/relay callers remain supported when no owner is supplied.
        assert!(session.can_resize_for_owner(None));
        assert!(!session.can_resize_for_owner(Some("owner-a")));

        session.begin_startup_pause(Some("owner-a".to_string()));
        assert!(session.can_resize_for_owner(Some("owner-a")));
        assert!(!session.can_resize_for_owner(Some("owner-b")));
        let generation = session
            .attach_listener("owner-a".to_string(), false)
            .expect("startup owner should attach");
        assert!(session.can_resize_for_owner(Some("owner-a")));
        assert!(!session.can_resize_for_owner(Some("owner-b")));

        assert!(session.detach_listener("owner-a", generation));
        // A replacement may resize while the old owner is detached and output
        // remains paused, before it completes the listener handoff.
        assert!(session.can_resize_for_owner(Some("owner-b")));
        assert!(!session.can_resize_for_owner(Some("owner-a")));
        session.close_fd();
    }

    #[test]
    fn owned_fd_lease_survives_close_and_master_fd_reuse() {
        // The body below recycles a freed fd number with dup2. That is only
        // race-free when no other thread is allocating descriptors, which
        // parallel `cargo test` cannot guarantee (other tests spawn PTYs
        // without the FD_RECYCLE_LOCK). Re-run the assertions in a fresh
        // single-test subprocess so fd allocation is deterministic.
        if std::env::var_os("ATHENA_FD_LEASE_TEST_CHILD").is_none() {
            let exe = std::env::current_exe().expect("current test binary");
            let status = std::process::Command::new(exe)
                .arg("--exact")
                .arg("session::tests::owned_fd_lease_survives_close_and_master_fd_reuse")
                .env("ATHENA_FD_LEASE_TEST_CHILD", "1")
                .status()
                .expect("spawning fd-lease child test should succeed");
            assert!(status.success(), "fd-lease child test failed");
            return;
        }
        let _guard = FD_RECYCLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let raw_fd = read_end.into_raw_fd();
        let foreign_pgid = nix::unistd::Pid::from_raw(1);
        let session = TerminalSession::new(
            "fd-lease".to_string(),
            raw_fd,
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        let leased_fd = session
            .duplicate_master_fd()
            .expect("duplicating the live master fd should succeed");
        let flags = unsafe { libc::fcntl(leased_fd.as_raw_fd(), libc::F_GETFL) };
        assert!(
            flags >= 0,
            "F_GETFL should succeed: {}",
            io::Error::last_os_error()
        );
        assert_eq!(
            unsafe {
                libc::fcntl(
                    leased_fd.as_raw_fd(),
                    libc::F_SETFL,
                    flags | libc::O_NONBLOCK,
                )
            },
            0,
            "setting O_NONBLOCK should succeed"
        );

        assert!(session.close_fd());

        // Recycle the session's old integer for an unrelated pipe endpoint.
        // dup2 makes this deterministic even if another test has allocated a
        // descriptor between close_fd and pipe().
        let (replacement_read, replacement_write) =
            nix::unistd::pipe().expect("replacement pipe() should succeed");
        if replacement_read.as_raw_fd() != raw_fd {
            assert_eq!(
                unsafe { libc::dup2(replacement_read.as_raw_fd(), raw_fd) },
                raw_fd,
                "dup2 should recycle the old master fd number"
            );
        }
        let byte = *b"x";
        assert_eq!(
            unsafe {
                libc::write(
                    replacement_write.as_raw_fd(),
                    byte.as_ptr() as *const libc::c_void,
                    byte.len(),
                )
            },
            1
        );

        let mut buf = [0u8; 1];
        let read_result = unsafe {
            libc::read(
                leased_fd.as_raw_fd(),
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            )
        };
        assert_eq!(
            read_result, -1,
            "the owned lease must not follow the recycled raw fd"
        );
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::EAGAIN),
            "the original endpoint should still be empty and non-blocking"
        );
        assert_eq!(
            unsafe { libc::read(raw_fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len(),) },
            1,
            "the recycled raw fd should refer to the replacement pipe"
        );

        if replacement_read.as_raw_fd() != raw_fd {
            unsafe { libc::close(raw_fd) };
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn resize_rechecks_owner_after_waiting_for_resize_lock() {
        let manager = Arc::new(SessionManager::new());
        let session = manager
            .spawn("resize-owner-race".to_string(), "/bin/sh", "/", 80, 24)
            .await
            .expect("spawn should succeed");
        let generation = session
            .attach_listener("owner-a".to_string(), false)
            .expect("owner-a should attach");

        let probe = ResizeTestProbe::new();
        session.install_resize_test_probe(probe.clone());
        let resize_guard = session.resize_lock.lock().await;
        let acquired = probe.acquired.notified();
        let resize_manager = manager.clone();
        let resize_task = tokio::spawn(async move {
            resize_manager
                .resize("resize-owner-race", 100, 30, Some("owner-a"))
                .await
        });

        tokio::task::yield_now().await;
        drop(resize_guard);
        acquired.await;

        assert!(session.detach_listener("owner-a", generation));
        assert!(session
            .attach_listener("owner-b".to_string(), false)
            .is_some());
        probe.continue_after_acquire.notify_one();

        let error = resize_task
            .await
            .expect("resize task should not panic")
            .expect_err("owner-a must be rejected after owner-b takes over");
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        manager
            .kill("resize-owner-race")
            .await
            .expect("kill should succeed");
    }

    #[test]
    fn cancelled_startup_owner_cannot_reclaim() {
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let foreign_pgid = nix::unistd::Pid::from_raw(1);
        let session = TerminalSession::new(
            "startup-lease-cancel".to_string(),
            read_end.into_raw_fd(),
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        session.begin_startup_pause(Some("owner-a".to_string()));
        assert!(session.cancel_startup_pause("owner-a"));
        assert!(!session.raw_paused.load(Ordering::Acquire));
        assert!(session
            .attach_listener("owner-a".to_string(), false)
            .is_none());
        assert!(session
            .attach_listener("owner-b".to_string(), false)
            .is_some());
        session.close_fd();
    }

    #[test]
    fn expired_startup_owner_cannot_reclaim() {
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let foreign_pgid = nix::unistd::Pid::from_raw(1);
        let session = TerminalSession::new(
            "startup-lease-expiry".to_string(),
            read_end.into_raw_fd(),
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        session.begin_startup_pause(Some("owner-a".to_string()));
        *session
            .startup_pause_deadline
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) =
            Some(Instant::now() - Duration::from_secs(1));
        assert!(session.expire_startup_pause());
        assert!(!session.raw_paused.load(Ordering::Acquire));
        assert!(session
            .attach_listener("owner-a".to_string(), false)
            .is_none());
        assert!(session
            .attach_listener("owner-b".to_string(), false)
            .is_some());
        session.close_fd();
    }

    /// Only one caller may claim a session's background PTY reader.
    #[test]
    fn read_loop_claim_is_single_use() {
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let raw_fd = read_end.into_raw_fd();
        let foreign_pgid = nix::unistd::Pid::from_raw(1);
        let session = TerminalSession::new(
            "reader-claim".to_string(),
            raw_fd,
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        assert!(session.try_claim_read_loop());
        assert!(!session.try_claim_read_loop());
        session.close_fd();
    }

    /// close_fd is idempotent and atomically swaps the fd to -1.
    #[test]
    fn close_fd_is_idempotent_and_swaps_to_sentinel() {
        // Create a real fd via pipe() so libc::close has something valid to
        // close. We only need one end — the other end is closed immediately
        // and forgotten.
        let (read_end, _write_end) = nix::unistd::pipe().expect("pipe() should succeed");
        let raw_fd = read_end.into_raw_fd();

        // PGID 1 = launchd on macOS / init on Linux. We have no permission
        // to signal it, so `Drop`'s `killpg(self.pgid, SIGTERM)` is a benign
        // no-op (returns EPERM, discarded). Using pid 0 here would target
        // the test process's own process group and kill the test runner.
        let foreign_pgid = nix::unistd::Pid::from_raw(1);

        let session = TerminalSession::new(
            "test".to_string(),
            raw_fd,
            foreign_pgid,
            foreign_pgid,
            "/bin/sh".to_string(),
            "/".to_string(),
            80,
            24,
        );

        // Sanity: the sentinel holds the real fd we provided.
        assert_eq!(session.master_fd.load(Ordering::Acquire), raw_fd);

        // First call: should report it actually closed the fd.
        assert!(
            session.close_fd(),
            "first close_fd should report it closed a fd"
        );
        assert_eq!(
            session.master_fd.load(Ordering::Acquire),
            -1,
            "sentinel should be -1 after close"
        );

        // Second call: should be a no-op.
        assert!(
            !session.close_fd(),
            "second close_fd should report nothing to close"
        );
        assert_eq!(
            session.master_fd.load(Ordering::Acquire),
            -1,
            "sentinel should remain -1 on second close"
        );
    }

    /// Regression test for the large-paste data-loss bug.
    ///
    /// The PTY master fd is opened O_NONBLOCK, so a write larger than the
    /// kernel pipe buffer (~64 KB on macOS) returns EAGAIN when the buffer
    /// fills mid-write. The `do_write` loop must retry on EAGAIN instead of
    /// returning a fatal error — otherwise a paste >~10 lines is truncated
    /// and the tail silently dropped (the original symptom).
    ///
    /// This test writes a 256 KB payload (far exceeding any pipe buffer) to a
    /// real PTY running `cat`, while a background reader drains `cat`'s echoed
    /// stdout so the PTY output pipe doesn't fill and block the child. It
    /// asserts every byte was reported written. Without the EAGAIN retry,
    /// `write()` returns a `WouldBlock` error and the test fails.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn large_write_retries_on_eagain_and_lands_full_payload() {
        let manager = SessionManager::new();
        let session = manager
            .spawn("large_paste".to_string(), "/bin/cat", "/", 80, 24)
            .await
            .expect("spawn should succeed");

        // Ensure the child has exec'd so the PTY is ready.
        session.mark_ready().await;

        // Drain cat's echoed stdout in the background so the output side of the
        // PTY never fills and stalls the child (which would in turn stall our
        // stdin write). Read until the deadline.
        let drain_session = session.clone();
        let drain_handle = tokio::spawn(async move {
            let mut buf = [0u8; 8192];
            // Drain for up to 10s; errors/EOF are expected once the session is
            // killed at the end of the test.
            let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(10);
            loop {
                match tokio::time::timeout_at(deadline, drain_session.read_bytes(&mut buf, 50))
                    .await
                {
                    Err(_) => break,     // deadline elapsed
                    Ok(Err(_)) => break, // fd closed
                    Ok(Ok(_)) => continue,
                }
            }
        });

        // 256 KB of repeated line content — larger than any OS pipe buffer.
        let lines: Vec<String> = (0..4096).map(|i| format!("paste-line-{i:04}\n")).collect();
        let payload: String = lines.join("");

        let written = tokio::time::timeout(
            std::time::Duration::from_secs(10),
            session.write(payload.as_bytes()),
        )
        .await
        .expect("write should not hang or return a WouldBlock error on a full pipe")
        .expect("write should not return a WouldBlock error on a full pipe");

        assert_eq!(
            written,
            payload.len(),
            "expected the full {}-byte paste to be written, got {} (EAGAIN retry dropped data)",
            payload.len(),
            written,
        );

        // Cleanup: kill the session and let the drain reader exit.
        let _ = manager.kill("large_paste").await;
        let _ = drain_handle.await;
    }
    /// F10: `read_bytes` parks in poll(2) instead of spinning. An idle fd
    /// returns `Ok(0)` after ~`wait_ms` (not immediately), and data written
    /// mid-window wakes the poll promptly — the read must not wait out the
    /// full window once bytes are ready.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn read_bytes_parks_on_eagain_and_wakes_on_data() {
        let manager = SessionManager::new();
        let session = manager
            .spawn("f10_poll".to_string(), "/bin/cat", "/", 80, 24)
            .await
            .expect("spawn should succeed");

        // Flush the Spawning gate: without mark_ready, write() queues to
        // pending_writes forever in a unit test (nothing runs the read loop
        // that normally flips the status).
        session.mark_ready().await;

        let mut buf = [0u8; 4096];
        // Phase 1 — data mid-window: write, then read with a long window.
        // The poll must wake on readiness well before the window expires.
        // (Done first: proves echo works before the idle phase runs.)
        session
            .write(b"ping\n")
            .await
            .expect("write should succeed");
        let echo_start = std::time::Instant::now();
        let echo_deadline = echo_start + std::time::Duration::from_secs(5);
        let n = loop {
            let n = session
                .read_bytes(&mut buf, 1_000)
                .await
                .expect("data read should not error");
            if n > 0 || std::time::Instant::now() >= echo_deadline {
                break n;
            }
        };
        assert!(n > 0, "read should see the echoed bytes");
        assert!(
            echo_start.elapsed() < std::time::Duration::from_millis(900),
            "echo took {:?}; poll readiness wake is broken",
            echo_start.elapsed()
        );

        // Phase 2 — idle: drain any residual echo, then a 300 ms window on
        // a quiet fd must NOT return instantly (the old EAGAIN path returned
        // Ok(0) immediately); it should park for roughly the window.
        let quiet_deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while session
            .read_bytes(&mut buf, 50)
            .await
            .expect("drain read should not error")
            > 0
            && std::time::Instant::now() < quiet_deadline
        {}
        let idle_start = std::time::Instant::now();
        let n = session
            .read_bytes(&mut buf, 300)
            .await
            .expect("idle read should not error");
        assert_eq!(n, 0, "idle fd should report no data");
        assert!(
            idle_start.elapsed() >= std::time::Duration::from_millis(250),
            "idle read returned after {:?}; poll wait was skipped",
            idle_start.elapsed()
        );

        let _ = manager.kill("f10_poll").await;
    }
}
