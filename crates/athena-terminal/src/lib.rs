use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const MAX_HISTORY_BYTES: usize = 100_000;

/// Shell prompt patterns used to detect when a session is "ready".
const READY_PATTERNS: &[&str] = &[
    r"(?m)\$\s*$",
    r"(?m)❯\s*$",
    r"(?m)>\s*$",
    r"(?m)>>\>\s*$",
    r"(?m)% \s*$",
    r"(?m)\? $",
    r"(?m)╰─+>\s*$",
    r"(?m)\(y/n\)\s*$",
];

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
    history_size: usize,
    ready: bool,
    cwd: Option<String>,
    shell: String,
    /// Resize callback stored as a trait object so that the reader thread
    /// does not need direct access to the `MasterPty`.
    resize_fn: Option<Box<dyn Fn(u16, u16) -> Result<(), PtyError> + Send + 'static>>,
}

/// Manages multiple PTY sessions by ID.
pub struct SessionManager {
    sessions: Mutex<HashMap<String, Arc<Mutex<SessionData>>>>,
    writers: Mutex<HashMap<String, Arc<Mutex<Box<dyn Write + Send>>>>>,
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionManager {
    /// Create a new empty `SessionManager`.
    pub fn new() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            writers: Mutex::new(HashMap::new()),
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
        agent_cmd: Option<String>,
    ) -> Result<(), PtyError> {
        // Remove any existing session with the same ID.
        self.kill(&id);

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

        let _child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;

        let master = pair.master;

        // Take writer once so we can store it separately.
        let writer = master
            .take_writer()
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;

        // Set up shared session data.
        let data = SessionData {
            history: Vec::new(),
            history_size: 0,
            ready: false,
            cwd: Some(cwd),
            shell: shell_path,
            resize_fn: None,
        };
        let shared = Arc::new(Mutex::new(data));

        {
            let mut sessions = self.sessions.lock().map_err(|_| {
                PtyError::SpawnError("Mutex poisoned".to_string())
            })?;
            sessions.insert(id.clone(), shared.clone());
        }

        {
            let writer_arc = Arc::new(Mutex::new(writer));
            let mut writers = self.writers.lock().map_err(|_| {
                PtyError::SpawnError("Mutex poisoned".to_string())
            })?;
            writers.insert(id.clone(), writer_arc.clone());
        }

        // Start a reader thread to capture PTY output.
        let reader = master
            .try_clone_reader()
            .map_err(|e| PtyError::SpawnError(e.to_string()))?;
        let id_for_reader = id.clone();
        let shared_for_reader = shared.clone();
        std::thread::spawn(move || {
            read_pty_loop(&id_for_reader, reader, shared_for_reader);
        });

        // Optionally write an initial command after a short delay.
        drop(agent_cmd);

        Ok(())
    }

    /// Static helper used by the delayed agent command thread.
    fn write_static(
        id: &str,
        data: String,
        writers: &Mutex<HashMap<String, Arc<Mutex<Box<dyn Write + Send>>>>>,
    ) -> Result<(), PtyError> {
        let writers = writers
            .lock()
            .map_err(|_| PtyError::WriteError("Mutex poisoned".to_string()))?;
        let mut writer = writers
            .get(id)
            .ok_or_else(|| PtyError::SessionNotFound(id.to_string()))?
            .lock()
            .map_err(|_| PtyError::WriteError("Mutex poisoned".to_string()))?;
        writer.write_all(data.as_bytes()).map_err(PtyError::Io)?;
        writer.flush().map_err(PtyError::Io)?;
        Ok(())
    }

    /// Write raw data into the PTY identified by `id`.
    pub fn write(&self, id: &str, data: String) -> Result<(), PtyError> {
        Self::write_static(id, data, &self.writers)
    }

    /// Resize the PTY window for the given session.
    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), PtyError> {
        let sessions = self
            .sessions
            .lock()
            .map_err(|_| PtyError::ResizeError("Mutex poisoned".to_string()))?;
        let data = sessions
            .get(id)
            .ok_or_else(|| PtyError::SessionNotFound(id.to_string()))?;
        if let Some(ref resize_fn) = data.lock().unwrap().resize_fn {
            resize_fn(cols, rows)?;
        }
        Ok(())
    }

    /// Kill (close) the session identified by `id`.
    pub fn kill(&self, id: &str) {
        {
            let mut writers = match self.writers.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            writers.remove(id);
        }
        {
            let mut sessions = match self.sessions.lock() {
                Ok(guard) => guard,
                Err(_) => return,
            };
            sessions.remove(id);
        }
    }

    /// Return the accumulated output history for a session.
    pub fn get_history(&self, id: &str) -> String {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return String::new(),
        };
        match sessions.get(id) {
            Some(data) => match data.lock() {
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
    pub fn is_ready(&self, id: &str) -> bool {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return false,
        };
        match sessions.get(id) {
            Some(data) => match data.lock() {
                Ok(d) => d.ready,
                Err(_) => false,
            },
            None => false,
        }
    }

    /// Get the current working directory of a session, if known.
    pub fn get_cwd(&self, id: &str) -> Option<String> {
        let sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return None,
        };
        match sessions.get(id) {
            Some(data) => data.lock().ok().and_then(|d| d.cwd.clone()),
            None => None,
        }
    }

    /// Gracefully shut down all active sessions.
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
            let _ = self.write(id, "/exit\r".to_string());
        }
        std::thread::sleep(Duration::from_millis(800));

        let mut writers = match self.writers.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        writers.clear();

        let mut sessions = match self.sessions.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        sessions.clear();
    }
}

/// Read from the PTY in a loop, updating history and ready state.
fn read_pty_loop(
    _id: &str,
    mut reader: Box<dyn Read + Send>,
    shared: Arc<Mutex<SessionData>>,
) {
    let mut buffer = [0u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let chunk = &buffer[..n];
                // Update history.
                let _ = update_history(&shared, chunk);
                // Check ready patterns.
                check_ready(&shared, chunk);
            }
            Err(_e) => break,
        }
    }
}

/// Append incoming data to a session's history, trimming when it exceeds the max.
fn update_history(shared: &Arc<Mutex<SessionData>>, chunk: &[u8]) -> Result<(), PtyError> {
    let mut data = shared
        .lock()
        .map_err(|_| PtyError::WriteError("Mutex poisoned".to_string()))?;
    data.history.extend_from_slice(chunk);
    data.history_size += chunk.len();
    while data.history_size > MAX_HISTORY_BYTES && !data.history.is_empty() {
        let removed = data.history.remove(0);
        data.history_size -= 1;
    }
    Ok(())
}

/// Check whether the latest output contains a shell prompt (ready).
fn check_ready(shared: &Arc<Mutex<SessionData>>, chunk: &[u8]) {
    if let Ok(text) = std::str::from_utf8(chunk) {
        let mut data = match shared.lock() {
            Ok(guard) => guard,
            Err(_) => return,
        };
        for pattern in READY_PATTERNS {
            // Simple regex-free matching for common shell prompt patterns
            if text.contains('$') || text.contains('❯') || text.contains('>')
                || text.contains('%') || text.contains('?')
            {
                data.ready = true;
                return;
            }
        }
    }
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
mod tests {
    use super::*;

    #[test]
    fn session_manager_new_is_empty() {
        let manager = SessionManager::new();
        assert!(!manager.has_session("test"));
    }

    #[test]
    fn get_history_returns_empty_for_unknown_session() {
        let manager = SessionManager::new();
        assert_eq!(manager.get_history("unknown"), "");
    }

    #[test]
    fn is_ready_returns_false_for_unknown_session() {
        let manager = SessionManager::new();
        assert!(!manager.is_ready("unknown"));
    }

    #[test]
    fn get_cwd_returns_none_for_unknown_session() {
        let manager = SessionManager::new();
        assert!(manager.get_cwd("unknown").is_none());
    }

    #[test]
    fn kill_does_not_panic_on_unknown_session() {
        let manager = SessionManager::new();
        manager.kill("unknown");
    }
}
