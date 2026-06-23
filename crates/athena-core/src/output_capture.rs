//! Output capture module — ported from electron/services/output-capture.ts
//!
//! High-level output capture service that bridges PTY lifecycle events
//! to the output buffer. This is the main entry point for registering
//! panes, appending output, and handling PTY exits.

use crate::output_buffer::{AgentSessionState, OutputBuffer};
use crate::shell_hooks::ShellHooks;
use std::sync::{Arc, Mutex};

/// Output capture service — coordinates output buffering and shell hooks.
#[derive(Debug, Clone)]
pub struct OutputCapture {
    output_buffer: Arc<OutputBuffer>,
    shell_hooks: ShellHooks,
    initialized: Arc<Mutex<bool>>,
}

impl OutputCapture {
    pub fn new(output_buffer: Arc<OutputBuffer>) -> Self {
        let hooks = ShellHooks::new(output_buffer.clone());
        Self {
            output_buffer,
            shell_hooks: hooks,
            initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Initialize the output capture service.
    pub async fn init_output_capture(&self) {
        let mut init = match self.initialized.lock() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("OutputCapture: lock poisoned during init");
                return;
            }
        };
        if *init {
            return;
        }
        *init = true;
        self.shell_hooks.init();
        log::info!("Output capture service initialized");
    }

    /// Shutdown the output capture service.
    pub fn shutdown_output_capture(&self) {
        self.shell_hooks.shutdown();
        self.output_buffer.shutdown();
        let mut init = match self.initialized.lock() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("OutputCapture: lock poisoned during shutdown");
                return;
            }
        };
        *init = false;
        log::info!("Output capture service shut down");
    }

    /// Called when a new PTY is spawned.
    pub fn on_pty_spawn(&self, pane_id: &str, agent_type: Option<&str>) {
        self.shell_hooks.on_pty_spawn(pane_id, agent_type);
        self.output_buffer
            .mark_pane_state(pane_id, AgentSessionState::Running);
    }

    /// Called when data is received from a running PTY.
    pub fn on_pty_data(&self, pane_id: &str, data: &str) {
        self.shell_hooks.on_pty_data(pane_id, data);
    }

    /// Called when a PTY exits.
    pub fn on_pty_exit(&self, pane_id: &str) {
        self.shell_hooks.on_pty_exit(pane_id);
        self.output_buffer.capture_exit_snapshot(pane_id, 20);
        self.output_buffer
            .mark_pane_state(pane_id, AgentSessionState::Exited);
    }

    /// Capture stderr output from a child pane.
    pub fn capture_stderr(&self, child_pane_id: &str, data: &str) {
        self.shell_hooks.capture_stderr(child_pane_id, data);
    }

    /// Register a pane explicitly (mirrors the TS `registerPane` IPC handler).
    pub fn register_pane(&self, pane_id: &str, agent_type: Option<&str>) {
        self.on_pty_spawn(pane_id, agent_type);
    }

    /// Get a reference to the underlying output buffer.
    pub fn output_buffer(&self) -> &OutputBuffer {
        &self.output_buffer
    }

    /// Get a reference to the underlying shell hooks.
    pub fn shell_hooks(&self) -> &ShellHooks {
        &self.shell_hooks
    }

    /// Set the resume id for a pane.
    pub fn set_pane_resume_id(&self, pane_id: &str, resume_id: Option<String>) {
        self.output_buffer.set_pane_resume_id(pane_id, resume_id);
    }
}
