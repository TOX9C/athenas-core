//! Tool executor module — ported from electron/toolExecutor.ts
//!
//! Contains the stateful `execute_tool_call` dispatch and service integrations.
//! Public tool schemas and pure command-building helpers remain available here
//! through compatibility re-exports from the `tool_schema` module.

use crate::agent_comms::AgentComms;
use crate::kanban::KanbanBackend;
use crate::output_buffer::OutputBuffer;
use crate::plan_manager::{ExecutionPlan, PlanManager};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;
#[path = "agent_tools.rs"]
mod agent_tools;
#[path = "fs_tools.rs"]
mod fs_tools;
#[path = "kanban_tools.rs"]
mod kanban_tools;
#[path = "plan_tools.rs"]
mod plan_tools;
#[path = "workspace_tools.rs"]
mod workspace_tools;

pub use crate::tool_schema::{
    build_agent_command, orchestrator_tools, shell_escape, to_openai_tools, OpenAIFunction,
    OpenAITool, ToolCallResult, ToolDefinition, ToolInput,
};

// ---------------------------------------------------------------------------
// Error types
// ---------------------------------------------------------------------------

/// Errors that can occur during tool execution.
#[derive(Debug, Error)]
pub enum ToolExecutorError {
    #[error("No window available")]
    NoWindow,
    #[error("Unknown tool: {0}")]
    UnknownTool(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Agent comms error: {0}")]
    AgentComms(#[from] crate::agent_comms::AgentCommsError),
    #[error("Plan manager error: {0}")]
    PlanManager(#[from] crate::plan_manager::PlanManagerError),
    #[error("Notification error: {0}")]
    Notification(String),
    #[error("Missing required parameter: {0}")]
    MissingParam(String),
    #[error("Request cancelled")]
    Cancelled,
    #[error("Lock poisoned")]
    LockPoisoned,
    #[error("Path traversal blocked: {0}")]
    PathTraversal(String),
}

// ---------------------------------------------------------------------------
// ToolExecutor
// ---------------------------------------------------------------------------

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|_| {
            log::warn!("System clock error");
            std::time::Duration::default()
        })
        .as_millis() as u64
}

/// Callback trait for events that would normally go through Electron IPC.
///
/// In the original TypeScript code, tool execution sends IPC messages
/// to the renderer process. In Rust, we use a trait object so the
/// consumer can decide what to do with these events.
pub trait ToolEventSender: Send + Sync {
    /// An agent was spawned.
    fn agent_spawned(&self, id: &str, agent_type: &str, agent_cmd: &str);

    /// Close panes by their IDs.
    fn close_panes(&self, pane_ids: &[String]);

    /// Write data to a PTY session.
    fn pty_write(&self, pane_id: &str, data: &str);

    /// Check if a PTY session exists.
    fn has_session(&self, pane_id: &str) -> bool;

    /// Ask the user a question (returns the answer).
    fn ask_user(&self, request_id: &str, question: &str, options: &[serde_json::Value]) -> String;

    /// Associate tool-generated UI events with the current assistant request.
    fn set_request_context(&self, _request_id: &str, _session_id: &str) {}

    /// Clear the current assistant request context.
    fn clear_request_context(&self) {}

    /// Return whether a request was cancelled before a tool began.
    fn request_cancelled(&self, _request_id: &str) -> bool {
        false
    }

    /// Forget cancellation tombstones once a request has fully unwound.
    fn finish_request(&self, _request_id: &str) {}

    /// Cancel pending UI interactions belonging to an assistant request.
    fn cancel_request(&self, _request_id: &str) -> bool {
        false
    }

    /// Send a plan update event.
    fn plan_update(&self, plan: &ExecutionPlan);

    /// Send a plan evaluated event.
    fn plan_evaluated(
        &self,
        plan_id: &str,
        overall_status: &str,
        step_evaluations: &[serde_json::Value],
        next_action: &str,
        reasoning: &str,
    );
}

/// The tool executor — dispatches tool calls to the appropriate service.
pub struct ToolExecutor {
    pub(super) output_buffer: Arc<OutputBuffer>,
    pub(super) plan_manager: Arc<PlanManager>,
    pub(super) agent_comms: Arc<AgentComms>,
    pub(super) event_sender: Arc<dyn ToolEventSender>,
    pub(super) kanban_backend: KanbanBackend,
    pub(super) store: Arc<athena_store::KeyValueStore>,
    pub(super) notification_service: Option<Arc<crate::notification::NotificationService>>,
    /// Override for workspace root — used by tests to avoid mutating the
    /// process-global CWD via `std::env::set_current_dir`. When `Some`,
    /// `get_workspace_root` returns this path directly.
    #[allow(dead_code)]
    workspace_root_override: Option<PathBuf>,
}

impl std::fmt::Debug for ToolExecutor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ToolExecutor").finish()
    }
}

impl ToolExecutor {
    pub fn new(
        output_buffer: Arc<OutputBuffer>,
        plan_manager: Arc<PlanManager>,
        agent_comms: Arc<AgentComms>,
        event_sender: Arc<dyn ToolEventSender>,
        store: Arc<athena_store::KeyValueStore>,
        notification_service: Option<Arc<crate::notification::NotificationService>>,
    ) -> Self {
        let kanban_backend = KanbanBackend::new(Arc::clone(&store));
        Self {
            output_buffer,
            plan_manager,
            agent_comms,
            event_sender,
            kanban_backend,
            store,
            notification_service,
            workspace_root_override: None,
        }
    }

    /// Set the workspace root override (for tests).  Avoids mutating the
    /// process-global CWD from parallel `#[tokio::test]` cases which race
    /// on `std::env::set_current_dir`.
    #[cfg(test)]
    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_root_override = Some(root);
        self
    }

    /// Associate tool-generated UI events with a streamed assistant request.
    pub fn set_request_context(&self, request_id: &str, session_id: &str) {
        self.event_sender
            .set_request_context(request_id, session_id);
    }

    pub fn clear_request_context(&self) {
        self.event_sender.clear_request_context();
    }

    pub fn cancel_request(&self, request_id: &str) -> bool {
        self.event_sender.cancel_request(request_id)
    }

    pub fn request_cancelled(&self, request_id: &str) -> bool {
        self.event_sender.request_cancelled(request_id)
    }

    pub fn finish_request(&self, request_id: &str) {
        self.event_sender.finish_request(request_id);
    }

    /// Clone the event sender for cancellation paths that must not wait for
    /// the executor mutex (for example while a blocking ask_user call holds it).
    pub(crate) fn event_sender_handle(&self) -> Arc<dyn ToolEventSender> {
        Arc::clone(&self.event_sender)
    }

    /// Execute a tool call by name with the given arguments.
    pub fn execute_tool_call(
        &self,
        name: &str,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        match name {
            "launch_builtin_agent" => self.launch_builtin_agent(args),
            "launch_custom_agent" => self.launch_custom_agent(args),
            "close_terminals" => self.close_terminals(args),
            "run_command_in_terminals" => self.run_command_in_terminals(args),
            "read_agent_output" => self.read_agent_output(args),
            "list_agents" => self.list_agents(),
            "check_agent_status" => self.check_agent_status(args),
            "create_execution_plan" => self.create_execution_plan(args),
            "dispatch_plan_step" => self.dispatch_plan_step(args),
            "prompt_agent" => self.prompt_agent(args),
            "ask_user" => self.ask_user(args),
            "evaluate_results" => self.evaluate_results(args),
            "kanban_list_tasks" => self.kanban_list_tasks(),
            "kanban_create_task" => self.kanban_create_task(args),
            "kanban_update_task" => self.kanban_update_task(args),
            "kanban_delete_task" => self.kanban_delete_task(args),
            "fs_read_file" => self.fs_read_file(args),
            "fs_list_dir" => self.fs_list_dir(args),
            "fs_search" => self.fs_search(args),
            "workspace_list" => self.workspace_list(),
            "workspace_get_active" => self.workspace_get_active(),
            "workspace_switch" => self.workspace_switch(args),
            _ => Err(ToolExecutorError::UnknownTool(name.to_string())),
        }
    }

    /// Execute one streamed tool while its request/session context is held
    /// exclusively by this executor. The context is cleared before the mutex
    /// is released, so concurrent legacy/MCP calls cannot inherit stream IDs.
    pub fn execute_tool_call_with_context(
        &self,
        name: &str,
        args: &ToolInput,
        request_id: &str,
        session_id: &str,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        if self.request_cancelled(request_id) {
            return Err(ToolExecutorError::Cancelled);
        }
        self.set_request_context(request_id, session_id);
        let result = self.execute_tool_call(name, args);
        self.clear_request_context();
        result
    }

    // -- Individual tool implementations ------------------------------------

    fn ask_user(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let request_id = Uuid::new_v4().to_string();
        let question = args
            .question
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("question".to_string()))?;
        let options = args.options.as_deref().unwrap_or(&[]);

        let answer = self.event_sender.ask_user(&request_id, question, options);

        if let Some(ref svc) = self.notification_service {
            let _ = svc.notify(
                crate::notification::NotificationType::NeedsInput,
                "Needs Input",
                question.to_string(),
            );
        }

        Ok(ToolCallResult {
            text: format!("User selected: {}", answer),
            is_error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct MockEventSender;
    impl ToolEventSender for MockEventSender {
        fn agent_spawned(&self, _id: &str, _agent_type: &str, _agent_cmd: &str) {}
        fn close_panes(&self, _pane_ids: &[String]) {}
        fn pty_write(&self, _pane_id: &str, _data: &str) {}
        fn has_session(&self, _pane_id: &str) -> bool {
            false
        }
        fn ask_user(
            &self,
            _request_id: &str,
            _question: &str,
            _options: &[serde_json::Value],
        ) -> String {
            String::new()
        }
        fn plan_update(&self, _plan: &ExecutionPlan) {}
        fn plan_evaluated(
            &self,
            _plan_id: &str,
            _overall_status: &str,
            _step_evaluations: &[serde_json::Value],
            _next_action: &str,
            _reasoning: &str,
        ) {
        }
    }

    fn create_executor() -> ToolExecutor {
        ToolExecutor::new(
            Arc::new(OutputBuffer::new()),
            Arc::new(PlanManager::new()),
            Arc::new(AgentComms::new()),
            Arc::new(MockEventSender),
            Arc::new(athena_store::KeyValueStore::new_empty()),
            None,
        )
    }

    // CurrentDirGuard removed — tests now use with_workspace_root() to inject
    // the temp dir directly via ToolExecutor's override field, avoiding any
    // mutation of the process-global CWD (which races under parallel test
    // execution).

    #[test]
    fn path_validation_security() {
        let temp_dir = tempfile::tempdir().unwrap();
        // Create the workspace marker so PathValidator can find a root.
        let marker = temp_dir.path().join("src-tauri").join("tauri.conf.json");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(
            &marker,
            r##"{ "build": { "beforeBuildCommand": "echo" } }"##,
        )
        .unwrap();

        let executor = create_executor().with_workspace_root(temp_dir.path().to_path_buf());

        // Test absolute path escape
        assert!(
            executor.validate_path("/etc/passwd").is_err(),
            "absolute path outside workspace should be blocked"
        );

        // Test dotdot escape
        assert!(
            executor.validate_path("../../../etc/passwd").is_err(),
            "dotdot escape should be blocked"
        );

        // Test symlink escape
        #[cfg(unix)]
        {
            let link_path = temp_dir.path().join("evil_link");
            std::os::unix::fs::symlink("/etc/passwd", &link_path).unwrap();
            assert!(
                executor.validate_path("evil_link").is_err(),
                "symlink escape should be blocked"
            );
        }

        // Test in-workspace file
        let file_path = temp_dir.path().join("hello.txt");
        std::fs::write(&file_path, "hello world").unwrap();
        assert!(
            executor.validate_path("hello.txt").is_ok(),
            "in-workspace file should be allowed"
        );
    }

    /// Verifies that `fs_search` (sync) drives the async `search_code` via
    /// `Handle::current().block_on`. The tool is always invoked from an
    /// async context in production (Tauri command handlers wrap it in
    /// `tokio::task::spawn_blocking` and the MCP server is fully async).
    /// We mirror that by calling from inside `spawn_blocking`, which
    /// schedules the call on a dedicated thread where `block_on` is safe.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_fs_search_uses_async_search_code() {
        let tmp = tempfile::tempdir().unwrap();
        // Create the workspace marker.
        let marker = tmp.path().join("src-tauri").join("tauri.conf.json");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(
            &marker,
            r##"{ "build": { "beforeBuildCommand": "echo" } }"##,
        )
        .unwrap();

        // Write a file inside the workspace
        std::fs::write(tmp.path().join("target.txt"), "needle in haystack\n").unwrap();

        let executor = create_executor().with_workspace_root(tmp.path().to_path_buf());

        let args = ToolInput {
            pattern: Some("needle".to_string()),
            path: Some("target.txt".to_string()),
            ..Default::default()
        };

        // Mirror production: Tauri command handlers wrap `fs_search` in
        // `tokio::task::spawn_blocking`. Doing the same here keeps
        // `Handle::current().block_on` off the runtime-driving thread.
        let result = tokio::task::spawn_blocking(move || executor.fs_search(&args))
            .await
            .expect("spawn_blocking join");
        match result {
            Ok(call) => {
                // ripgrep not installed on the host would surface as an
                // error payload, not a panic.
                if call.is_error.unwrap_or(false) {
                    let msg = call.text;
                    if msg.contains("ripgrep") || msg.contains("rg") {
                        eprintln!("ripgrep unavailable — skipping assertion: {msg}");
                    } else {
                        panic!("fs_search returned unexpected error: {msg}");
                    }
                } else {
                    let parsed: serde_json::Value =
                        serde_json::from_str(&call.text).expect("fs_search result should be JSON");
                    let total = parsed["stats"]["total_matches"].as_u64().unwrap_or(0);
                    assert!(total >= 1, "expected at least one match, got {}", total);
                }
            }
            Err(e) => panic!("fs_search should not propagate Err to callers: {e}"),
        }
    }

    /// Integration test: workspace tools work end-to-end with a real store.
    #[test]
    fn test_workspace_tools_roundtrip() {
        let store = Arc::new(athena_store::KeyValueStore::new_empty());

        // Seed workspaces key
        let workspaces = serde_json::json!({
            "active_space_id": "space-1",
            "spaces": [
                {"id": "space-1", "name": "Backend Refactor"},
                {"id": "space-2", "name": "Frontend Polish"}
            ]
        });
        store
            .set_sync("workspaces", &workspaces.to_string())
            .unwrap();

        // Build executor with real store (keep a handle for post-switch
        // blob verification)
        let store_handle = store.clone();
        let executor = ToolExecutor::new(
            Arc::new(OutputBuffer::new()),
            Arc::new(PlanManager::new()),
            Arc::new(AgentComms::new()),
            Arc::new(MockEventSender),
            store,
            None,
        );

        // workspace_list
        let list_result = executor
            .execute_tool_call("workspace_list", &ToolInput::default())
            .unwrap();
        assert!(
            !list_result.is_error.unwrap_or(false),
            "workspace_list failed: {}",
            list_result.text
        );
        let spaces: Vec<serde_json::Value> = serde_json::from_str(&list_result.text).unwrap();
        assert_eq!(spaces.len(), 2);

        // workspace_get_active — before switch, should report space-1
        let active_result = executor
            .execute_tool_call("workspace_get_active", &ToolInput::default())
            .unwrap();
        assert!(
            !active_result.is_error.unwrap_or(false),
            "workspace_get_active failed: {}",
            active_result.text
        );
        let active_before: serde_json::Value = serde_json::from_str(&active_result.text).unwrap();
        assert_eq!(
            active_before.get("id").and_then(|v| v.as_str()),
            Some("space-1"),
            "active should be space-1 before switch"
        );

        // workspace_switch
        let switch_input = ToolInput {
            space_id: Some("space-2".to_string()),
            ..Default::default()
        };
        let switch_result = executor
            .execute_tool_call("workspace_switch", &switch_input)
            .unwrap();
        assert!(
            !switch_result.is_error.unwrap_or(false),
            "workspace_switch failed: {}",
            switch_result.text
        );

        // Verify switch stuck via the tool's own reader…
        let active_after = executor
            .execute_tool_call("workspace_get_active", &ToolInput::default())
            .unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&active_after.text).unwrap();
        assert_eq!(parsed.get("id").and_then(|v| v.as_str()), Some("space-2"));

        let blob = store_handle
            .get::<String>("workspaces")
            .unwrap()
            .expect("workspaces blob present");
        let blob_val: serde_json::Value = serde_json::from_str(&blob).unwrap();
        assert_eq!(
            blob_val.get("active_space_id").and_then(|v| v.as_str()),
            Some("space-2"),
            "active_space_id must be updated inside the workspaces blob"
        );
        assert_eq!(
            store_handle.get::<String>("workspace.active").unwrap(),
            None,
            "the orphan workspace.active key must not be written"
        );
    }
}
