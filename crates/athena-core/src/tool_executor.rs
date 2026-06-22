//! Tool executor module — ported from electron/toolExecutor.ts
//!
//! Contains the ORCHESTRATOR_TOOLS definitions, `execute_tool_call` dispatch,
//! `to_openai_tools` conversion, and shell escaping utilities.

use crate::agent_comms::AgentComms;
use crate::kanban::{KanbanBackend, KanbanBackendStatus, KanbanBackendTask};
use crate::output_buffer::OutputBuffer;
use crate::plan_manager::{
    ExecutionPlan, PlanInput, PlanManager, PlanStatus, PlanStepInput, StepStatus,
};
use athena_fs::path_validator::PathValidator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;
use uuid::Uuid;

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
    #[error("Lock poisoned")]
    LockPoisoned,
    #[error("Path traversal blocked: {0}")]
    PathTraversal(String),
}

// ---------------------------------------------------------------------------
// ToolInput
// ---------------------------------------------------------------------------

/// Input parameters for tool calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolInput {
    pub task_prompt: Option<String>,
    pub agent_count: Option<u32>,
    pub agent_type: Option<String>,
    pub command: Option<String>,
    pub pane_ids: Option<Vec<String>>,
    pub pane_id: Option<String>,
    pub limit: Option<usize>,
    pub since_line: Option<u32>,
    pub since_time: Option<u64>,
    pub agent_id: Option<String>,
    pub goal: Option<String>,
    pub reasoning: Option<String>,
    pub steps: Option<Vec<serde_json::Value>>,
    pub step_id: Option<String>,
    pub prompt: Option<String>,
    pub plan_id: Option<String>,
    pub overall_status: Option<String>,
    pub step_evaluations: Option<Vec<serde_json::Value>>,
    pub next_action: Option<String>,
    pub question: Option<String>,
    pub options: Option<Vec<serde_json::Value>>,
    pub message: Option<String>,
    pub target_agent_id: Option<String>,
    pub message_type: Option<String>,
    // Kanban
    pub title: Option<String>,
    pub description: Option<String>,
    pub status: Option<String>,
    pub task_id: Option<String>,
    pub space_id: Option<String>,
    // FS
    pub path: Option<String>,
    pub pattern: Option<String>,
}

// ---------------------------------------------------------------------------
// ToolCallResult
// ---------------------------------------------------------------------------

/// Result of a tool call.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
}

// ---------------------------------------------------------------------------
// Tool definitions
// ---------------------------------------------------------------------------

/// Definition of a single orchestrator tool.
///
/// `input_schema` is a JSON Schema object — `{"type":"object",
/// "properties":{...},"required":[...]}` — passed verbatim to the LLM
/// provider as the tool's parameter schema.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

/// The full list of orchestrator tools, with JSON Schema parameter
/// definitions passed verbatim to the LLM provider.
pub fn orchestrator_tools() -> Vec<ToolDefinition> {
    use serde_json::json;
    vec![
        ToolDefinition {
            name: "close_terminals".to_string(),
            description: "Close and remove terminal panes/agents from the UI using their pane IDs. Use whenever the user asks to close, exit, or remove a running terminal/agent. Destructive — confirm with the user first.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane_ids": {
                        "type": "array",
                        "description": "IDs of the panes to close (taken from the STATE SNAPSHOT).",
                        "items": { "type": "string" }
                    }
                },
                "required": ["pane_ids"]
            }),
        },
        ToolDefinition {
            name: "launch_builtin_agent".to_string(),
            description: "Launch one or more standard background agents. Omit task_prompt to open an interactive agent shell with no initial prompt.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_type": {
                        "type": "string",
                        "enum": ["claude", "codex", "opencode", "gemini", "shell"],
                        "description": "Which built-in agent to spawn. Map names like 'Open Code' -> 'opencode', 'Gemini' -> 'gemini'."
                    },
                    "task_prompt": {
                        "type": "string",
                        "description": "Optional. Initial prompt for the agent. Omit to open a blank interactive shell."
                    },
                    "agent_count": {
                        "type": "number",
                        "description": "How many agents to spawn. Defaults to 1."
                    }
                },
                "required": ["agent_type"]
            }),
        },
        ToolDefinition {
            name: "run_command_in_terminals".to_string(),
            description: "Run a CLI command inside one or more ALREADY OPEN shell/terminal panes.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane_ids": {
                        "type": "array",
                        "description": "IDs of the target panes (from the STATE SNAPSHOT).",
                        "items": { "type": "string" }
                    },
                    "command": {
                        "type": "string",
                        "description": "The command string to execute in the shells."
                    }
                },
                "required": ["pane_ids", "command"]
            }),
        },
        ToolDefinition {
            name: "launch_custom_agent".to_string(),
            description: "Launch one of the user's custom-defined agents using its direct CLI command.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": {
                        "type": "string",
                        "description": "The exact CLI command of the custom agent to launch."
                    },
                    "agent_count": {
                        "type": "number",
                        "description": "How many custom agents to spawn. Defaults to 1."
                    }
                },
                "required": ["command"]
            }),
        },
        ToolDefinition {
            name: "read_agent_output".to_string(),
            description: "Read the captured terminal output from a specific agent pane. Use this to see what an agent has done, check for errors, or read results.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane_id": { "type": "string", "description": "The pane ID to read output from." },
                    "limit": { "type": "number", "description": "Maximum number of lines to return. Defaults to 100." },
                    "since_line": { "type": "number", "description": "Only return lines after this line number (for pagination)." },
                    "since_time": { "type": "number", "description": "Only return lines after this Unix-ms timestamp." }
                },
                "required": ["pane_id"]
            }),
        },
        ToolDefinition {
            name: "list_agents".to_string(),
            description: "List all currently running agent panes with their IDs, types, line counts, and last-activity timestamps. The STATE SNAPSHOT already includes this — call only when you need a refresh.".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "check_agent_status".to_string(),
            description: "Check the current status of a specific agent by its pane or agent ID. Returns connection status, last activity, line count, and whether it is waiting for input.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "agent_id": { "type": "string", "description": "The agent or pane ID to check." }
                },
                "required": ["agent_id"]
            }),
        },
        ToolDefinition {
            name: "create_execution_plan".to_string(),
            description: "Create a structured execution plan before dispatching agents for any non-trivial task. Give each step a unique `id` and a `description`. The step's description is sent to the agent as its prompt, so write each one as a clear, self-contained instruction.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "The high-level goal this plan achieves." },
                    "reasoning": { "type": "string", "description": "Why this plan structure was chosen." },
                    "steps": {
                        "type": "array",
                        "description": "Ordered plan steps.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string", "description": "Unique identifier for this step (e.g. 'step-1'). You reference it later in dispatch_plan_step and evaluate_results." },
                                "description": { "type": "string", "description": "Self-contained instruction for this step. This exact text is sent to the agent as its prompt — keep each step distinct." }
                            },
                            "required": ["id", "description"]
                        }
                    }
                },
                "required": ["goal", "reasoning", "steps"]
            }),
        },
        ToolDefinition {
            name: "dispatch_plan_step".to_string(),
            description: "Launch an agent to execute a step from the active execution plan, by its step id. Use this instead of launch_builtin_agent when executing a plan.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "step_id": { "type": "string", "description": "The id of the step (from create_execution_plan) to dispatch." }
                },
                "required": ["step_id"]
            }),
        },
        ToolDefinition {
            name: "prompt_agent".to_string(),
            description: "Send a prompt or instruction to an already-running agent pane — for follow-ups, clarifications, or re-direction.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pane_id": { "type": "string", "description": "The pane ID of the running agent." },
                    "prompt": { "type": "string", "description": "The prompt or instruction to send." }
                },
                "required": ["pane_id", "prompt"]
            }),
        },
        ToolDefinition {
            name: "ask_user".to_string(),
            description: "Ask the user a clarifying question with selectable options. Use when you need a decision to proceed — choosing an approach, confirming scope, selecting a preference. The user clicks an option (or types a custom reply) and you continue.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "question": { "type": "string", "description": "The question to ask." },
                    "options": {
                        "type": "array",
                        "description": "2-5 selectable choices.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string", "description": "Short text shown on the clickable option button. Required." },
                                "description": { "type": "string", "description": "Optional longer explanation of this choice." }
                            },
                            "required": ["label"]
                        }
                    }
                },
                "required": ["question", "options"]
            }),
        },
        ToolDefinition {
            name: "evaluate_results".to_string(),
            description: "Evaluate the results of an execution plan: read agent outputs, assess whether each step and the overall goal succeeded, and decide the next action.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "plan_id": { "type": "string", "description": "The plan ID to evaluate." },
                    "overall_status": {
                        "type": "string",
                        "enum": ["success", "partial_success", "failure", "needs_replanning"],
                        "description": "Your assessment of the overall plan outcome."
                    },
                    "step_evaluations": {
                        "type": "array",
                        "description": "Per-step outcomes.",
                        "items": {
                            "type": "object",
                            "properties": {
                                "step_id": { "type": "string", "description": "The id of the step being evaluated." },
                                "status": { "type": "string", "enum": ["success", "failure"], "description": "Whether this step succeeded." }
                            },
                            "required": ["step_id", "status"]
                        }
                    },
                    "next_action": {
                        "type": "string",
                        "enum": ["done", "replan", "retry_steps", "escalate_to_user"],
                        "description": "What to do next based on the evaluation."
                    },
                    "reasoning": { "type": "string", "description": "Your reasoning for this evaluation." }
                },
                "required": ["plan_id", "overall_status", "step_evaluations", "next_action", "reasoning"]
            }),
        },
        ToolDefinition {
            name: "kanban_list_tasks".to_string(),
            description: "List all tasks on the active workspace's Kanban board.".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "kanban_create_task".to_string(),
            description: "Create a new Kanban task in the specified workspace.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Title of the task." },
                    "description": { "type": "string", "description": "Optional description of the task." },
                    "status": { "type": "string", "enum": ["todo", "in_progress", "in_review", "complete"], "description": "Status of the task." },
                    "space_id": { "type": "string", "description": "The workspace (space) ID to create the task in." }
                },
                "required": ["title", "space_id"]
            }),
        },
        ToolDefinition {
            name: "kanban_update_task".to_string(),
            description: "Update an existing Kanban task.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "The ID of the task to update." },
                    "title": { "type": "string", "description": "Optional new title." },
                    "description": { "type": "string", "description": "Optional new description." },
                    "status": { "type": "string", "enum": ["todo", "in_progress", "in_review", "complete"], "description": "Optional new status." }
                },
                "required": ["task_id"]
            }),
        },
        ToolDefinition {
            name: "kanban_delete_task".to_string(),
            description: "Delete a Kanban task by its ID. Destructive — confirm with the user first.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "The ID of the task to delete." }
                },
                "required": ["task_id"]
            }),
        },
        ToolDefinition {
            name: "fs_read_file".to_string(),
            description: "Read the contents of a file from the workspace.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the file to read (relative to workspace root or absolute)." }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "fs_list_dir".to_string(),
            description: "List the contents of a directory in the workspace.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Path of the directory to list (relative to workspace root or absolute)." }
                },
                "required": ["path"]
            }),
        },
        ToolDefinition {
            name: "fs_search".to_string(),
            description: "Search files in the workspace using ripgrep.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "The regex or literal pattern to search for." },
                    "path": { "type": "string", "description": "Optional directory to search in. Defaults to the workspace root." }
                },
                "required": ["pattern"]
            }),
        },
    ]
}

// ---------------------------------------------------------------------------
// toOpenAITools
// ---------------------------------------------------------------------------

/// An OpenAI-compatible tool definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAITool {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: OpenAIFunction,
}

/// The function portion of an OpenAI tool definition.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAIFunction {
    pub name: String,
    pub description: String,
    /// JSON Schema for the function parameters, passed through verbatim.
    pub parameters: serde_json::Value,
}

/// Convert the orchestrator tools list into OpenAI function-calling format.
/// Cached via LazyLock to avoid rebuilding the ~14-item vector on every
/// LLM turn (hot path in `send_openai` / `send_anthropic`).
static CACHED_OPENAI_TOOLS: LazyLock<Vec<OpenAITool>> = LazyLock::new(|| {
    orchestrator_tools()
        .into_iter()
        .map(|t| OpenAITool {
            tool_type: "function".to_string(),
            function: OpenAIFunction {
                name: t.name,
                description: t.description,
                parameters: t.input_schema,
            },
        })
        .collect()
});

pub fn to_openai_tools() -> Vec<OpenAITool> {
    CACHED_OPENAI_TOOLS.clone()
}

// ---------------------------------------------------------------------------
// Shell escaping
// ---------------------------------------------------------------------------

/// Shell-escape a single argument using the shell-escape crate.
pub fn shell_escape(arg: &str) -> String {
    shell_escape::escape(arg.into()).to_string()
}

/// Build the agent command string for the given agent type and optional prompt.
pub fn build_agent_command(agent_type: &str, task_prompt: Option<&str>) -> String {
    let base_cmd = match agent_type {
        "codex" => "codex",
        "opencode" => "opencode",
        "gemini" => "gemini",
        "shell" => return String::new(),
        _ => "claude",
    };

    match task_prompt {
        Some(prompt) if !prompt.is_empty() => {
            format!("{} -p {}", base_cmd, shell_escape(prompt))
        }
        _ => base_cmd.to_string(),
    }
}

// ---------------------------------------------------------------------------
// ToolExecutor
// ---------------------------------------------------------------------------

fn now_ms() -> u64 {
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
    output_buffer: Arc<OutputBuffer>,
    plan_manager: Arc<PlanManager>,
    agent_comms: Arc<AgentComms>,
    event_sender: Arc<dyn ToolEventSender>,
    kanban_backend: KanbanBackend,
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
    ) -> Self {
        let kanban_backend = KanbanBackend::new(Arc::clone(&store));
        Self {
            output_buffer,
            plan_manager,
            agent_comms,
            event_sender,
            kanban_backend,
        }
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
            _ => Err(ToolExecutorError::UnknownTool(name.to_string())),
        }
    }

    // -- Individual tool implementations ------------------------------------

    fn launch_builtin_agent(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let agent_type = args.agent_type.as_deref().unwrap_or("claude");
        let agent_count = args.agent_count.unwrap_or(1);
        let agent_command = build_agent_command(agent_type, args.task_prompt.as_deref());

        for _ in 0..agent_count {
            let id = format!("agent-{}", Uuid::new_v4());
            self.event_sender
                .agent_spawned(&id, agent_type, &agent_command);
        }

        Ok(ToolCallResult {
            text: format!("Done, launched {} {} agents.", agent_count, agent_type),
            is_error: None,
        })
    }

    fn launch_custom_agent(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let command = args
            .command
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("command".to_string()))?;
        // Strict allowlist: commands must be explicitly permitted
        let allowed: Vec<String> = std::env::var("ATHENA_COMMAND_ALLOWLIST")
            .ok()
            .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
            .unwrap_or_default();
        if !allowed.contains(&command.trim().to_string()) {
            return Err(ToolExecutorError::Notification(format!(
                "Command not in allowlist: '{}'",
                command
            )));
        }
        let agent_count = args.agent_count.unwrap_or(1);

        for _ in 0..agent_count {
            let id = format!("custom-agent-{}", Uuid::new_v4());
            self.event_sender.agent_spawned(&id, "custom", command);
        }

        Ok(ToolCallResult {
            text: format!("Done, launched {} custom agents.", agent_count),
            is_error: None,
        })
    }

    fn close_terminals(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        if let Some(ref pane_ids) = args.pane_ids {
            self.event_sender.close_panes(pane_ids);
            Ok(ToolCallResult {
                text: format!("Closed {} terminal(s).", pane_ids.len()),
                is_error: None,
            })
        } else {
            Ok(ToolCallResult {
                text: "Closed 0 terminal(s).".to_string(),
                is_error: None,
            })
        }
    }

    fn run_command_in_terminals(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let pane_ids = args.pane_ids.as_deref().unwrap_or(&[]);
        let command = args.command.as_deref().unwrap_or("");

        if !pane_ids.is_empty() && !command.is_empty() {
            for pane_id in pane_ids {
                self.event_sender.pty_write(pane_id, command);
                self.event_sender.pty_write(pane_id, "\r");
            }
        }

        Ok(ToolCallResult {
            text: format!("Sent command to {} terminal(s).", pane_ids.len()),
            is_error: None,
        })
    }

    fn read_agent_output(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let pane_id = args
            .pane_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("pane_id".to_string()))?;

        let opts = crate::output_buffer::GetOutputOptions {
            limit: args.limit.or(Some(100)),
            since_line: args.since_line,
            since_time: args.since_time,
            offset: None,
            raw: None,
        };

        let lines = self.output_buffer.get_output(pane_id, Some(&opts));

        if lines.is_empty() {
            return Ok(ToolCallResult {
                text: format!(
                    "No output captured for pane '{}'. The pane may not exist or has not produced output yet.",
                    pane_id
                ),
                is_error: None,
            });
        }

        let formatted: String = lines
            .iter()
            .map(|l| format!("[{}] {}", l.line_num, l.text))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolCallResult {
            text: formatted,
            is_error: None,
        })
    }

    fn list_agents(&self) -> Result<ToolCallResult, ToolExecutorError> {
        let panes = self.output_buffer.get_agent_list();
        let sessions = self.agent_comms.get_agent_sessions();

        if panes.is_empty() && sessions.is_empty() {
            return Ok(ToolCallResult {
                text: "No agents currently running.".to_string(),
                is_error: None,
            });
        }

        let mut parts: Vec<String> = Vec::new();

        if !panes.is_empty() {
            parts.push("Terminal Panes:".to_string());
            for p in &panes {
                parts.push(format!(
                    "  {} ({}) — {} lines, last activity: {}",
                    p.pane_id,
                    p.agent_type,
                    p.line_count,
                    chrono::DateTime::from_timestamp_millis(p.last_activity_at as i64)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                ));
            }
        }

        if !sessions.is_empty() {
            parts.push("Agent Sessions:".to_string());
            for s in &sessions {
                parts.push(format!(
                    "  {} [{}] — plugin: {}, connected: {}",
                    s.agent_id,
                    s.status,
                    s.plugin_id,
                    chrono::DateTime::from_timestamp_millis(s.connected_at as i64)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                ));
            }
        }

        Ok(ToolCallResult {
            text: parts.join("\n"),
            is_error: None,
        })
    }

    fn check_agent_status(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let agent_id = args
            .agent_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("agent_id".to_string()))?;

        let pane_info = self.output_buffer.get_pane_buffer_info(agent_id);
        let sessions = self.agent_comms.get_agent_sessions();
        let session = sessions
            .iter()
            .find(|s| s.agent_id == agent_id || s.id == agent_id);

        if pane_info.is_none() && session.is_none() {
            return Ok(ToolCallResult {
                text: format!("No agent found with ID '{}'.", agent_id),
                is_error: None,
            });
        }

        let mut parts: Vec<String> = Vec::new();

        if let Some(info) = &pane_info {
            parts.push(format!("Pane: {}", info.pane_id));
            parts.push(format!("Type: {}", info.agent_type));
            parts.push(format!(
                "Lines: {} ({} total)",
                info.line_count, info.total_lines
            ));
            parts.push(format!("Size: {} bytes", info.total_bytes));
            parts.push(format!(
                "Created: {}",
                chrono::DateTime::from_timestamp_millis(info.created_at as i64)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            ));
            parts.push(format!(
                "Last Activity: {}",
                chrono::DateTime::from_timestamp_millis(info.last_activity_at as i64)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            ));
            let is_active = now_ms().saturating_sub(info.last_activity_at) < 30_000;
            parts.push(format!(
                "Status: {}",
                if is_active { "active" } else { "idle" }
            ));
        }

        if let Some(s) = session {
            parts.push(format!("Session: {}", s.id));
            parts.push(format!("Agent ID: {}", s.agent_id));
            parts.push(format!("Connection Status: {}", s.status));
            parts.push(format!(
                "Connected: {}",
                chrono::DateTime::from_timestamp_millis(s.connected_at as i64)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            ));
        }

        let pty_connected = self.event_sender.has_session(agent_id);
        parts.push(format!("PTY Connected: {}", pty_connected));

        Ok(ToolCallResult {
            text: parts.join("\n"),
            is_error: None,
        })
    }

    fn create_execution_plan(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let goal = args
            .goal
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("goal".to_string()))?;
        let reasoning = args
            .reasoning
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("reasoning".to_string()))?;

        let steps: Vec<PlanStepInput> = args
            .steps
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|s| PlanStepInput {
                id: s
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: s
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
            .collect();

        let plan = self.plan_manager.set_active_plan(PlanInput {
            goal: goal.to_string(),
            reasoning: reasoning.to_string(),
            steps,
        })?;

        self.event_sender.plan_update(&plan);

        let step_summary: String = plan
            .steps
            .iter()
            .map(|s| format!("  {}: {}", s.id, s.description))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolCallResult {
            text: format!(
                "Plan created ({}):\nGoal: {}\nSteps:\n{}",
                plan.id, plan.goal, step_summary
            ),
            is_error: None,
        })
    }

    fn dispatch_plan_step(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let step_id = args
            .step_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("step_id".to_string()))?;

        let plan = match self.plan_manager.get_active_plan() {
            Some(p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "No active execution plan. Create one first with create_execution_plan."
                        .to_string(),
                    is_error: None,
                })
            }
        };

        let step = match plan.steps.iter().find(|s| s.id == step_id) {
            Some(s) => s,
            None => {
                return Ok(ToolCallResult {
                    text: format!("Step '{}' not found in active plan.", step_id),
                    is_error: None,
                })
            }
        };

        // Dispatch the agent — default to "claude" as the agent type
        // since the plan step does not carry its own agent_type field.
        let default_agent_type = "claude";
        let agent_cmd = build_agent_command(default_agent_type, Some(&step.description));
        let pane_id = format!(
            "plan-{}-{}-{}",
            plan.id,
            step_id,
            &Uuid::new_v4().to_string()[..8]
        );
        self.event_sender
            .agent_spawned(&pane_id, default_agent_type, &agent_cmd);

        self.plan_manager
            .update_step_status(step_id, StepStatus::InProgress, Some(&pane_id))?;

        if let Some(updated_plan) = self.plan_manager.get_active_plan() {
            self.event_sender.plan_update(&updated_plan);
        }

        Ok(ToolCallResult {
            text: format!(
                "Dispatched step '{}' ({}) -> pane {}",
                step_id, step.description, pane_id
            ),
            is_error: None,
        })
    }

    fn prompt_agent(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let pane_id = match args.pane_id {
            Some(ref id) => id,
            None => {
                return Ok(ToolCallResult {
                    text: "Missing pane_id or prompt.".to_string(),
                    is_error: None,
                })
            }
        };
        let prompt = match args.prompt {
            Some(ref p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "Missing pane_id or prompt.".to_string(),
                    is_error: None,
                })
            }
        };

        if !self.event_sender.has_session(pane_id) {
            return Ok(ToolCallResult {
                text: format!("No active PTY session for pane '{}'.", pane_id),
                is_error: None,
            });
        }

        self.event_sender.pty_write(pane_id, prompt);
        self.event_sender.pty_write(pane_id, "\r");

        Ok(ToolCallResult {
            text: format!("Prompt sent to {}.", pane_id),
            is_error: None,
        })
    }

    fn ask_user(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let request_id = Uuid::new_v4().to_string();
        let question = args
            .question
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("question".to_string()))?;
        let options = args.options.as_deref().unwrap_or(&[]);

        let answer = self.event_sender.ask_user(&request_id, question, options);

        Ok(ToolCallResult {
            text: format!("User selected: {}", answer),
            is_error: None,
        })
    }

    fn evaluate_results(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let plan = match self.plan_manager.get_active_plan() {
            Some(p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "No active execution plan to evaluate.".to_string(),
                    is_error: None,
                })
            }
        };

        // Update step statuses
        if let Some(ref evals) = args.step_evaluations {
            for eval_item in evals {
                let step_id = eval_item
                    .get("step_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status_str = eval_item
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("failure");

                let step_status = if status_str == "success" {
                    StepStatus::Completed
                } else {
                    StepStatus::Failed
                };

                let _ = self
                    .plan_manager
                    .update_step_status(step_id, step_status, None);
            }
        }

        // Update plan status
        let status_map: HashMap<&str, PlanStatus> = {
            let mut m = HashMap::new();
            m.insert("success", PlanStatus::Completed);
            m.insert("partial_success", PlanStatus::Completed);
            m.insert("failure", PlanStatus::Failed);
            m.insert("needs_replanning", PlanStatus::Failed);
            m
        };

        let plan_status = args
            .overall_status
            .as_deref()
            .and_then(|s| status_map.get(s))
            .copied()
            .unwrap_or(PlanStatus::Completed);

        let _ = self.plan_manager.update_plan_status(plan_status);

        let updated_plan = match self.plan_manager.get_active_plan() {
            Some(p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "No active execution plan to evaluate.".to_string(),
                    is_error: None,
                })
            }
        };

        self.event_sender.plan_update(&updated_plan);

        let evals = args.step_evaluations.as_deref().unwrap_or(&[]);
        self.event_sender.plan_evaluated(
            &plan.id,
            args.overall_status.as_deref().unwrap_or("unknown"),
            evals,
            args.next_action.as_deref().unwrap_or("done"),
            args.reasoning.as_deref().unwrap_or(""),
        );

        let action_instructions: HashMap<&str, &str> = {
            let mut m = HashMap::new();
            m.insert("done", "Plan complete. Report results to the user.");
            m.insert(
                "replan",
                "Create a new execution plan addressing the failures.",
            );
            m.insert("retry_steps", "Re-dispatch the failed steps.");
            m.insert(
                "escalate_to_user",
                "Ask the user for guidance on how to proceed.",
            );
            m
        };

        let next = args.next_action.as_deref().unwrap_or("done");
        let instruction = action_instructions.get(next).copied().unwrap_or(next);

        Ok(ToolCallResult {
            text: format!(
                "Evaluation recorded. Overall: {}. Next: {}",
                args.overall_status.as_deref().unwrap_or("unknown"),
                instruction
            ),
            is_error: None,
        })
    }

    // -- Kanban tools -------------------------------------------------------

    fn get_workspace_root(&self) -> Result<PathBuf, ToolExecutorError> {
        std::env::current_dir()
            .and_then(|p| p.canonicalize())
            .map_err(|e| {
                ToolExecutorError::Io(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Failed to get workspace root: {}", e),
                ))
            })
    }

    fn validate_path(&self, path: &str) -> Result<PathBuf, ToolExecutorError> {
        let root = self.get_workspace_root()?;
        let path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            root.join(path)
        };
        let validator = PathValidator::new(&root).map_err(|e| {
            ToolExecutorError::PathTraversal(format!("failed to initialize path validator: {}", e))
        })?;
        // TODO: opt-in allowlist for extra roots
        validator
            .validate(&path)
            .map_err(|e| ToolExecutorError::PathTraversal(e.to_string()))
    }

    fn get_current_time_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    }

    fn kanban_list_tasks(&self) -> Result<ToolCallResult, ToolExecutorError> {
        let workspace_id = match self.kanban_backend.get_active_workspace_id() {
            Ok(id) => id,
            Err(_) => {
                return Ok(ToolCallResult {
                    text: "No active workspace found.".to_string(),
                    is_error: None,
                })
            }
        };

        let tasks = match self.kanban_backend.get_tasks(&workspace_id) {
            Ok(tasks) => tasks,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error reading kanban tasks: {}", e),
                    is_error: Some(true),
                })
            }
        };

        if tasks.is_empty() {
            return Ok(ToolCallResult {
                text: "No tasks found on the Kanban board.".to_string(),
                is_error: None,
            });
        }

        let json = match serde_json::to_string(&tasks) {
            Ok(j) => j,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error serializing tasks: {}", e),
                    is_error: Some(true),
                })
            }
        };

        Ok(ToolCallResult {
            text: json,
            is_error: None,
        })
    }

    fn kanban_create_task(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let title = args
            .title
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("title".to_string()))?;
        let space_id = args
            .space_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("space_id".to_string()))?;

        let status = match args.status.as_deref() {
            Some(s) => KanbanBackendStatus::from_str(s).unwrap_or(KanbanBackendStatus::Todo),
            None => KanbanBackendStatus::Todo,
        };

        let task = KanbanBackendTask {
            id: format!("task-{}", Uuid::new_v4()),
            space_id: space_id.to_string(),
            title: title.to_string(),
            description: args.description.clone(),
            assigned_agent: None,
            status,
            order: 0,
            created_at: self.get_current_time_ms(),
        };

        match self.kanban_backend.create_task(space_id, task) {
            Ok(created) => Ok(ToolCallResult {
                text: format!("Task created: {} (ID: {})", created.title, created.id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error creating task: {}", e),
                is_error: Some(true),
            }),
        }
    }

    fn kanban_update_task(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let task_id = args
            .task_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("task_id".to_string()))?;

        let workspace_id = match self.kanban_backend.get_active_workspace_id() {
            Ok(id) => id,
            Err(_) => {
                return Ok(ToolCallResult {
                    text: "No active workspace found.".to_string(),
                    is_error: Some(true),
                })
            }
        };

        let status = args
            .status
            .as_ref()
            .and_then(|s| KanbanBackendStatus::from_str(s).ok());

        match self.kanban_backend.update_task(
            &workspace_id,
            task_id,
            args.title.clone(),
            args.description.clone(),
            status,
        ) {
            Ok(updated) => Ok(ToolCallResult {
                text: format!("Task updated: {} (ID: {})", updated.title, updated.id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error updating task: {}", e),
                is_error: Some(true),
            }),
        }
    }

    fn kanban_delete_task(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let task_id = args
            .task_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("task_id".to_string()))?;

        let workspace_id = match self.kanban_backend.get_active_workspace_id() {
            Ok(id) => id,
            Err(_) => {
                return Ok(ToolCallResult {
                    text: "No active workspace found.".to_string(),
                    is_error: Some(true),
                })
            }
        };

        match self.kanban_backend.delete_task(&workspace_id, task_id) {
            Ok(_) => Ok(ToolCallResult {
                text: format!("Task {} deleted.", task_id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error deleting task: {}", e),
                is_error: Some(true),
            }),
        }
    }

    // -- File system tools --------------------------------------------------

    fn fs_read_file(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let path = args
            .path
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("path".to_string()))?;

        let validated = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Invalid path '{}': {}", path, e),
                    is_error: Some(true),
                })
            }
        };

        if !validated.exists() {
            return Ok(ToolCallResult {
                text: format!("File not found: {}", validated.display()),
                is_error: Some(true),
            });
        }

        match std::fs::read_to_string(&validated) {
            Ok(contents) => Ok(ToolCallResult {
                text: contents,
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Failed to read file '{}': {}", validated.display(), e),
                is_error: Some(true),
            }),
        }
    }

    fn fs_list_dir(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let path = args
            .path
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("path".to_string()))?;

        let validated = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Invalid path '{}': {}", path, e),
                    is_error: Some(true),
                })
            }
        };

        if !validated.exists() {
            return Ok(ToolCallResult {
                text: format!("Directory not found: {}", validated.display()),
                is_error: Some(true),
            });
        }

        let entries = match std::fs::read_dir(&validated) {
            Ok(dir) => dir,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Failed to read directory '{}': {}", validated.display(), e),
                    is_error: Some(true),
                })
            }
        };

        let mut results: Vec<serde_json::Value> = Vec::new();
        for entry in entries {
            match entry {
                Ok(e) => {
                    let name = e.file_name().to_string_lossy().to_string();
                    let path = e.path().to_string_lossy().to_string();
                    let is_dir = match e.file_type() {
                        Ok(ft) => ft.is_dir(),
                        Err(_) => false,
                    };
                    results.push(serde_json::json!({
                        "name": name,
                        "path": path,
                        "is_dir": is_dir,
                    }));
                }
                Err(_) => continue,
            }
        }

        let json = match serde_json::to_string(&results) {
            Ok(j) => j,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error serializing directory entries: {}", e),
                    is_error: Some(true),
                })
            }
        };

        Ok(ToolCallResult {
            text: json,
            is_error: None,
        })
    }

    fn fs_search(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let pattern = args
            .pattern
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("pattern".to_string()))?;
        // `path` is optional in the tool schema; default to the workspace root.
        let path = args.path.as_deref().unwrap_or(".");

        let validated = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Invalid path '{}': {}", path, e),
                    is_error: Some(true),
                })
            }
        };

        let options = crate::types::SearchOptions {
            pattern: pattern.to_string(),
            path: validated.to_string_lossy().to_string(),
            glob: None,
            case_sensitive: false,
            max_results: Some(50),
            context_lines: Some(2),
        };

        // Drive the async `search_code` on the current Tokio runtime.
        // `fs_search` is sync because `execute_tool_call` dispatches it
        // without an `await` in the Tauri command handler (`spawn_blocking`
        // closure) and the orchestrator lock-guard chain. We must keep the
        // signature sync to avoid cascading `async`/`Send` changes, so we
        // bridge via `Handle::current().block_on`. The runtime is always
        // available in practice (Tauri main + MCP server are both async),
        // and this replaces a `std::process::Command` that would block
        // the worker thread.
        let search_result = tokio::runtime::Handle::current()
            .block_on(crate::search::search_code(&options));

        match search_result {
            Ok(result) => {
                let json = match serde_json::to_string(&result) {
                    Ok(j) => j,
                    Err(e) => {
                        return Ok(ToolCallResult {
                            text: format!("Error serializing search results: {}", e),
                            is_error: Some(true),
                        })
                    }
                };
                Ok(ToolCallResult {
                    text: json,
                    is_error: None,
                })
            }
            Err(e) => Ok(ToolCallResult {
                text: format!("Search failed: {}", e),
                is_error: Some(true),
            }),
        }
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
        )
    }

    struct CurrentDirGuard {
        original: std::path::PathBuf,
    }

    impl Drop for CurrentDirGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn path_validation_security() {
        let temp_dir = tempfile::tempdir().unwrap();
        let original_dir = std::env::current_dir().unwrap();
        // Create the workspace marker so get_workspace_root() can find a root.
        let marker = temp_dir.path().join("src-tauri").join("tauri.conf.json");
        std::fs::create_dir_all(marker.parent().unwrap()).unwrap();
        std::fs::write(&marker, r##"{ "build": { "beforeBuildCommand": "echo" } }"##).unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();
        let _guard = CurrentDirGuard {
            original: original_dir,
        };

        let executor = create_executor();

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
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&tmp).unwrap();
        let _guard = CurrentDirGuard {
            original: original_dir,
        };

        // Write a file inside the workspace (current_dir)
        std::fs::write(tmp.path().join("target.txt"), "needle in haystack\n").unwrap();

        let executor = create_executor();
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
                    let parsed: serde_json::Value = serde_json::from_str(&call.text)
                        .expect("fs_search result should be JSON");
                    let total = parsed["stats"]["total_matches"].as_u64().unwrap_or(0);
                    assert!(
                        total >= 1,
                        "expected at least one match, got {}",
                        total
                    );
                }
            }
            Err(e) => panic!("fs_search should not propagate Err to callers: {e}"),
        }
    }
}
