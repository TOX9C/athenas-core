//! Public tool schemas and pure command-building helpers.

use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

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
        ToolDefinition {
            name: "workspace_list".to_string(),
            description: "List all available workspaces. Returns a list of workspace names and their IDs.".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "workspace_get_active".to_string(),
            description: "Get the currently active workspace name and ID.".to_string(),
            input_schema: json!({ "type": "object", "properties": {} }),
        },
        ToolDefinition {
            name: "workspace_switch".to_string(),
            description: "Switch to a different workspace by its ID. Destructive — confirm with the user first.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "space_id": { "type": "string", "description": "The ID of the workspace to switch to." }
                },
                "required": ["space_id"]
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
