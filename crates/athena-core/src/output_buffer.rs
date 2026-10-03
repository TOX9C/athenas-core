#![allow(clippy::type_complexity)]
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Represents the state of an agent session.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub enum AgentSessionState {
    #[default]
    Running,
    Exited,
    Completed,
}

/// Serialize a `Arc<str>` field as a plain string — avoids enabling the
/// whole serde "rc" feature for one field.
mod arc_str_serde {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::sync::Arc;

    pub fn serialize<S: Serializer>(value: &Arc<str>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(value)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Arc<str>, D::Error> {
        Ok(Arc::from(String::deserialize(d)?.as_str()))
    }
}

/// Represents a single line of output from a pane.
///
/// `pane_id` is an `Arc<str>` shared with every line of the pane so the hot
/// append path bumps a refcount instead of allocating a fresh `String` per
/// line. On the wire it is a plain string, unchanged.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct OutputLine {
    #[serde(with = "arc_str_serde")]
    pub pane_id: Arc<str>,
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
        Arc<parking_lot::Mutex<Option<Arc<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
    exit_snapshots: Arc<RwLock<HashMap<String, Vec<OutputLine>>>>,
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
    pub fn new() -> Self {
        Self {
            buffers: Arc::new(RwLock::new(HashMap::new())),
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
            // Terminal output can contain credentials or private source text.
            log::debug!("[output-buffer] event emitted on channel {channel}");
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
        let mut buffers = self.buffers.write();
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
            while drop_count < buf.lines.len() && projected > MAX_TOTAL_BYTES_PER_PANE {
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
                        let seq_start = i;
                        i += 2;
                        // Skip parameter bytes (0x30–0x3F)
                        while i < bytes.len() && bytes[i] >= 0x30 && bytes[i] <= 0x3F {
                            i += 1;
                        }
                        // Skip intermediate bytes (0x20–0x2F)
                        while i < bytes.len() && bytes[i] >= 0x20 && bytes[i] <= 0x2F {
                            i += 1;
                        }
                        // Skip final byte (0x40–0x7E). A byte outside this range
                        // means a truncated/malformed sequence — emit the scanned
                        // bytes verbatim instead of eating real text (e.g. a
                        // UTF-8 lead byte).
                        if i < bytes.len() && (0x40..=0x7E).contains(&bytes[i]) {
                            i += 1;
                        } else {
                            out.extend_from_slice(&bytes[seq_start..i]);
                        }
                        continue;
                    }
                    // ESC ]  → OSC sequence; ESC _ → APC (e.g. Kitty
                    // graphics); ESC P → DCS (e.g. sixel). All are
                    // string sequences terminated by BEL or ESC \; their
                    // payloads (multi-KB base64/binary) must never reach
                    // the text buffer.
                    b']' | b'_' | b'P' => {
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
        // Lossy: a malformed/truncated escape (e.g. a multibyte UTF-8 char
        // split across a chunk boundary) previously discarded ALL stripping
        // for the chunk by falling back to the raw text. Dropping only the
        // invalid byte keeps the rest of the chunk stripped.
        String::from_utf8_lossy(&out).into_owned()
    }

    /// If `text` ends inside an unterminated string escape (OSC `ESC ]`,
    /// APC `ESC _`, DCS `ESC P`, or a trailing lone `ESC`), returns the byte
    /// offset where that sequence begins. The caller carries the suffix into
    /// the next chunk so a payload split across PTY reads is stripped whole
    /// instead of leaking its tail into the text buffer.
    fn trailing_string_seq_start(text: &str) -> Option<usize> {
        let bytes = text.as_bytes();
        let esc = bytes.iter().rposition(|&b| b == b'\x1b')?;
        if esc + 1 == bytes.len() {
            // Lone trailing ESC: sequence head arrives next chunk.
            return Some(esc);
        }
        match bytes[esc + 1] {
            b']' | b'_' | b'P' => {
                // Unterminated if neither BEL nor ESC \ follows.
                let tail = &bytes[esc + 2..];
                let terminated = tail.contains(&b'\x07')
                    || tail.windows(2).any(|w| w == b"\x1b\\");
                if terminated { None } else { Some(esc) }
            }
            _ => None,
        }
    }

    /// Append output to a pane buffer.
    /// Acquires the write lock once: creates the buffer if needed, then appends.
    pub fn append_output(&self, pane_id: &str, raw_data: &str, agent_type: Option<&str>) {
        let mut esc_tail = Vec::new();
        self.append_output_carried(pane_id, raw_data, agent_type, &mut esc_tail);
    }

    /// `append_output` with a carry buffer for escape sequences split across
    /// PTY read chunks (Kitty graphics APC frames routinely exceed one 16 KiB
    /// read). `esc_tail` must be per-session and reused across calls.
    pub fn append_output_carried(
        &self,
        pane_id: &str,
        raw_data: &str,
        agent_type: Option<&str>,
        esc_tail: &mut Vec<u8>,
    ) {
        // Prepend a sequence fragment carried from the previous chunk.
        let owned;
        let text: &str = if esc_tail.is_empty() {
            raw_data
        } else {
            let mut merged =
                String::with_capacity(esc_tail.len() + raw_data.len());
            // The carry bytes were already lossy-decoded once; keep them
            // verbatim so the rejoined sequence strips identically.
            merged.push_str(&String::from_utf8_lossy(esc_tail));
            merged.push_str(raw_data);
            esc_tail.clear();
            owned = merged;
            &owned
        };
        // Hold back a trailing unterminated string sequence for next chunk.
        let strippable = match Self::trailing_string_seq_start(text) {
            Some(pos) => {
                esc_tail.extend_from_slice(&text.as_bytes()[pos..]);
                &text[..pos]
            }
            None => text,
        };
        // Strip ANSI outside the lock — this is the expensive byte-scanner.
        let stripped = Self::strip_ansi(strippable);
        let raw_lines: Vec<&str> = stripped.split('\n').collect();

        // One clock read and one pane-id Arc per batch: the previous version
        // paid 3× Self::now() and a fresh pane_id String per line.
        let now = Self::now();
        let pane_id: Arc<str> = Arc::from(pane_id);

        let mut buffers = self.buffers.write();

        // Create buffer if it does not yet exist (single lock scope)
        if !buffers.contains_key(pane_id.as_ref()) {
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

        let buf = buffers.get_mut(pane_id.as_ref()).expect("just inserted");

        buf.last_activity_at = now;
        if let Some(at) = agent_type {
            if buf.agent_type == "shell" {
                buf.agent_type = at.to_string();
            }
        }

        // Batch lines into a single IPC event (vs one event per line). The
        // batch is collected as lines are pushed — BEFORE trim_buffer — so a
        // burst that overflows the cap still emits every pushed line (trim
        // drains from the front and would otherwise drop just-pushed lines
        // from the event, leaving a silent gap).
        let mut batch = Vec::new();

        let last_idx = raw_lines.len().saturating_sub(1);
        for (i, raw_line) in raw_lines.iter().enumerate() {
            // Only strip the spurious trailing empty string produced by
            // `split('\n')` when the data ends with '\n' (last element).
            // Interior empty lines (paragraph breaks) MUST be preserved.
            if i == last_idx && raw_line.is_empty() && raw_lines.len() > 1 {
                continue;
            }

            buf.line_counter += 1;
            let line = OutputLine {
                pane_id: Arc::clone(&pane_id),
                line_num: buf.line_counter,
                timestamp: now,
                text: raw_line.to_string(),
            };
            batch.push(serde_json::json!({
                "lineNum": line.line_num,
                "text": raw_line,
                "timestamp": line.timestamp,
            }));
            buf.total_bytes += raw_line.len();
            buf.lines.push(line);
        }

        Self::trim_buffer(buf);

        drop(buffers);

        if !batch.is_empty() {
            self.emit_event(
                "output-capture:batch",
                &serde_json::json!({
                    "paneId": pane_id.as_ref(),
                    "lines": batch,
                }),
            );
        }
    }

    /// All pane ids with an output buffer. Used by the synchronous
    /// best-effort resume capture on `RunEvent::Exit`, where async session
    /// listing is unavailable (the read loops may already be dead).
    pub fn all_pane_ids(&self) -> Vec<String> {
        self.buffers.read().keys().cloned().collect()
    }

    /// Get output lines from a pane buffer.
    ///
    /// When no `since_line`/`since_time` filters are set, offset/limit are
    /// applied as a slice so only the requested window is cloned instead of
    /// the whole (up to 5000-line) buffer. Tail callers should prefer
    /// [`Self::get_output_tail`].
    pub fn get_output(&self, pane_id: &str, options: Option<&GetOutputOptions>) -> Vec<OutputLine> {
        let buffers = self.buffers.read();
        let buf = match buffers.get(pane_id) {
            Some(b) => b,
            None => return Vec::new(),
        };

        let (offset, limit, since_line, since_time) = match options {
            None => (0, usize::MAX, None, None),
            Some(opts) => (
                opts.offset.unwrap_or(0),
                opts.limit.unwrap_or(usize::MAX),
                opts.since_line,
                opts.since_time,
            ),
        };

        if since_line.is_none() && since_time.is_none() {
            let start = offset.min(buf.lines.len());
            let end = start.saturating_add(limit).min(buf.lines.len());
            return buf.lines[start..end].to_vec();
        }

        let mut result: Vec<OutputLine> = buf
            .lines
            .iter()
            .filter(|l| {
                if let Some(since_line) = since_line {
                    if l.line_num <= since_line {
                        return false;
                    }
                }
                if let Some(since_time) = since_time {
                    if l.timestamp <= since_time {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();
        drop(buffers);

        let start = offset.min(result.len());
        result.drain(..start);
        result.truncate(limit);

        result
    }

    /// Read only the newest `limit` lines without cloning the older retained
    /// history. This is the hot path for heartbeat/context previews, where a
    /// full-buffer clone turns a small tail request into O(buffer size) work.
    pub fn get_output_tail(&self, pane_id: &str, limit: usize) -> Vec<OutputLine> {
        if limit == 0 {
            return Vec::new();
        }
        let buffers = self.buffers.read();
        let Some(buf) = buffers.get(pane_id) else {
            return Vec::new();
        };
        buf.lines.iter().rev().take(limit).rev().cloned().collect()
    }

    /// Get a list of all agents (panes) with their metadata.
    pub fn get_agent_list(&self) -> Vec<AgentListEntry> {
        let buffers = self.buffers.read();
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
        let buffers = self.buffers.read();
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
        let mut buffers = self.buffers.write();
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
        let mut buffers = self.buffers.write();
        buffers.remove(pane_id).is_some()
    }

    /// Remove all panes that are marked as dead.
    /// Returns the number of panes removed.
    pub fn cleanup_dead_panes(&self) -> usize {
        let mut buffers = self.buffers.write();
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
        let mut buffers = self.buffers.write();
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
        let mut buffers = self.buffers.write();
        buffers.clear();
    }

    /// Capture the last `n` lines of a pane buffer as an exit snapshot.
    pub fn capture_exit_snapshot(&self, pane_id: &str, n: usize) {
        let buffers = self.buffers.read();
        if let Some(buf) = buffers.get(pane_id) {
            let snapshot: Vec<OutputLine> = buf.lines.iter().rev().take(n).rev().cloned().collect();
            drop(buffers);
            let mut exit_snapshots = self.exit_snapshots.write();
            exit_snapshots.insert(pane_id.to_string(), snapshot);
        }
    }

    /// Get the exit snapshot for a pane.
    pub fn get_exit_snapshot(&self, pane_id: &str) -> Vec<OutputLine> {
        let exit_snapshots = self.exit_snapshots.read();
        exit_snapshots.get(pane_id).cloned().unwrap_or_default()
    }

    /// Serialize the exit snapshots to a JSON file.
    pub fn save_to_disk(&self, path: &std::path::Path) -> Result<(), OutputBufferError> {
        let exit_snapshots = self.exit_snapshots.read();
        let json = serde_json::to_string_pretty(&*exit_snapshots)?;
        drop(exit_snapshots);
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Deserialize exit snapshots from a JSON file and replace the current state.
    pub fn load_from_disk(&self, path: &std::path::Path) -> Result<(), OutputBufferError> {
        let data = std::fs::read_to_string(path)?;
        let deserialized: HashMap<String, Vec<OutputLine>> = serde_json::from_str(&data)?;
        let mut exit_snapshots = self.exit_snapshots.write();
        *exit_snapshots = deserialized;
        Ok(())
    }

    /// Set the session state on a pane buffer.
    pub fn mark_pane_state(&self, pane_id: &str, state: AgentSessionState) {
        let mut buffers = self.buffers.write();
        if let Some(buf) = buffers.get_mut(pane_id) {
            buf.session_state = state;
        }
    }

    /// Set the resume id on a pane buffer.
    pub fn set_pane_resume_id(&self, pane_id: &str, resume_id: Option<String>) {
        let mut buffers = self.buffers.write();
        if let Some(buf) = buffers.get_mut(pane_id) {
            buf.resume_id = resume_id;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glued(ob: &OutputBuffer, pane: &str) -> String {
        ob.get_output(pane, None)
            .iter()
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn strips_apc_kitty_graphics_payload() {
        let ob = OutputBuffer::new();
        let apc = format!("before\x1b_Ga=T,f=100;{}\x1b\\after", "A".repeat(8192));
        ob.append_output("p", &apc, None);
        let text = glued(&ob, "p");
        assert_eq!(text, "beforeafter");
    }

    #[test]
    fn strips_dcs_payload() {
        let ob = OutputBuffer::new();
        ob.append_output("p", "a\x1bPq#1;2;sixelflood\x1b\\b", None);
        assert_eq!(glued(&ob, "p"), "ab");
    }

    #[test]
    fn split_apc_across_chunks_is_carried_and_stripped() {
        let ob = OutputBuffer::new();
        let mut tail = Vec::new();
        let big = "B".repeat(40_000);
        let full = format!("start\x1b_Ga=T,f=100;{}\x1b\\end", big);
        // Split mid-payload, as the 16 KiB PTY read would.
        let cut = 20 + 10_000;
        ob.append_output_carried("p", &full[..cut], None, &mut tail);
        assert!(!tail.is_empty(), "tail must hold the unterminated APC");
        let mid = glued(&ob, "p");
        assert_eq!(mid, "start");
        ob.append_output_carried("p", &full[cut..], None, &mut tail);
        assert!(tail.is_empty());
        let text = glued(&ob, "p");
        assert!(
            !text.contains('B'),
            "payload leaked into text buffer: {:?}",
            &text[..text.len().min(120)]
        );
        assert!(text.contains("start") && text.contains("end"));
    }

    #[test]
    fn lone_trailing_esc_is_carried() {
        let ob = OutputBuffer::new();
        let mut tail = Vec::new();
        ob.append_output_carried("p", "abc\x1b", None, &mut tail);
        assert_eq!(tail, b"\x1b");
        ob.append_output_carried("p", "]8;;http://x\x07link", None, &mut tail);
        let text = glued(&ob, "p");
        assert_eq!(text, "abclink");
    }
}
