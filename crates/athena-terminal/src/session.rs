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
        let _ = nix::sys::signal::killpg(self.shell_pid, nix::sys::signal::Signal::SIGTERM);
    }
}

impl TerminalSession {
    pub fn new(
        id: String,
        master_fd: RawFd,
        shell_pid: nix::unistd::Pid,
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
        {
            let sessions = self.sessions.read().await;
            if let Some(existing) = sessions.get(&id).cloned() {
                info!("PTY session {} already exists, returning existing", id);
                return Ok(existing);
            }
        }
        let winsize = Winsize {
            ws_row: rows,
            ws_col: cols,
            ws_xpixel: 0,
            ws_ypixel: 0,
        };

        let pty = openpty(Some(&winsize), None).map_err(|e| io::Error::other(e.to_string()))?;
        // Consume the OwnedFds so they don't double-close when pty drops.
        let master_fd = pty.master.into_raw_fd();
        let slave_fd = pty.slave.into_raw_fd();

        let shell_cstr = CString::new(shell.as_bytes())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
        let args: Vec<CString> = vec![shell_cstr.clone()];

        match unsafe { fork() } {
            Ok(ForkResult::Child) => {
                // In child: set up PTY and exec shell
                let _ = close(master_fd);
                setsid().ok();
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
                let _ = nix::unistd::execvp(&shell_cstr, &args);
                std::process::exit(1);
            }
            Ok(ForkResult::Parent { child }) => {
                let _ = close(slave_fd);
                unsafe {
                    let flags = libc::fcntl(master_fd, libc::F_GETFL, 0);
                    libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
                let session = Arc::new(TerminalSession::new(
                    id.clone(),
                    master_fd,
                    child,
                    shell.to_string(),
                    cwd.to_string(),
                    cols as usize,
                    rows as usize,
                ));
                let mut sessions = self.sessions.write().await;
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
            let pid = session.shell_pid;
            let _ = nix::sys::signal::killpg(pid, nix::sys::signal::Signal::SIGTERM);
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
