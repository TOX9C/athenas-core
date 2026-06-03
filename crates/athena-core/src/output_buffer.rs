use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};
use thiserror::Error;

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
}

/// Thread-safe output buffer service.
pub struct OutputBuffer {
    buffers: Arc<RwLock<HashMap<String, PaneBuffer>>>,
    event_emitter:
        Arc<std::sync::Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
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
        }
    }
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputBuffer {
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
            event_emitter: Arc::new(std::sync::Mutex::new(None)),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        if let Ok(mut guard) = self.event_emitter.lock() {
            *guard = Some(Box::new(emitter));
        }
    }

    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        if let Ok(guard) = self.event_emitter.lock() {
            if let Some(ref emitter) = *guard {
                emitter(channel, data);
                return;
            }
        }
        log::debug!("[output-buffer] {} -> {}", channel, data);
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
        while buf.total_bytes > MAX_TOTAL_BYTES_PER_PANE && !buf.lines.is_empty() {
            let removed = buf.lines.remove(0);
            buf.total_bytes = buf.total_bytes.saturating_sub(removed.text.len());
        }
    }

    fn strip_ansi(text: &str) -> String {
        static RE_OSC_BEL: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\x1b\][^\x07]*\x07").unwrap());
        static RE_OSC_ESC: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\x1b\][^\x1b]*\x1b\\").unwrap());
        static RE_CSI: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;?]*[A-Za-z]").unwrap());
        static RE_CHARSET: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\x1b[()][0-9A-B]").unwrap());
        static RE_MODE: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r"\x1b\[\?[0-9]+[hl]").unwrap());

        let result = RE_OSC_BEL.replace_all(text, "");
        let result = RE_OSC_ESC.replace_all(&result, "");
        let result = RE_CSI.replace_all(&result, "");
        let result = RE_CHARSET.replace_all(&result, "");
        let result = RE_MODE.replace_all(&result, "");
        let result = result.replace("\r\n", "\n");
        result.replace('\r', "")
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

        for (line_num, text) in emitted_lines {
            self.emit_event(
                "output-capture:line",
                &serde_json::json!({
                    "paneId": pane_id,
                    "lineNum": line_num,
                    "text": text,
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
}
