//! Shell integration module — ported from electron/services/shellIntegration.ts
//!
//! Provides OSC 633 parsing, command tracking, and shell integration scripts
//! for zsh, bash, and fish.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const OSC_PREFIX: &str = "\x1b]633;";
const BEL: char = '\x07';
const ST: &str = "\x1b\\";

// ---------------------------------------------------------------------------
// ShellIntegrationSequence
// ---------------------------------------------------------------------------

/// A parsed shell integration sequence emitted by the terminal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShellIntegrationSequence {
    Prompt { data: Option<String> },
    Command { data: String },
    CommandStart,
    CommandExecuted,
    CommandFinished { exit_code: i32 },
    Cwd { data: String },
    Property { key: String, value: String },
}

// ---------------------------------------------------------------------------
// ParsedSequence
// ---------------------------------------------------------------------------

/// A parsed sequence together with the raw byte length it occupied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParsedSequence {
    pub sequence: ShellIntegrationSequence,
    pub raw_length: usize,
}

// ---------------------------------------------------------------------------
// Osc633Parser
// ---------------------------------------------------------------------------

/// Incremental parser for OSC 633 escape sequences.
pub struct Osc633Parser {
    buffer: String,
}

impl Osc633Parser {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    /// Feed raw terminal data and return all complete parsed sequences.
    pub fn feed(&mut self, data: &str) -> Vec<ParsedSequence> {
        self.buffer.push_str(data);
        let mut results: Vec<ParsedSequence> = Vec::new();

        while !self.buffer.is_empty() {
            let osc_start = match self.buffer.find(OSC_PREFIX) {
                Some(pos) => pos,
                None => {
                    // Prevent unbounded buffer growth without splitting an incomplete OSC
                    // sequence in half. Search backwards from the keep point for the
                    // last complete OSC boundary (BEL or ST terminator) so that the
                    // parser state stays valid.
                    if self.buffer.len() > 10_000 {
                        let mut keep = self.buffer.len().saturating_sub(4096);
                        // Walk backward to find the last BEL or ST before keep
                        let tail = &self.buffer[..keep];
                        let last_bel = tail.rfind(BEL);
                        let last_st = tail.rfind(ST);
                        if let Some(pos) = last_bel.max(last_st) {
                            // Advance to just past the terminator
                            keep = pos
                                + if last_bel >= last_st {
                                    BEL.len_utf8()
                                } else {
                                    ST.len()
                                };
                        }
                        // Clamp to prevent slicing past the end of the buffer
                        keep = keep.min(self.buffer.len());
                        self.buffer = self.buffer[keep..].to_string();
                    }
                    break;
                }
            };

            // Discard anything before the OSC prefix
            if osc_start > 0 {
                self.buffer = self.buffer[osc_start..].to_string();
            }

            let payload_start = OSC_PREFIX.len();

            let bel_idx = self.buffer[payload_start..]
                .find(BEL)
                .map(|i| payload_start + i);
            let st_idx = self.buffer[payload_start..]
                .find(ST)
                .map(|i| payload_start + i);

            let (terminator_idx, terminator_len) = match (bel_idx, st_idx) {
                (Some(bi), Some(si)) if bi < si => (bi, 1),
                (Some(_), Some(si)) => (si, 2),
                (Some(bi), None) => (bi, 1),
                (None, Some(si)) => (si, 2),
                (None, None) => {
                    // No terminator found yet
                    if self.buffer.len() > 100_000 {
                        // Try to recover by jumping to the next ESC
                        if let Some(next_esc) = self.buffer[1..].find('\x1b') {
                            self.buffer = self.buffer[next_esc + 1..].to_string();
                            continue;
                        }
                        self.buffer.clear();
                    }
                    break;
                }
            };

            let payload = self.buffer[payload_start..terminator_idx].to_string();
            let raw_length = terminator_idx + terminator_len;
            self.buffer = self.buffer[raw_length..].to_string();

            if let Some(seq) = parse_payload(&payload) {
                results.push(ParsedSequence {
                    sequence: seq,
                    raw_length,
                });
            }
        }

        results
    }

    /// Reset the parser state.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

impl Default for Osc633Parser {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function: parse a single chunk of data and return all sequences.
pub fn parse_osc633(data: &str) -> Vec<ParsedSequence> {
    let mut parser = Osc633Parser::new();
    parser.feed(data)
}

// ---------------------------------------------------------------------------
// parsePayload
// ---------------------------------------------------------------------------

fn parse_payload(payload: &str) -> Option<ShellIntegrationSequence> {
    let (command, rest) = match payload.find(';') {
        Some(idx) => (&payload[..idx], &payload[idx + 1..]),
        None => (payload, ""),
    };

    match command {
        "A" => Some(ShellIntegrationSequence::Prompt { data: None }),
        "B" => Some(ShellIntegrationSequence::Command {
            data: rest.to_string(),
        }),
        "C" => Some(ShellIntegrationSequence::CommandStart),
        "D" => {
            let code = if rest.is_empty() {
                0
            } else {
                rest.parse::<i32>().unwrap_or(0)
            };
            Some(ShellIntegrationSequence::CommandFinished { exit_code: code })
        }
        "E" => Some(ShellIntegrationSequence::CommandExecuted),
        "P" => Some(ShellIntegrationSequence::Cwd {
            data: rest.to_string(),
        }),
        "Is" => Some(ShellIntegrationSequence::Property {
            key: "icon".to_string(),
            value: rest.to_string(),
        }),
        "Set" | "S" => {
            let (key, value) = match rest.find('=') {
                Some(eq) => (rest[..eq].to_string(), rest[eq + 1..].to_string()),
                None => (rest.to_string(), String::new()),
            };
            Some(ShellIntegrationSequence::Property { key, value })
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// stripOsc633
// ---------------------------------------------------------------------------

/// Strip all OSC 633 sequences from a string, returning only the visible text.
pub fn strip_osc633(data: &str) -> String {
    let mut result = String::new();
    let mut pos = 0;

    while pos < data.len() {
        let osc_start = match data[pos..].find(OSC_PREFIX) {
            Some(i) => pos + i,
            None => {
                result.push_str(&data[pos..]);
                break;
            }
        };

        result.push_str(&data[pos..osc_start]);
        let payload_start = osc_start + OSC_PREFIX.len();

        let bel_idx = data[payload_start..]
            .find(BEL)
            .map(|i| payload_start + i + BEL.len_utf8());
        let st_idx = data[payload_start..]
            .find(ST)
            .map(|i| payload_start + i + ST.len());

        match (bel_idx, st_idx) {
            (Some(bi), Some(si)) if bi < si => {
                pos = bi;
            }
            (Some(_bi), Some(si)) => {
                pos = si;
            }
            (Some(bi), None) => {
                pos = bi;
            }
            (None, Some(si)) => {
                pos = si;
            }
            (None, None) => {
                result.push_str(&data[osc_start..]);
                break;
            }
        }
    }

    result
}

// ---------------------------------------------------------------------------
// CommandTracker
// ---------------------------------------------------------------------------

/// Tracks the active command state for a pane.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandTracker {
    pub active_command: Option<String>,
    pub active_start_time: Option<u64>,
    pub active_start_notified: bool,
    pub pending_command_text: Option<String>,
    pub current_cwd: Option<String>,
    pub last_exit_code: Option<i32>,
}

impl Default for CommandTracker {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandTracker {
    pub fn new() -> Self {
        Self {
            active_command: None,
            active_start_time: None,
            active_start_notified: false,
            pending_command_text: None,
            current_cwd: None,
            last_exit_code: None,
        }
    }
}

/// Create a new command tracker (mirrors the TS `createCommandTracker`).
pub fn create_command_tracker() -> CommandTracker {
    CommandTracker::new()
}

// ---------------------------------------------------------------------------
// ShellIntegrationEvent
// ---------------------------------------------------------------------------

/// An event emitted by the shell integration tracker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ShellIntegrationEvent {
    Prompt {
        pane_id: String,
        timestamp: u64,
    },
    CommandStart {
        pane_id: String,
        command: String,
        cwd: Option<String>,
        timestamp: u64,
    },
    CommandExecuted {
        pane_id: String,
        command: String,
        cwd: Option<String>,
        timestamp: u64,
    },
    CommandFinished {
        pane_id: String,
        exit_code: i32,
        command: String,
        cwd: Option<String>,
        timestamp: u64,
        duration: Option<u64>,
    },
    Cwd {
        pane_id: String,
        cwd: String,
        timestamp: u64,
    },
    Property {
        pane_id: String,
        key: String,
        value: String,
        timestamp: u64,
    },
}

// ---------------------------------------------------------------------------
// processSequences
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Process parsed sequences through a command tracker, returning events.
pub fn process_sequences(
    tracker: &mut CommandTracker,
    sequences: &[ParsedSequence],
    pane_id: &str,
) -> Vec<ShellIntegrationEvent> {
    let mut events: Vec<ShellIntegrationEvent> = Vec::new();

    for ParsedSequence { sequence, .. } in sequences {
        match sequence {
            ShellIntegrationSequence::Prompt { .. } => {
                tracker.active_command = None;
                tracker.active_start_time = None;
                tracker.active_start_notified = false;
                tracker.pending_command_text = None;
                events.push(ShellIntegrationEvent::Prompt {
                    pane_id: pane_id.to_string(),
                    timestamp: now_ms(),
                });
            }
            ShellIntegrationSequence::Command { data } => {
                tracker.pending_command_text = Some(data.clone());
            }
            ShellIntegrationSequence::CommandStart => {
                tracker.active_command =
                    Some(tracker.pending_command_text.take().unwrap_or_default());
                let start = now_ms();
                tracker.active_start_time = Some(start);
                tracker.active_start_notified = true;
                events.push(ShellIntegrationEvent::CommandStart {
                    pane_id: pane_id.to_string(),
                    command: tracker.active_command.clone().unwrap_or_default(),
                    cwd: tracker.current_cwd.clone(),
                    timestamp: start,
                });
            }
            ShellIntegrationSequence::CommandExecuted => {
                if tracker.active_command.is_some() {
                    events.push(ShellIntegrationEvent::CommandExecuted {
                        pane_id: pane_id.to_string(),
                        command: tracker.active_command.clone().unwrap_or_default(),
                        cwd: tracker.current_cwd.clone(),
                        timestamp: now_ms(),
                    });
                }
            }
            ShellIntegrationSequence::CommandFinished { exit_code } => {
                tracker.last_exit_code = Some(*exit_code);
                let duration = tracker
                    .active_start_time
                    .map(|start| now_ms().saturating_sub(start));
                events.push(ShellIntegrationEvent::CommandFinished {
                    pane_id: pane_id.to_string(),
                    exit_code: *exit_code,
                    command: tracker.active_command.clone().unwrap_or_default(),
                    cwd: tracker.current_cwd.clone(),
                    timestamp: now_ms(),
                    duration,
                });
                tracker.active_command = None;
                tracker.active_start_time = None;
                tracker.active_start_notified = false;
                tracker.pending_command_text = None;
            }
            ShellIntegrationSequence::Cwd { data } => {
                tracker.current_cwd = Some(data.clone());
                events.push(ShellIntegrationEvent::Cwd {
                    pane_id: pane_id.to_string(),
                    cwd: data.clone(),
                    timestamp: now_ms(),
                });
            }
            ShellIntegrationSequence::Property { key, value } => {
                events.push(ShellIntegrationEvent::Property {
                    pane_id: pane_id.to_string(),
                    key: key.clone(),
                    value: value.clone(),
                    timestamp: now_ms(),
                });
            }
        }
    }

    events
}

// ---------------------------------------------------------------------------
// Shell integration scripts
// ---------------------------------------------------------------------------

/// Return the shell integration script for the given shell.
pub fn get_shell_integration_script(shell: &str) -> String {
    let base = shell.rsplit('/').next().unwrap_or("zsh");

    match base {
        "zsh" => get_zsh_integration(),
        "bash" => get_bash_integration(),
        "fish" => get_fish_integration(),
        _ => get_zsh_integration(),
    }
}

fn get_zsh_integration() -> String {
    [
        "__athena_si_enabled=1",
        "",
        "__athena_osc633() { printf \"\\e]633;%s\\a\" \"$1\"; }",
        "",
        "__athena_precmd() {",
        "  local __athena_exit=$?",
        "  if [[ -n $__athena_si_last_cmd ]]; then",
        "    __athena_osc633 \"D;$__athena_exit\"",
        "    __athena_si_last_cmd=\"\"",
        "  fi",
        "  __athena_osc633 A",
        "  __athena_osc633 \"P;$PWD\"",
        "}",
        "",
        "__athena_preexec() {",
        "  __athena_si_last_cmd=\"$3\"",
        "  __athena_osc633 \"B;$3\"",
        "  __athena_osc633 C",
        "  __athena_osc633 E",
        "}",
        "",
        "autoload -Uz add-zsh-hook 2>/dev/null",
        "add-zsh-hook precmd __athena_precmd 2>/dev/null",
        "add-zsh-hook preexec __athena_preexec 2>/dev/null",
        "",
        "__athena_osc633 \"Set=shellIntegration=zsh\"",
    ]
    .join("\n")
}

fn get_bash_integration() -> String {
    [
        "__athena_si_enabled=1",
        "",
        "__athena_osc633() { printf \"\\e]633;%s\\a\" \"$1\"; }",
        "",
        "__athena_prompt_command() {",
        "  local __athena_exit=\"$?\"",
        "  if [[ -n $__athena_si_last_cmd ]]; then",
        "    __athena_osc633 \"D;$__athena_exit\"",
        "    __athena_si_last_cmd=\"\"",
        "  fi",
        "  __athena_osc633 A",
        "  __athena_osc633 \"P;$PWD\"",
        "}",
        "",
        "__athena_debug_trap() {",
        "  if [[ -n $__athena_si_last_cmd ]]; then",
        "    return",
        "  fi",
        "  local __athena_cmd=\"$BASH_COMMAND\"",
        "  if [[ \"$__athena_cmd\" != \"__athena_prompt_command\" && \"$__athena_cmd\" != *\"__athena_osc633\"* ]]; then",
        "    __athena_si_last_cmd=\"$__athena_cmd\"",
        "    __athena_osc633 \"B;$__athena_cmd\"",
        "    __athena_osc633 C",
        "    __athena_osc633 E",
        "  fi",
        "}",
        "",
        "trap \"__athena_debug_trap\" DEBUG",
        "PROMPT_COMMAND=\"__athena_prompt_command; $PROMPT_COMMAND\"",
        "",
        "__athena_osc633 \"Set=shellIntegration=bash\"",
    ]
    .join("\n")
}

fn get_fish_integration() -> String {
    [
        "set -g __athena_si_enabled 1",
        "",
        "function __athena_osc633 -d \"Emit OSC 633 sequence\"",
        "  printf \"\\e]633;%s\\a\" $argv",
        "end",
        "",
        "function __athena_prompt_start --on-event fish_prompt",
        "  __athena_osc633 A",
        "  __athena_osc633 \"P;(pwd)\"",
        "end",
        "",
        "function __athena_preexec --on-event fish_preexec",
        "  __athena_osc633 \"B;$argv\"",
        "  __athena_osc633 C",
        "  __athena_osc633 E",
        "end",
        "",
        "function __athena_postexec --on-event fish_postexec -a __athena_exit",
        "  __athena_osc633 \"D;$__athena_exit\"",
        "end",
        "",
        "__athena_osc633 \"Set=shellIntegration=fish\"",
    ]
    .join("\n")
}

/// Check whether the given shell is compatible with shell integration.
pub fn is_shell_integration_compatible(shell: &str) -> bool {
    if cfg!(windows) {
        return false;
    }
    let base = shell.rsplit('/').next().unwrap_or("");
    matches!(base, "zsh" | "bash" | "fish" | "sh")
}

/// Build environment variables for shell integration.
pub fn build_shell_integration_env(_shell: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    map.insert("ATHENA_SHELL_INTEGRATION".to_string(), "1".to_string());
    map.insert("ATHENA_TERM".to_string(), "athena-core".to_string());
    map
}

#[cfg(test)]
mod strip_osc633_tests {
    use super::*;

    #[test]
    fn strip_osc633_removes_bel_terminator() {
        let input = "\x1b]633;set-mark\x07hello";
        let output = strip_osc633(input);
        assert_eq!(output, "hello");
        assert!(!output.contains('\x07'));
    }

    #[test]
    fn strip_osc633_removes_st_terminator() {
        let input = "\x1b]633;set-mark\x1b\\hello";
        let output = strip_osc633(input);
        assert_eq!(output, "hello");
        assert!(!output.contains("\x1b\\"));
    }

    #[test]
    fn strip_osc633_preserves_non_osc_text() {
        let input = "regular text";
        let output = strip_osc633(input);
        assert_eq!(output, "regular text");
    }
}
