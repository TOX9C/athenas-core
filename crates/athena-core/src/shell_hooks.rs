//! Shell hooks module — ported from electron/services/shellHooks.ts
//!
//! Manages the lifecycle hooks for PTY processes:
//! - `on_pty_spawn`: register a pane when a new PTY is created
//! - `on_pty_data`: capture output data from a running PTY
//! - `on_pty_exit`: clean up when a PTY exits
//! - `capture_stderr`: capture stderr output from a child pane
//!
//! These hooks delegate to the `OutputBuffer` for the actual storage.

use crate::output_buffer::OutputBuffer;
use std::sync::{Arc, Mutex};

/// Shell hooks service — bridges PTY lifecycle events to the output buffer.
#[derive(Debug, Clone)]
pub struct ShellHooks {
    output_buffer: Arc<OutputBuffer>,
    initialized: Arc<Mutex<bool>>,
}

impl ShellHooks {
    pub fn new(output_buffer: Arc<OutputBuffer>) -> Self {
        Self {
            output_buffer,
            initialized: Arc::new(Mutex::new(false)),
        }
    }

    /// Initialize the shell hooks service.
    pub fn init(&self) {
        let mut init = match self.initialized.lock() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("ShellHooks: lock poisoned during init");
                return;
            }
        };
        *init = true;
        log::info!("Shell hooks service initialized");
    }

    /// Shutdown the shell hooks service.
    pub fn shutdown(&self) {
        let mut init = match self.initialized.lock() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("ShellHooks: lock poisoned during shutdown");
                return;
            }
        };
        *init = false;
        log::info!("Shell hooks service shut down");
    }

    /// Called when a new PTY is spawned. Registers the pane with the output buffer
    /// without appending any output lines (avoids phantom blank line).
    pub fn on_pty_spawn(&self, pane_id: &str, agent_type: Option<&str>) {
        let at = agent_type.unwrap_or("shell");
        let _ = self.output_buffer.init_pane_buffer(pane_id, at);
        log::debug!("PTY spawned: pane_id={}, agent_type={}", pane_id, at);
    }

    /// Called when data is received from a running PTY. Appends it to the output buffer.
    pub fn on_pty_data(&self, pane_id: &str, data: &str) {
        self.output_buffer.append_output(pane_id, data, None);
    }

    /// Called when a PTY exits. Marks the pane as dead without clearing
    /// the buffer history so that past output remains accessible.
    pub fn on_pty_exit(&self, pane_id: &str) {
        self.output_buffer.mark_pane_dead(pane_id);
        log::debug!("PTY exited: pane_id={}", pane_id);
    }

    /// Capture stderr output from a child pane.
    pub fn capture_stderr(&self, child_pane_id: &str, data: &str) {
        self.output_buffer.append_output(child_pane_id, data, None);
    }

    /// Check if the service is initialized.
    pub fn is_initialized(&self) -> bool {
        match self.initialized.lock() {
            Ok(guard) => *guard,
            Err(_) => {
                log::error!("ShellHooks: lock poisoned while checking initialized");
                false
            }
        }
    }
}
