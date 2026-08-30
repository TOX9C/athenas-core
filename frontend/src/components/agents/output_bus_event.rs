//! Events routed through the agent output bus coroutine.

use crate::stores::agent_output::OutputLine;
use crate::stores::agent_status::{AgentProgress, AgentRunStatus};

/// Events that the output bus can receive from the Tauri backend.
///
/// Signal writes are deferred to the coroutine so they happen inside the
/// reactive runtime, avoiding panics when a write lands while a read lock
/// is still held.
#[derive(Debug)]
pub(super) enum OutputBusEvent {
    AgentStatus {
        pane_id: String,
        status: AgentRunStatus,
        message: Option<String>,
        progress: Option<AgentProgress>,
        now: i64,
        /// Raw classified foreground process (`"claude"`, `"vim"`, …).
        /// `None` when the foreground is a shell — mirrors `pty_agent_info`
        /// so the terminal store stays in sync without a frontend `ps` poll.
        fg_process: Option<String>,
        task_title: Option<String>,
        session_id: Option<String>,
        raw_prompt: Option<String>,
        generation: Option<u64>,
    },
    TerminalExit {
        pane_id: String,
        generation: Option<u64>,
    },
    TerminalData {
        session_id: String,
        payload: String,
    },
    AgentConnected {
        pane_id: String,
        now: i64,
    },
    AgentDisconnected {
        pane_id: String,
        now: i64,
    },
    AgentStatusUpdate {
        pane_id: String,
        status: AgentRunStatus,
        message: Option<String>,
        now: i64,
    },
    InputRequested {
        pane_id: String,
        request_id: Option<String>,
        message: String,
        now: i64,
    },
    OutputBatch {
        pane_id: String,
        lines: Vec<OutputLine>,
    },
    PaneRegistered {
        pane_id: String,
        agent_type: String,
        now: i64,
    },
}
