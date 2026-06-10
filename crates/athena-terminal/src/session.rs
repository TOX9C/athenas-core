use crate::ansi::handler::{apply_ops, AnsiHandler};
use crate::grid::CellDelta;
use crate::grid::Grid;
use log::info;
use nix::pty::{openpty, Winsize};
use nix::unistd::{close, fork, setsid, ForkResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::ffi::CString;
use std::io;
use std::os::unix::io::{IntoRawFd, RawFd};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use vte::Parser;

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
pub struct TerminalSession {
    pub id: String,
    pub grid: Arc<Mutex<Grid>>,
    pub master_fd: RawFd,
    fd_closed: AtomicBool,
    pub shell_pid: nix::unistd::Pid,
    /// The process group ID of the shell. Explicitly stored because it is only
    /// equal to `shell_pid` if `setsid()` succeeded. If `setsid()` failed, the
    /// shell remains in the parent's process group and `killpg(shell_pid)` would
    /// kill the parent process group.
    pub pgid: nix::unistd::Pid,
    pub shell: String,
    pub cwd: String,
    pub status: Mutex<PtyStatus>,
    pub pending_writes: Mutex<VecDeque<Vec<u8>>>,
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
        if !self.fd_closed.swap(true, Ordering::SeqCst) {
            let _ = close(self.master_fd);
        }
        let _ = nix::sys::signal::killpg(self.pgid, nix::sys::signal::Signal::SIGTERM);
    }
}

impl TerminalSession {
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
        Self {
            id,
            grid: Arc::new(Mutex::new(Grid::new(cols, rows))),
            master_fd,
            fd_closed: AtomicBool::new(false),
            shell_pid,
            pgid,
            shell,
            cwd,
            status: Mutex::new(PtyStatus::Spawning),
            pending_writes: Mutex::new(VecDeque::new()),
            parser: Mutex::new(Parser::new()),
        }
    }

    /// Write data to the PTY master fd.
    /// If the session is not yet ready, data is queued for later.
    pub async fn write(&self, data: &[u8]) -> io::Result<usize> {
        {
            let mut pending = self.pending_writes.lock().await;
            let status = self.status.lock().await;
            if *status == PtyStatus::Spawning {
                drop(status);
                pending.push_back(data.to_vec());
                return Ok(data.len());
            }
            drop(status);
            drop(pending);
        }
        self.do_write(data).await
    }

    /// Internal: perform the actual write to the PTY master fd.
    async fn do_write(&self, data: &[u8]) -> io::Result<usize> {
        let fd = self.master_fd;
        let buf = data.to_vec();
        tokio::task::spawn_blocking(move || {
            let written = unsafe { libc::write(fd, buf.as_ptr() as *const _, buf.len()) };
            if written < 0 {
                let err = io::Error::last_os_error();
                // EIO on a fresh PTY usually means the child hasn't finished exec yet;
                // treat as WouldBlock so callers can retry.
                if err.raw_os_error() == Some(5) {
                    return Err(io::Error::new(io::ErrorKind::WouldBlock, err));
                }
                Err(err)
            } else {
                Ok(written as usize)
            }
        })
        .await
        .map_err(|e| io::Error::other(e))?
    }

    /// Mark the session as ready and flush any pending writes.
    pub async fn mark_ready(&self) {
        let mut status = self.status.lock().await;
        *status = PtyStatus::Ready;
        drop(status);

        let mut pending = self.pending_writes.lock().await;
        while let Some(data) = pending.pop_front() {
            let _ = self.do_write(&data).await;
        }
    }
}

/// Manages all active PTY sessions.
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, Arc<TerminalSession>>>>,
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
        info!(
            "Spawning PTY session: id={}, shell={}, cwd={}",
            id, shell, cwd
        );

        // Fast-path: check under read lock.
        {
            let sessions = self.sessions.read().await;
            if let Some(existing) = sessions.get(&id).cloned() {
                info!("PTY session {} already exists, returning existing", id);
                return Ok(existing);
            }
        }

        // Create a pipe for the child to report pre-exec errors.
        // FD_CLOEXEC ensures the pipe is closed automatically on a successful exec.
        let (err_read, err_write) =
            nix::unistd::pipe().map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?;
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

        let shell_cstr = CString::new(shell.as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let args: Vec<CString> = vec![shell_cstr.clone()];

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
                let _ = nix::unistd::execvp(&shell_cstr, &args);
                // execvp only returns on failure.
                let _ = unsafe { libc::write(err_write, [2u8].as_ptr() as *const _, 1) };
                let _ = close(err_write);
                std::process::exit(1);
            }
            Ok(ForkResult::Parent { child }) => {
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
                    return Err(io::Error::new(io::ErrorKind::Other, err_msg));
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

                let session = Arc::new(TerminalSession::new(
                    id.clone(),
                    master_fd,
                    child,
                    pgid,
                    shell.to_string(),
                    cwd.to_string(),
                    cols as usize,
                    rows as usize,
                ));

                // P1-2 TOCTOU fix: re-check under the write lock before insert.
                // If another thread already created a session for this id,
                // clean up the newly-created resources and return the existing one.
                let mut sessions = self.sessions.write().await;
                if let Some(existing) = sessions.get(&id).cloned() {
                    drop(sessions);
                    if !session.fd_closed.swap(true, Ordering::SeqCst) {
                        let _ = close(session.master_fd);
                    }
                    let _ =
                        nix::sys::signal::killpg(session.pgid, nix::sys::signal::Signal::SIGTERM);
                    let _ =
                        nix::sys::wait::waitpid(child, Some(nix::sys::wait::WaitPidFlag::WNOHANG));
                    return Ok(existing);
                }

                sessions.insert(id.clone(), session.clone());
                Ok(session)
            }
            Err(e) => Err(io::Error::other(e.to_string())),
        }
    }

    pub async fn get_session(&self, id: &str) -> Option<Arc<TerminalSession>> {
        let sessions = self.sessions.read().await;
        sessions.get(id).cloned()
    }

    pub async fn kill(&self, id: &str) -> io::Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.remove(id) {
            if !session.fd_closed.swap(true, Ordering::SeqCst) {
                let _ = close(session.master_fd);
            }
            let pgid = session.pgid;
            let _ = nix::sys::signal::killpg(pgid, nix::sys::signal::Signal::SIGTERM);
        }
        Ok(())
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

    pub async fn resize(&self, id: &str, cols: u16, rows: u16) -> io::Result<()> {
        if let Some(session) = self.get_session(id).await {
            let ws = libc::winsize {
                ws_row: rows,
                ws_col: cols,
                ws_xpixel: 0,
                ws_ypixel: 0,
            };
            let fd = session.master_fd;
            let result = tokio::task::spawn_blocking(move || unsafe {
                libc::ioctl(fd, libc::TIOCSWINSZ, &ws)
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
    /// Returns the number of bytes read (`0` means the fd is non-blocking and
    /// currently has no data; callers should `continue` or sleep).
    ///
    /// This is the entry point for raw byte consumers (e.g. an xterm.js
    /// subscriber) that want the bytes *before* the VTE parser rewrites them
    /// into cell deltas. Pair with `parse_bytes` if you also need the legacy
    /// grid update.
    pub async fn read_bytes(&self, buf: &mut [u8]) -> io::Result<usize> {
        let nbytes = unsafe { libc::read(self.master_fd, buf.as_mut_ptr() as *mut _, buf.len()) };
        if nbytes < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::WouldBlock || e.raw_os_error() == Some(libc::EAGAIN) {
                return Ok(0);
            }
            return Err(e);
        }
        Ok(nbytes as usize)
    }

    /// Feed raw bytes through the persistent VTE parser and apply the
    /// resulting ops to the grid. Returns a `TerminalUpdate` with cell
    /// deltas only if the grid state actually changed.
    pub async fn parse_bytes(&self, data: &[u8]) -> io::Result<Option<TerminalUpdate>> {
        if data.is_empty() {
            return Ok(None);
        }

        // Feed bytes through the persistent parser; the handler is a local
        // sink for completed ops — VTE's state lives in the parser.
        let mut parser_guard = self.parser.lock().await;
        let mut handler = AnsiHandler::new();
        parser_guard.advance(&mut handler, data);
        let ops = handler.ops();
        drop(parser_guard);

        let mut grid_guard = self.grid.lock().await;
        apply_ops(&mut grid_guard, ops);

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
        Ok(update)
    }

    /// Convenience wrapper: `read_bytes` + `parse_bytes` in one call.
    /// Prefer the split methods when you need access to the raw bytes
    /// alongside the parsed deltas (e.g. to fan out to both an xterm.js
    /// subscriber and a legacy grid listener).
    pub async fn read_and_parse(&self, buf: &mut [u8]) -> io::Result<Option<TerminalUpdate>> {
        let n = self.read_bytes(buf).await?;
        if n == 0 {
            return Ok(None);
        }
        self.parse_bytes(&buf[..n]).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
