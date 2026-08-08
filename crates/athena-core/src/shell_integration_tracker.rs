//! OSC 633 command tracking and event generation.

use super::{ParsedSequence, ShellIntegrationSequence};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

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
