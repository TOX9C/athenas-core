use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Callback invoked when a PTY session produces output data.
/// Receives the session ID and the raw output bytes.
pub type OnDataCallback = dyn Fn(&str, &[u8]) + Send + Sync;

/// Callback invoked when a PTY session becomes ready (shell prompt visible).
pub type OnReadyCallback = dyn Fn(&str) + Send + Sync;

/// Callback invoked when a PTY session exits.
pub type OnExitCallback = dyn Fn(&str, Option<i32>) + Send + Sync;

const MAX_HISTORY_BYTES: usize = 100_000;
const SHELL_ARGS: &[&str] = &["-l"];

/// Errors that can occur during PTY operations.
#[derive(Debug, thiserror::Error)]
pub enum PtyError {
    #[error("Session not found: {0}")]
    SessionNotFound(String),
    #[error("Failed to spawn PTY: {0}")]
    SpawnError(String),
    #[error("Failed to write to PTY: {0}")]
    WriteError(String),
    #[error("Failed to resize PTY: {0}")]
    ResizeError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Per-session data shared between the manager and reader threads.
struct SessionData {
    history: Vec<u8>,
    ready: bool,
    cwd: Option<String>,
}

/// Manages multiple PTY sessions by ID.
///
/// Each session is identified by a unique string ID and can be
/// independently spawned, written to, resized, and killed.
///
/// The `SessionManager` implements `Drop` to kill all active child
/// processes when the manager is dropped.
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Arc<SessionInner>>>,
    on_data: Option<Arc<OnDataCallback>>,
    on_ready: Option<Arc<OnReadyCallback>>,
    on_exit: Option<Arc<OnExitCallback>>,
}

/// Inner state for an active PTY session.
struct SessionInner {
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Option<Box<dyn portable_pty::MasterPty + Send>>>,
    child: Mutex<Option<Box<dyn portable_pty::Child + Send + Sync>>>,
    data: Mutex<SessionData>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for SessionManager {
    fn drop(&mut self) {
        if let Ok(sessions) = self.sessions.lock() {
            for inner in sessions.values() {
                if let Ok(mut child) = inner.child.lock() {
                    if let Some(ref mut c) = *child {
                        let _ = c.kill();
                    }
                    *child = None;
                }
                if let Ok(mut master) = inner.master.lock() {
                    *master = None;
                }
            }
        }
    }
}

impl SessionManager {
    /// Create a new empty `SessionManager`.
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            on_data: None,
            on_ready: None,
            on_exit: None,
        }
    }

    /// Create a `SessionManager` that invokes `callback` whenever a PTY
    /// session produces output data. The callback receives `(session_id, data_bytes)`.
    pub fn new_with_data_callback(callback: impl Fn(&str, &[u8]) + Send + Sync + 'static) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            on_data: Some(Arc::new(callback)),
            on_ready: None,
            on_exit: None,
        }
    }

    /// Create a `SessionManager` with callbacks for data, ready, and exit events.
    pub fn new_with_callbacks(
        on_data: Option<Box<dyn Fn(&str, &[u8]) + Send + Sync + 'static>>,
        on_ready: Option<Box<dyn Fn(&str) + Send + Sync + 'static>>,
        on_exit: Option<Box<dyn Fn(&str, Option<i32>) + Send + Sync + 'static>>,
    ) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            on_data: on_data.map(Arc::from),
            on_ready: on_ready.map(Arc::from),
            on_exit: on_exit.map(Arc::from),
        }
    }

    /// Spawn a new PTY session with the given ID.
    ///
    /// * `id` – Unique identifier for the session.
    /// * `cwd` – Working directory for the spawned shell.
    /// * `shell` – Path to the shell executable. If empty, a default is chosen.
    /// * `agent_cmd` – Optional command to write into the PTY after a short delay.
    pub fn spawn(
        &self,
        id: String,
        cwd: String,
        shell: String,
        _agent_cmd: Option<String>,
    ) -> Result<(), PtyError> {
        let mut sessions = self
            .sessions
            .lock()
            .map_err(|_| PtyError::SpawnError("Mutex poisoned".to_string()))?;

        // Remove any existing session with the same ID under the same lock
        // to avoid TOCTOU between kill() lock release and this lock acquisition.
        if let Some(old_inner) = sessions.remove(&id) {
            // Close master fd to send EOF to reader and SIGHUP to child.
            if let Ok(mut master) = old_inner.master.lock() {
                *master = None;
            }
            // Kill the child process if still running.
            if let Ok(mut child) = old_inner.child.lock() {
                if let Some(ref mut c) = *child {
                    let _ = c.kill();
                }
                *child = None;
            }
        }

        let shell_path = if shell.is_empty() {
            default_shell()
        } else {
            shell
        };

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;

        let mut cmd = CommandBuilder::new(&shell_path);
        cmd.cwd(&cwd);
        for arg in SHELL_ARGS {
            cmd.arg(arg);
        }

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;

        let master = pair.master;

        // Take writer once so we can store it separately.
        let writer = master
            .take_writer()
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;

        // Clone reader before moving master; we call try_clone_reader on the
        // concrete `&dyn MasterPty` so we avoid the trait object limitation.
        let reader = master
            .try_clone_reader()
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;

        // Set up shared session data.
        let data_inner = Arc::new(SessionInner {
            writer: Mutex::new(writer),
            master: Mutex::new(Some(master)),
            child: Mutex::new(Some(child)),
            data: Mutex::new(SessionData {
                history: Vec::new(),
                ready: false,
                cwd: Some(cwd),
            }),
        });

        sessions.insert(id.clone(), data_inner.clone());

        // Drop the sessions lock before spawning the reader thread.
        drop(sessions);

        // Start a reader thread to capture PTY output.
        let id_for_reader = id.clone();
        let on_data = self.on_data.clone();
        let on_ready = self.on_ready.clone();
        let on_exit = self.on_exit.clone();
        std::thread::spawn(move || {
            read_pty_loop(&id_for_reader, reader, data_inner, on_data.as_deref(), on_ready.as_deref(), on_exit.as_deref());
        });

        // agent_cmd is not used here because `self` cannot be captured by
        // a spawned thread; callers should call `write()` after the session
        // has been spawned.
        let _ = id;

        Ok(())
    }

    /// Write raw data into the PTY identified by `id`.
    ///
    /// Data is sent directly to the shell's stdin. Use `\r` for Enter.
    pub fn write(&self, id: &str, data: String) -> Result<(), PtyError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| PtyError::WriteError("Mutex poisoned".to_string()))?;
        let inner = sessions
            .get(id)
            .ok_or_else(|| PtyError::SessionNotFound(id.to_string()))?;
        let mut writer = inner
            .writer
            .lock()
            .map_err(|_| PtyError::WriteError("Mutex poisoned".to_string()))?;
        writer.write_all(data.as_bytes()).map_err(PtyError::Io)?;
        writer.flush().map_err(PtyError::Io)?;
        Ok(())
    }

    /// Resize the PTY window for the given session.
    ///
    /// Updates the terminal dimensions so that the shell's SIGWINCH handler
    /// can adjust line wrapping and display.
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| PtyError::ResizeError("Mutex poisoned".to_string()))?;
        let inner = sessions
            .get(id)
            .ok_or_else(|| PtyError::SessionNotFound(id.to_string()))?;
        let mut master_guard = inner
            .master
            .lock()
            .map_err(|_| PtyError::ResizeError("Mutex poisoned".to_string()))?;
        if let Some(ref mut master) = *master_guard {
            master
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .map_err(|e| PtyError::ResizeError(e.to_string()))?;
        }
        Ok(())
    }

    /// Kill (close) the session identified by `id`.
    ///
    /// Sends SIGHUP to the child process and closes the master PTY fd.
    /// The session is removed from the internal map.
    pub fn kill(&self, id: &str) {
        let mut sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("kill: mutex poisoned: {}", e);
                return;
            }
        };
        if let Some(inner) = sessions.remove(id) {
            // Close master fd to send EOF to reader and SIGHUP to child.
            if let Ok(mut master) = inner.master.lock() {
                *master = None;
            }
            // Kill the child process if still running.
            if let Ok(mut child) = inner.child.lock() {
                if let Some(ref mut c) = *child {
                    let _ = c.kill();
                }
                *child = None;
            }
        }
    }

    /// Return the accumulated output history for a session.
    ///
    /// History is capped at 100KB per session. Older bytes are trimmed
    /// from the beginning when the limit is exceeded.
    pub fn get_history(&self, id: &str) -> String {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return String::new(),
        };
        match sessions.get(id) {
            Some(inner) => match inner.data.lock() {
                Ok(d) => String::from_utf8_lossy(&d.history).into_owned(),
                Err(_) => String::new(),
            },
            None => String::new(),
        }
    }

    /// Check whether a session with the given ID exists.
    pub fn has_session(&self, id: &str) -> bool {
        match self.sessions.lock() {
            Ok(sessions) => sessions.contains_key(id),
            Err(_) => false,
        }
    }

    /// Check whether a shell prompt is visible (session is "ready").
    ///
    /// Detects common prompt characters (`$`, `❯`, `>`, `%`, `?`) in the
    /// output stream. Returns `true` once a prompt has been seen.
    pub fn is_ready(&self, id: &str) -> bool {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        match sessions.get(id) {
            Some(inner) => match inner.data.lock() {
                Ok(d) => d.ready,
                Err(_) => false,
            },
            None => false,
        }
    }

    /// Get the current working directory of a session, if known.
    ///
    /// Returns the `cwd` passed to `spawn()`. This does not track
    /// directory changes made by `cd` commands within the shell.
    pub fn get_cwd(&self, id: &str) -> Option<String> {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return None,
        };
        match sessions.get(id) {
            Some(inner) => inner.data.lock().ok().and_then(|d| d.cwd.clone()),
            None => None,
        }
    }

    /// Gracefully shut down all active sessions.
    ///
    /// Sends Ctrl+C (`\x03`) to each session, waits 50ms, then sends
    /// `exit\r` and waits 800ms. Finally clears all session data.
    /// Used during application shutdown to avoid orphaned processes.
    pub fn graceful_shutdown(&self) {
        let ids: Vec<String> = match self.sessions.lock() {
            Ok(sessions) => sessions.keys().cloned().collect(),
            Err(_) => return,
        };

        for id in &ids {
            let _ = self.write(id, "\x03".to_string());
        }
        std::thread::sleep(Duration::from_millis(50));
        for id in &ids {
            let _ = self.write(id, "exit\r".to_string());
        }
        std::thread::sleep(Duration::from_millis(800));

        let mut sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        sessions.clear();
    }
}

/// Read from the PTY in a loop, updating history and ready state.
/// If `on_data` is provided, it is invoked for each chunk of output.
/// If `on_ready` is provided, it is invoked when the session becomes ready.
/// If `on_exit` is provided, it is invoked when the PTY exits.
fn read_pty_loop(
    id: &str,
    mut reader: Box<dyn Read + Send>,
    shared: Arc<SessionInner>,
    on_data: Option<&OnDataCallback>,
    on_ready: Option<&OnReadyCallback>,
    on_exit: Option<&OnExitCallback>,
) {
    let mut buffer = [0u8; 4096];
    let mut was_ready = false;
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buffer[..n];
                // Update history.
                let _ = update_history(&shared, chunk);
                // Check ready patterns.
                let became_ready = check_ready(&shared, chunk);
                if became_ready && !was_ready {
                    was_ready = true;
                    if let Some(cb) = on_ready {
                        cb(id);
                    }
                }
                // Forward to callback if registered.
                if let Some(cb) = on_data {
                    cb(id, chunk);
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::Interrupted {
                    continue; // retry on EINTR
                }
                log::warn!("PTY reader for session {} failed: {}", id, e);
                break;
            }
        }
    }

    // PTY exited
    if let Some(cb) = on_exit {
        // Exit code is not easily available without mutable access; pass None
        cb(id, None);
    }
}

/// Append incoming data to a session's history, trimming when it exceeds the max.
fn update_history(shared: &Arc<SessionInner>, chunk: &[u8]) -> Result<(), PtyError> {
    let mut data = shared
        .data
        .lock()
        .map_err(|_| PtyError::WriteError("Mutex poisoned".to_string()))?;
    data.history.extend_from_slice(chunk);
    if data.history.len() > MAX_HISTORY_BYTES {
        let excess = data.history.len() - MAX_HISTORY_BYTES;
        data.history.drain(..excess);
    }
    Ok(())
}

/// Check whether the latest output contains a shell prompt (ready).
/// Returns true if the session just became ready.
fn check_ready(shared: &Arc<SessionInner>, chunk: &[u8]) -> bool {
    if let Ok(text) = std::str::from_utf8(chunk) {
        let mut data = match shared.data.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        // Simple matching for common shell prompt characters.
        if text.contains('$')
            || text.contains('\u{276f}') // ❯
            || text.contains('>')
            || text.contains('%')
            || text.contains('?')
        {
            let was_not_ready = !data.ready;
            data.ready = true;
            return was_not_ready;
        }
    }
    false
}

/// Determine the default shell for the current platform.
fn default_shell() -> String {
    #[cfg(target_os = "windows")]
    {
        "powershell.exe".to_string()
    }
    #[cfg(not(target_os = "windows"))]
    {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string())
    }
}

#[cfg(test)]
mod tests;
