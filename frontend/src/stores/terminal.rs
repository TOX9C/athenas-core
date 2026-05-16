use dioxus::prelude::*;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Status of a terminal session.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PtyStatus {
    #[default]
    Idle,
    Running,
    Ready,
    Exited,
    Error,
}

/// A single executed command block within a terminal session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommandBlock {
    pub id: String,
    pub command: String,
    pub output: String,
    pub exit_code: Option<i32>,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub duration: Option<i64>,
    pub collapsed: bool,
}

/// A terminal pseudo-teletype session.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PtySession {
    pub pane_id: String,
    pub pid: Option<u32>,
    pub status: PtyStatus,
    pub blocks: Vec<CommandBlock>,
    pub error_message: Option<String>,
    pub cwd: Option<String>,
    pub last_command: Option<String>,
    pub last_exit_code: Option<i32>,
}

/// Type of shell integration event.
#[derive(Debug, Clone, PartialEq)]
pub enum ShellIntegrationEventType {
    Prompt,
    CommandStart,
    CommandExecuted,
    CommandFinished,
    Cwd,
    Property,
}

/// A shell integration event emitted from the PTY.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellIntegrationEvent {
    pub event_type: ShellIntegrationEventType,
    pub pane_id: String,
    pub timestamp: i64,
    pub command: Option<String>,
    pub exit_code: Option<i32>,
    pub cwd: Option<String>,
    pub duration: Option<i64>,
    pub key: Option<String>,
    pub value: Option<String>,
}

/// Maximum command blocks per session before trimming.
const MAX_BLOCKS_PER_SESSION: usize = 500;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Global terminal store state.
#[derive(Clone, PartialEq, Default)]
pub struct TerminalState {
    pub sessions: Vec<(String, PtySession)>,
}

impl TerminalState {
    pub fn new() -> Self {
        Self {
            sessions: Vec::new(),
        }
    }

    // -- Helpers -----------------------------------------------------------

    fn find_session_index(&self, pane_id: &str) -> Option<usize> {
        self.sessions.iter().position(|(id, _)| id == pane_id)
    }

    fn find_session_mut(&mut self, pane_id: &str) -> Option<&mut PtySession> {
        self.sessions
            .iter_mut()
            .find(|(id, _)| id == pane_id)
            .map(|(_, s)| s)
    }

    fn trim_blocks(blocks: &mut Vec<CommandBlock>) {
        if blocks.len() > MAX_BLOCKS_PER_SESSION {
            let excess = blocks.len() - MAX_BLOCKS_PER_SESSION;
            blocks.drain(0..excess);
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn set_session(&mut self, pane_id: impl Into<String>, session: PtySession) {
        let key = pane_id.into();
        if let Some(idx) = self.find_session_index(&key) {
            self.sessions[idx].1 = session;
        } else {
            self.sessions.push((key.clone(), session));
        }
    }

    pub fn update_session(&mut self, pane_id: &str, updates: PtySessionUpdate) {
        if let Some(session) = self.find_session_mut(pane_id) {
            if let Some(status) = updates.status {
                session.status = status;
            }
            if let Some(error_message) = updates.error_message {
                session.error_message = Some(error_message);
            }
            if let Some(cwd) = updates.cwd {
                session.cwd = Some(cwd);
            }
            if let Some(last_command) = updates.last_command {
                session.last_command = Some(last_command);
            }
            if let Some(last_exit_code) = updates.last_exit_code {
                session.last_exit_code = Some(last_exit_code);
            }
        }
    }

    pub fn remove_session(&mut self, pane_id: &str) {
        self.sessions.retain(|(id, _)| id != pane_id);
    }

    pub fn update_session_status(&mut self, pane_id: &str, status: PtyStatus) {
        if let Some(session) = self.find_session_mut(pane_id) {
            session.status = status;
        }
    }

    pub fn handle_shell_integration_event(&mut self, event: ShellIntegrationEvent) {
        let pane_id = event.pane_id.clone();

        // Ensure session exists.
        if self.find_session_index(&pane_id).is_none() {
            let fresh = PtySession {
                pane_id: pane_id.clone(),
                status: PtyStatus::Idle,
                blocks: Vec::new(),
                ..Default::default()
            };
            self.sessions.push((pane_id.clone(), fresh));
        }

        match event.event_type {
            ShellIntegrationEventType::CommandStart => {
                if let Some(session) = self.find_session_mut(&pane_id) {
                    let new_block = CommandBlock {
                        id: format!("blk-{}", event.timestamp),
                        command: event.command.unwrap_or_default(),
                        output: String::new(),
                        exit_code: None,
                        started_at: event.timestamp,
                        finished_at: None,
                        duration: None,
                        collapsed: false,
                    };
                    session.blocks.push(new_block);
                    Self::trim_blocks(&mut session.blocks);
                    session.status = PtyStatus::Running;
                    if let Some(cwd) = &event.cwd {
                        session.cwd = Some(cwd.clone());
                    }
                }
            }
            ShellIntegrationEventType::CommandFinished => {
                if let Some(session) = self.find_session_mut(&pane_id) {
                    let active_idx = session
                        .blocks
                        .iter()
                        .rposition(|b| b.exit_code.is_none() && b.started_at <= event.timestamp);
                    if let Some(idx) = active_idx {
                        let block = &mut session.blocks[idx];
                        block.exit_code = Some(event.exit_code.unwrap_or(0));
                        block.finished_at = Some(event.timestamp);
                        block.duration = Some(
                            event
                                .duration
                                .unwrap_or_else(|| event.timestamp - block.started_at),
                        );
                    }
                    session.status = PtyStatus::Idle;
                    if let Some(cmd) = &event.command {
                        session.last_command = Some(cmd.clone());
                    }
                    session.last_exit_code = Some(event.exit_code.unwrap_or(0));
                }
            }
            ShellIntegrationEventType::Cwd => {
                if let Some(session) = self.find_session_mut(&pane_id) {
                    if let Some(cwd) = &event.cwd {
                        session.cwd = Some(cwd.clone());
                    }
                }
            }
            ShellIntegrationEventType::Prompt => {
                if let Some(session) = self.find_session_mut(&pane_id) {
                    if session.status != PtyStatus::Idle {
                        session.status = PtyStatus::Idle;
                    }
                }
            }
            ShellIntegrationEventType::CommandExecuted | ShellIntegrationEventType::Property => {
                // No-op for these event types.
            }
        }
    }
}

/// Partial update descriptor for a PtySession.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PtySessionUpdate {
    pub status: Option<PtyStatus>,
    pub error_message: Option<String>,
    pub cwd: Option<String>,
    pub last_command: Option<String>,
    pub last_exit_code: Option<i32>,
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the terminal signal from the Dioxus context.
pub fn use_terminal_store() -> Signal<TerminalState> {
    use_context::<Signal<TerminalState>>()
}

/// Initialize the terminal store as a context provider.
pub fn provide_terminal_store() {
    use_context_provider(|| Signal::new(TerminalState::new()));
}
