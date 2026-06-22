use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Represents the state of an agent session.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub enum AgentSessionState {
    #[default]
    Running,
    Exited,
    Completed,
}

/// Represents a single line of output from a pane.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutputLine {
    pub pane_id: String,
    pub line_num: u32,
    pub timestamp: u64,
    pub text: String,
}

/// Buffer state for a single pane.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaneBuffer {
    pane_id: String,
    lines: Vec<OutputLine>,
    line_counter: u32,
    total_bytes: usize,
    created_at: u64,
    last_activity_at: u64,
    agent_type: String,
    /// Whether the PTY process has exited. The buffer history is preserved
    /// so the user can still read past output, but no new data will arrive.
    dead: bool,
    pub session_state: AgentSessionState,
    pub exit_code: Option<i32>,
    pub resume_id: Option<String>,
    pub exit_snapshot: Vec<OutputLine>,
}

/// Information about a pane buffer.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct PaneBufferInfo {
    pub pane_id: String,
    pub agent_type: String,
    pub line_count: usize,
    pub total_lines: u32,
    pub total_bytes: usize,
    pub created_at: u64,
    pub last_activity_at: u64,
    pub dead: bool,
}

/// Metadata about a tracked agent/pane.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct AgentListEntry {
    pub pane_id: String,
    pub agent_type: String,
    pub line_count: usize,
    pub created_at: u64,
    pub last_activity_at: u64,
}

/// Options for filtering output retrieval.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct GetOutputOptions {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub since_line: Option<u32>,
    pub since_time: Option<u64>,
    pub raw: Option<bool>,
}

const MAX_LINES_PER_PANE: usize = 5000;
const MAX_TOTAL_BYTES_PER_PANE: usize = 2_000_000;

/// Errors for the output buffer service.
#[derive(Debug, Error)]
pub enum OutputBufferError {
    #[error("Lock poisoned: {0}")]
    LockPoison(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Serialization error: {0}")]
    Serde(#[from] serde_json::Error),
}

/// Thread-safe output buffer service.
pub struct OutputBuffer {
    buffers: Arc<RwLock<HashMap<String, PaneBuffer>>>,
    event_emitter:
        Arc<parking_lot::Mutex<Option<Arc<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,    exit_snapshots: Arc<RwLock<HashMap<String, Vec<OutputLine>>>>,
}

impl std::fmt::Debug for OutputBuffer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OutputBuffer")
            .field("buffers", &"<RwLock<HashMap>>")
            .field("event_emitter", &"<Option>")
            .finish()
    }
}

impl Clone for OutputBuffer {
    fn clone(&self) -> Self {
        Self {
            buffers: Arc::clone(&self.buffers),
            event_emitter: Arc::clone(&self.event_emitter),
            exit_snapshots: Arc::clone(&self.exit_snapshots),
        }
    }
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputBuffer {
    pub fn new() -> Self {        Self {            buffers: Arc::new(RwLock::new(HashMap::new())),
            event_emitter: Arc::new(parking_lot::Mutex::new(None)),
            exit_snapshots: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        *self.event_emitter.lock() = Some(Arc::new(emitter));
    }

    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        // Clone the Arc<...> callback out of the lock so the lock is not held
        // during the callback. This prevents potential deadlocks if the callback
        // or downstream code attempts to acquire other locks.
        let maybe_emitter = self.event_emitter.lock().clone();
        if let Some(ref emitter) = maybe_emitter {
            emitter(channel, data);
        } else {
            log::debug!("[output-buffer] {} -> {}", channel, data);
        }
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    /// Initialize a pane buffer entry without appending any output lines.
    /// Used when a PTY is first spawned to register the pane without
    /// creating a phantom blank line.
    pub fn init_pane_buffer(
        &self,
        pane_id: &str,
        agent_type: &str,
    ) -> Result<(), OutputBufferError> {
        let mut buffers = self
            .buffers
            .write()
            .map_err(|_| OutputBufferError::LockPoison("buffers".to_string()))?;
        if !buffers.contains_key(pane_id) {
            let now = Self::now();
            buffers.insert(
                pane_id.to_string(),
                PaneBuffer {
                    pane_id: pane_id.to_string(),
                    lines: Vec::new(),
                    line_counter: 0,
                    total_bytes: 0,
                    created_at: now,
                    last_activity_at: now,
                    agent_type: agent_type.to_string(),
                    dead: false,
                    session_state: AgentSessionState::default(),
                    exit_code: None,
                    resume_id: None,
                    exit_snapshot: Vec::new(),
                },
            );
            let pane_id_str = pane_id.to_string();
            let agent_type_str = agent_type.to_string();
            drop(buffers);

            self.emit_event(
                "output-capture:paneRegistered",
                &serde_json::json!({
                    "paneId": pane_id_str,
                    "agentType": agent_type_str,
                }),
            );
        }
        Ok(())
    }

    fn trim_buffer(buf: &mut PaneBuffer) {
        let excess = buf.lines.len().saturating_sub(MAX_LINES_PER_PANE);
        if excess > 0 {
            let removed_bytes: usize = buf.lines.drain(0..excess).map(|l| l.text.len()).sum();
            buf.total_bytes = buf.total_bytes.saturating_sub(removed_bytes);
        }
        // Byte-budget trim: compute how many leading lines to drop in one pass,
        // then drain once. The previous `while … remove(0)` was O(n²) (each
        // remove shifts the whole tail), which burned CPU under high-volume
        // PTY output (e.g. cat of a large file) on every append.
        if buf.total_bytes > MAX_TOTAL_BYTES_PER_PANE {
            let mut drop_count = 0usize;
            let mut projected = buf.total_bytes;
            while drop_count < buf.lines.len()
                && projected > MAX_TOTAL_BYTES_PER_PANE
            {
                projected = projected.saturating_sub(buf.lines[drop_count].text.len());
                drop_count += 1;
            }
            if drop_count > 0 {
                let removed_bytes: usize =
                    buf.lines.drain(0..drop_count).map(|l| l.text.len()).sum();
                buf.total_bytes = buf.total_bytes.saturating_sub(removed_bytes);
            }
        }
    }

    /// Fast byte-scanner that strips ANSI escape sequences in a single pass.
    /// Replaces the previous 5-regex pipeline to avoid CPU cost in the hot
    /// PTY read loop.
    fn strip_ansi(text: &str) -> String {
        let bytes = text.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b == b'\x1b' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                match next {
                    // ESC [  → CSI sequence
                    b'[' => {
                        i += 2;
                        // Skip parameter bytes (0x30–0x3F)
                        while i < bytes.len() && bytes[i] >= 0x30 && bytes[i] <= 0x3F {
                            i += 1;
                        }
                        // Skip intermediate bytes (0x20–0x2F)
                        while i < bytes.len() && bytes[i] >= 0x20 && bytes[i] <= 0x2F {
                            i += 1;
                        }
                        // Skip final byte (0x40–0x7E)
                        if i < bytes.len() {
                            i += 1;
                        }
                        continue;
                    }
                    // ESC ]  → OSC sequence
                    b']' => {
                        i += 2;
                        while i < bytes.len() {
                            if bytes[i] == b'\x07' {
                                i += 1;
                                break;
                            }
                            if bytes[i] == b'\x1b' && i + 1 < bytes.len() && bytes[i + 1] == b'\\' {
                                i += 2;
                                break;
                            }
                            i += 1;
                        }
                        continue;
                    }
                    // ESC ( or ESC )  → charset select
                    b'(' | b')' => {
                        if i + 2 < bytes.len() {
                            i += 3; // skip ESC, (, and the charset code
                        } else {
                            i += 1;
                        }
                        continue;
                    }
                    // Other single-char ESC sequences
                    _ => {
                        // Skip the ESC and the following byte as a simple escape
                        i += 2;
                        continue;
                    }
                }
            }
            out.push(b);
            i += 1;
        }
        String::from_utf8(out).unwrap_or_else(|_| text.replace('\r', ""))
    }

    /// Append output to a pane buffer.
    /// Acquires the write lock once: creates the buffer if needed, then appends.
    pub fn append_output(&self, pane_id: &str, raw_data: &str, agent_type: Option<&str>) {
        let mut buffers = match self.buffers.write() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!(
                    "OutputBuffer: lock poisoned while appending to pane {}",
                    pane_id
                );
                return;
            }
        };

        // Create buffer if it does not yet exist (single lock scope)
        if !buffers.contains_key(pane_id) {
            let now = Self::now();
            buffers.insert(
                pane_id.to_string(),
                PaneBuffer {
                    pane_id: pane_id.to_string(),
                    lines: Vec::new(),
                    line_counter: 0,
                    total_bytes: 0,
                    created_at: now,
                    last_activity_at: now,
                    agent_type: agent_type.unwrap_or("shell").to_string(),
                    dead: false,
                    session_state: AgentSessionState::default(),
                    exit_code: None,
                    resume_id: None,
                    exit_snapshot: Vec::new(),
                },
            );
        }

        let buf = buffers.get_mut(pane_id).expect("just inserted");

        buf.last_activity_at = Self::now();
        if let Some(at) = agent_type {
            if buf.agent_type == "shell" {
                buf.agent_type = at.to_string();
            }
        }

        let stripped = Self::strip_ansi(raw_data);
        let raw_lines: Vec<&str> = stripped.split('\n').collect();

        let mut emitted_lines: Vec<(u32, String)> = Vec::new();

        for raw_line in &raw_lines {
            // Skip empty lines if there's more than one line
            if raw_line.is_empty() && raw_lines.len() > 1 {
                continue;
            }

            buf.line_counter += 1;
            let now = Self::now();
            let line = OutputLine {
                pane_id: pane_id.to_string(),
                line_num: buf.line_counter,
                timestamp: now,
                text: raw_line.to_string(),
            };
            let line_len = raw_line.len();
            buf.lines.push(line);
            buf.total_bytes += line_len;
            emitted_lines.push((buf.line_counter, raw_line.to_string()));
        }

        Self::trim_buffer(buf);
        drop(buffers);

        // Batch lines into a single IPC event instead of emitting one
        // event per line.  For a large output burst this can reduce IPC
        // traffic by an order of magnitude.  Each line still carries its
        // own lineNum and timestamp so ordering is preserved.
        if !emitted_lines.is_empty() {
            let batch: Vec<serde_json::Value> = emitted_lines
                .into_iter()
                .map(|(line_num, text)| {
                    serde_json::json!({
                        "lineNum": line_num,
                        "text": text,
                        "timestamp": Self::now(),
                    })
                })
                .collect();
            self.emit_event(
                "output-capture:batch",
                &serde_json::json!({
                    "paneId": pane_id,
                    "lines": batch,
                }),
            );
        }
    }

    /// Get output lines from a pane buffer.
    pub fn get_output(&self, pane_id: &str, options: Option<&GetOutputOptions>) -> Vec<OutputLine> {
        let buffers = match self.buffers.read() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("OutputBuffer: lock poisoned while reading pane {}", pane_id);
                return Vec::new();
            }
        };
        let buf = match buffers.get(pane_id) {
            Some(b) => b,
            None => return Vec::new(),
        };

        let result: Vec<OutputLine> = buf
            .lines
            .iter()
            .filter(|l| {
                if let Some(opts) = options {
                    if let Some(since_line) = opts.since_line {
                        if l.line_num <= since_line {
                            return false;
                        }
                    }
                    if let Some(since_time) = opts.since_time {
                        if l.timestamp <= since_time {
                            return false;
                        }
                    }
                }
                true
            })
            .cloned()
            .collect();
        drop(buffers);

        let mut result = result;
        if let Some(opts) = options {
            if let Some(offset) = opts.offset {
                if offset < result.len() {
                    result = result.split_off(offset);
                } else {
                    result.clear();
                }
            }
            if let Some(limit) = opts.limit {
                result.truncate(limit);
            }
        }

        result
    }

    /// Get a list of all agents (panes) with their metadata.
    pub fn get_agent_list(&self) -> Vec<AgentListEntry> {
        let buffers = match self.buffers.read() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("OutputBuffer: lock poisoned while listing agents");
                return Vec::new();
            }
        };
        buffers
            .values()
            .map(|buf| AgentListEntry {
                pane_id: buf.pane_id.clone(),
                agent_type: buf.agent_type.clone(),
                line_count: buf.lines.len(),
                created_at: buf.created_at,
                last_activity_at: buf.last_activity_at,
            })
            .collect()
    }

    /// Get info for a specific pane buffer.
    pub fn get_pane_buffer_info(&self, pane_id: &str) -> Option<PaneBufferInfo> {
        let buffers = match self.buffers.read() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!(
                    "OutputBuffer: lock poisoned while getting pane info for {}",
                    pane_id
                );
                return None;
            }
        };
        let buf = buffers.get(pane_id)?;
        Some(PaneBufferInfo {
            pane_id: buf.pane_id.clone(),
            agent_type: buf.agent_type.clone(),
            line_count: buf.lines.len(),
            total_lines: buf.line_counter,
            total_bytes: buf.total_bytes,
            created_at: buf.created_at,
            last_activity_at: buf.last_activity_at,
            dead: buf.dead,
        })
    }

    /// Mark a pane as dead (PTY exited) without clearing the buffer history.
    /// Returns true if the pane existed, false otherwise.
    pub fn mark_pane_dead(&self, pane_id: &str) -> bool {
        let mut buffers = match self.buffers.write() {
            Ok(g) => g,
            Err(_) => return false,
        };
        if let Some(buf) = buffers.get_mut(pane_id) {
            buf.dead = true;
            true
        } else {
            false
        }
    }

    /// Remove a pane from the internal map entirely.
    /// Call this when a PTY session exits and you no longer need its buffer.
    /// Returns true if the pane existed, false otherwise.
    pub fn remove_pane(&self, pane_id: &str) -> bool {
        let mut buffers = match self.buffers.write() {
            Ok(g) => g,
            Err(_) => return false,
        };
        buffers.remove(pane_id).is_some()
    }

    /// Remove all panes that are marked as dead.
    /// Returns the number of panes removed.
    pub fn cleanup_dead_panes(&self) -> usize {
        let mut buffers = match self.buffers.write() {
            Ok(g) => g,
            Err(_) => return 0,
        };
        let dead_ids: Vec<String> = buffers
            .iter()
            .filter(|(_, buf)| buf.dead)
            .map(|(id, _)| id.clone())
            .collect();
        let count = dead_ids.len();
        for id in dead_ids {
            buffers.remove(&id);
        }
        count
    }

    /// Clear all lines and reset bytes for a pane buffer.
    pub fn clear_pane_buffer(&self, pane_id: &str) -> bool {
        let mut buffers = match self.buffers.write() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!(
                    "OutputBuffer: lock poisoned while clearing pane {}",
                    pane_id
                );
                return false;
            }
        };
        if let Some(buf) = buffers.get_mut(pane_id) {
            buf.lines.clear();
            buf.total_bytes = 0;
            true
        } else {
            false
        }
    }

    /// Shutdown the service and clear all buffers.
    pub fn shutdown(&self) {
        let mut buffers = match self.buffers.write() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("OutputBuffer: lock poisoned during shutdown");
                return;
            }
        };
        buffers.clear();
    }

    /// Capture the last `n` lines of a pane buffer as an exit snapshot.
    pub fn capture_exit_snapshot(&self, pane_id: &str, n: usize) {
        let buffers = match self.buffers.read() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!(
                    "OutputBuffer: lock poisoned while capturing exit snapshot"
                );
                return;
            }
        };
        if let Some(buf) = buffers.get(pane_id) {
            let snapshot: Vec<OutputLine> =
                buf.lines.iter().rev().take(n).rev().cloned().collect();
            drop(buffers);
            let mut exit_snapshots = match self.exit_snapshots.write() {
                Ok(guard) => guard,
                Err(_) => {
                    log::error!(
                        "OutputBuffer: lock poisoned while storing exit snapshot"
                    );
                    return;
                }
            };
            exit_snapshots.insert(pane_id.to_string(), snapshot);
        }
    }

    /// Get the exit snapshot for a pane.
    pub fn get_exit_snapshot(&self, pane_id: &str) -> Vec<OutputLine> {
        let exit_snapshots = match self.exit_snapshots.read() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!(
                    "OutputBuffer: lock poisoned while getting exit snapshot"
                );
                return Vec::new();
            }
        };
        exit_snapshots.get(pane_id).cloned().unwrap_or_default()
    }

    /// Serialize the exit snapshots to a JSON file.
    pub fn save_to_disk(&self, path: &std::path::Path) -> Result<(), OutputBufferError> {
        let exit_snapshots = self
            .exit_snapshots
            .read()
            .map_err(|_| OutputBufferError::LockPoison("exit_snapshots".to_string()))?;
        let json = serde_json::to_string_pretty(&*exit_snapshots)?;
        drop(exit_snapshots);
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Deserialize exit snapshots from a JSON file and replace the current state.
    pub fn load_from_disk(&self, path: &std::path::Path) -> Result<(), OutputBufferError> {
        let data = std::fs::read_to_string(path)?;
        let deserialized: HashMap<String, Vec<OutputLine>> = serde_json::from_str(&data)?;
        let mut exit_snapshots = self
            .exit_snapshots
            .write()
            .map_err(|_| OutputBufferError::LockPoison("exit_snapshots".to_string()))?;
        *exit_snapshots = deserialized;
        Ok(())
    }

    /// Set the session state on a pane buffer.
    pub fn mark_pane_state(&self, pane_id: &str, state: AgentSessionState) {
        let mut buffers = match self.buffers.write() {
            Ok(g) => g,
            Err(_) => {
                log::error!("OutputBuffer: lock poisoned while marking pane state");
                return;
            }
        };
        if let Some(buf) = buffers.get_mut(pane_id) {
            buf.session_state = state;
        }
    }

    /// Set the resume id on a pane buffer.
    pub fn set_pane_resume_id(&self, pane_id: &str, resume_id: Option<String>) {
        let mut buffers = match self.buffers.write() {
            Ok(g) => g,
            Err(_) => {
                log::error!("OutputBuffer: lock poisoned while setting pane resume id");
                return;
            }
        };
        if let Some(buf) = buffers.get_mut(pane_id) {
            buf.resume_id = resume_id;
        }
    }
}
