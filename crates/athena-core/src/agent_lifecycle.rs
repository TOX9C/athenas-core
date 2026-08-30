//! Agent lifecycle push protocol (OSC 6337).
//!
//! Any CLI agent (or its notification plugin) can report its own lifecycle in
//! real time by printing an in-band OSC sequence to its PTY:
//!
//! ```text
//! ESC ] 6337 ; {"kind":"complete","agent":"claude"} BEL
//! ```
//!
//! `kind` is `complete`, `request` (needs input / approval / attention), or
//! `error`. The PTY read loop parses these (see [`AgentLifecycleParser`]) and
//! pushes an authoritative status transition through the activity tracker
//! immediately — no heartbeat poll.
//!
//! This mirrors Warp's "agent notification plugin" model: the agent is the
//! source of truth for its own done / needs-input / error state; we only relay
//! the signal. Emitters:
//!
//! - Claude Code: a `Stop` hook (turn complete) and `Notification` hook
//!   (waiting for permission) that print the marker.
//! - Codex: a `[notify]` hook in `~/.codex/config.toml` that prints the marker.
//! - OpenCode: a plugin on `session.idle` + the permission event.
//! - Freebuff / OMP: a native emitter in their own runtimes.
//!
//! The sequence family `6337` is distinct from shell integration's `633` so
//! the two parsers never collide.

use serde::{Deserialize, Serialize};

/// OSC prefix for agent lifecycle notifications.
const OSC_PREFIX: &str = "\x1b]6337;";
const BEL: char = '\x07';
const ST: &str = "\x1b\\";

/// Max bytes of trailing non-OSC text we keep while waiting for a sequence.
const BUFFER_KEEP_BYTES: usize = 4096;

/// Move a byte index back to the nearest valid UTF-8 boundary (terminal output
/// can contain multibyte glyphs such as OMP's box-drawing UI).
fn floor_char_boundary(input: &str, index: usize) -> usize {
    let mut boundary = index.min(input.len());
    while boundary > 0 && !input.is_char_boundary(boundary) {
        boundary -= 1;
    }
    boundary
}

/// The lifecycle states an agent can report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycleKind {
    Complete,
    Request,
    Error,
}

impl AgentLifecycleKind {
    pub fn as_str(self) -> &'static str {
        match self {
            AgentLifecycleKind::Complete => "complete",
            AgentLifecycleKind::Request => "request",
            AgentLifecycleKind::Error => "error",
        }
    }

    /// Parse a wire-format kind, accepting the canonical names plus common
    /// aliases so hand-written emitter snippets stay forgiving.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "complete" | "done" | "completed" | "success" | "finished" => Some(Self::Complete),
            "request" | "needs_input" | "waiting" | "waiting_input" | "attention" => {
                Some(Self::Request)
            }
            "error" | "failed" => Some(Self::Error),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for AgentLifecycleKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Self::parse(&s)
            .ok_or_else(|| serde::de::Error::custom(format!("unknown agent lifecycle kind: {s}")))
    }
}

/// A decoded agent lifecycle notification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLifecycleEvent {
    pub kind: AgentLifecycleKind,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

/// Encode an event into the OSC 6337 wire format (BEL-terminated).
pub fn encode(event: &AgentLifecycleEvent) -> String {
    let json = serde_json::to_string(event).unwrap_or_else(|_| "{}".to_string());
    format!("{OSC_PREFIX}{json}{BEL}")
}

/// Parse the raw payload (text between `;` and the terminator) into an event.
///
/// Accepts either a bare kind word (`complete`) or a JSON object
/// (`{"kind":"complete","agent":"claude"}`).
pub fn parse_payload(payload: &str) -> Option<AgentLifecycleEvent> {
    let trimmed = payload.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(kind) = AgentLifecycleKind::parse(trimmed) {
        return Some(AgentLifecycleEvent {
            kind,
            agent: None,
            session_id: None,
            message: None,
        });
    }
    serde_json::from_str(trimmed).ok()
}

/// Incremental parser for OSC 6337 agent lifecycle sequences. Feed raw PTY
/// bytes; it returns complete events and retains any partial sequence across
/// feeds.
pub struct AgentLifecycleParser {
    buffer: String,
}

impl AgentLifecycleParser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed terminal data and return all complete lifecycle events.
    pub fn feed(&mut self, data: &str) -> Vec<AgentLifecycleEvent> {
        self.buffer.push_str(data);
        let mut events = Vec::new();

        loop {
            let Some(start) = self.buffer.find(OSC_PREFIX) else {
                // No sequence in flight: retain only a bounded tail so a
                // partial prefix at the chunk boundary is not lost, but the
                // buffer cannot grow unboundedly.
                if self.buffer.len() > BUFFER_KEEP_BYTES {
                    let keep = floor_char_boundary(
                        &self.buffer,
                        self.buffer.len().saturating_sub(BUFFER_KEEP_BYTES),
                    );
                    self.buffer = self.buffer[keep..].to_string();
                }
                break;
            };
            if start > 0 {
                self.buffer = self.buffer[start..].to_string();
            }

            let payload_start = OSC_PREFIX.len();
            let bel = self.buffer[payload_start..]
                .find(BEL)
                .map(|i| payload_start + i);
            let st = self.buffer[payload_start..]
                .find(ST)
                .map(|i| payload_start + i);

            let (term_idx, term_len) = match (bel, st) {
                (Some(b), Some(s)) if b < s => (b, BEL.len_utf8()),
                (Some(_), Some(s)) => (s, ST.len()),
                (Some(b), None) => (b, BEL.len_utf8()),
                (None, Some(s)) => (s, ST.len()),
                (None, None) => {
                    // Incomplete sequence: wait for more data. Guard against
                    // pathological growth by dropping to the last ESC.
                    if self.buffer.len() > 8192 {
                        if let Some(next_esc) = self.buffer[1..].rfind('\x1b') {
                            self.buffer = self.buffer[next_esc + 1..].to_string();
                        } else {
                            let keep = floor_char_boundary(
                                &self.buffer,
                                self.buffer.len().saturating_sub(BUFFER_KEEP_BYTES),
                            );
                            self.buffer = self.buffer[keep..].to_string();
                        }
                    }
                    break;
                }
            };

            let payload = self.buffer[payload_start..term_idx].to_string();
            self.buffer = self.buffer[term_idx + term_len..].to_string();
            if let Some(event) = parse_payload(&payload) {
                events.push(event);
            }
        }

        events
    }

    /// Reset parser state.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

impl Default for AgentLifecycleParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: AgentLifecycleKind) -> AgentLifecycleEvent {
        AgentLifecycleEvent {
            kind,
            agent: Some("claude".to_string()),
            session_id: Some("s1".to_string()),
            message: None,
        }
    }

    #[test]
    fn encode_round_trips_through_parser() {
        let event = ev(AgentLifecycleKind::Complete);
        let wire = encode(&event);
        assert!(wire.starts_with(OSC_PREFIX));
        assert!(wire.ends_with(BEL));

        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed(&wire);
        assert_eq!(events, vec![event]);
    }

    #[test]
    fn parse_payload_accepts_bare_words_and_aliases() {
        assert_eq!(
            parse_payload("complete").map(|e| e.kind),
            Some(AgentLifecycleKind::Complete)
        );
        assert_eq!(
            parse_payload("done").map(|e| e.kind),
            Some(AgentLifecycleKind::Complete)
        );
        assert_eq!(
            parse_payload("request").map(|e| e.kind),
            Some(AgentLifecycleKind::Request)
        );
        assert_eq!(
            parse_payload("error").map(|e| e.kind),
            Some(AgentLifecycleKind::Error)
        );
        assert!(parse_payload("").is_none());
        assert!(parse_payload("banana").is_none());
    }

    #[test]
    fn parse_payload_accepts_json_and_alias_kinds() {
        let parsed = parse_payload(r#"{"kind":"done","agent":"codex"}"#).unwrap();
        assert_eq!(parsed.kind, AgentLifecycleKind::Complete);
        assert_eq!(parsed.agent.as_deref(), Some("codex"));
    }

    #[test]
    fn parser_handles_sequence_split_across_feeds() {
        let wire = encode(&ev(AgentLifecycleKind::Error));
        let mut parser = AgentLifecycleParser::new();
        assert!(parser.feed(&wire[..10]).is_empty());
        let events = parser.feed(&wire[10..]);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, AgentLifecycleKind::Error);
    }

    #[test]
    fn parser_handles_st_terminator() {
        let json = r#"{"kind":"request","agent":"claude"}"#;
        let wire = format!("{OSC_PREFIX}{json}{ST}");
        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed(&wire);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, AgentLifecycleKind::Request);
    }

    #[test]
    fn parser_handles_multiple_events_and_surrounding_text() {
        let a = encode(&ev(AgentLifecycleKind::Complete));
        let b = encode(&ev(AgentLifecycleKind::Request));
        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed(&format!("some output\n{a}more\n{b}"));
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].kind, AgentLifecycleKind::Complete);
        assert_eq!(events[1].kind, AgentLifecycleKind::Request);
    }

    #[test]
    fn parser_drops_malformed_json_and_unknown_kinds() {
        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed(&format!(
            "{OSC_PREFIX}{{not json}}\x07{OSC_PREFIX}{{\"kind\":\"huh\"}}\x07"
        ));
        assert!(events.is_empty());
    }

    #[test]
    fn parser_ignores_plain_text_without_sequences() {
        let mut parser = AgentLifecycleParser::new();
        assert!(parser.feed("just some terminal output").is_empty());
        assert!(parser.feed("").is_empty());
    }

    #[test]
    fn reset_clears_partial_state() {
        let wire = encode(&ev(AgentLifecycleKind::Complete));
        let mut parser = AgentLifecycleParser::new();
        parser.feed(&wire[..5]);
        parser.reset();
        assert!(parser.feed("clean").is_empty());
    }

    /// Lock the exact wire format emitted by the Freebuff launcher
    /// (fs.writeSync(1, …) on child exit) and the OMP extension
    /// (process.stdout.write on turn_end / session_shutdown) so a live PTY
    /// capture of either runtime parses into the right lifecycle event.
    #[test]
    fn parses_freebuff_launcher_and_omp_extension_formats() {
        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed("\x1b]6337;{\"kind\":\"complete\",\"agent\":\"freebuff\"}\x07");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, AgentLifecycleKind::Complete);
        assert_eq!(events[0].agent.as_deref(), Some("freebuff"));

        // Non-zero (non-signal) exit → error variant from the launcher.
        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed("\x1b]6337;{\"kind\":\"error\",\"agent\":\"freebuff\"}\x07");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, AgentLifecycleKind::Error);
        assert_eq!(events[0].agent.as_deref(), Some("freebuff"));

        // OMP extension: turn_end → request, session_shutdown → complete.
        let mut parser = AgentLifecycleParser::new();
        let events = parser.feed("\x1b]6337;{\"kind\":\"request\",\"agent\":\"omp\"}\x07");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].kind, AgentLifecycleKind::Request);
        assert_eq!(events[0].agent.as_deref(), Some("omp"));
    }
}
