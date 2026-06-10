//! Tool executor module — ported from electron/toolExecutor.ts
//!
//! Contains the ORCHESTRATOR_TOOLS definitions, `execute_tool_call` dispatch,
//! `to_openai_tools` conversion, and shell escaping utilities.

use crate::agent_comms::AgentComms;
use crate::kanban::{KanbanBackend, KanbanBackendStatus, KanbanBackendTask};
use crate::notification::NotificationService;
use crate::output_buffer::OutputBuffer;
use crate::plan_manager::{
    ExecutionPlan, PlanInput, PlanManager, PlanStatus, PlanStepInput, StepStatus,
};
use athena_fs::path_validator::PathValidator;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
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

/// Schema for a single property in a tool's input schema.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolPropertySchema {
    #[serde(rename = "type")]
    pub prop_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<ToolPropertySchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#enum: Option<Vec<String>>,
}

/// Input schema for a tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolInputSchema {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, ToolPropertySchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Definition of a single MCP tool.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: ToolInputSchema,
}

/// The full list of orchestrator tools.
pub fn orchestrator_tools() -> Vec<ToolDefinition> {
    vec![
        ToolDefinition {
            name: "close_terminals".to_string(),
            description: "Close, remove, or replace terminal panes/agents from the UI entirely (using pane IDs). Use this tool whenever the user asks to close, exit, completely remove, or replace an existing running terminal/agent.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("pane_ids".to_string(), ToolPropertySchema {
                        prop_type: "array".to_string(),
                        description: Some("Array of string IDs of the panes to drop/remove.".to_string()),
                        items: Some(Box::new(ToolPropertySchema {
                            prop_type: "string".to_string(),
                            description: None,
                            items: None,
                            r#enum: None,
                        })),
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["pane_ids".to_string()]),
            },
        },
        ToolDefinition {
            name: "launch_builtin_agent".to_string(),
            description: "Launch one or more standard background agents using system built-in integrations. If the user doesn't specify a task, you MUST leave task_prompt empty to launch an interactive agent shell.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("agent_type".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The type of agent to spawn. Must be one of: 'claude', 'codex', 'opencode', 'gemini', 'shell'. Examples: 'Open Code' -> 'opencode', 'Gemini' -> 'gemini'.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("task_prompt".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional. The prompt to start the background agent with. Leave entirely empty or omit it to open a blank terminal.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("agent_count".to_string(), ToolPropertySchema {
                        prop_type: "number".to_string(),
                        description: Some("The number of agents to spawn.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["agent_type".to_string(), "agent_count".to_string()]),
            },
        },
        ToolDefinition {
            name: "run_command_in_terminals".to_string(),
            description: "Run a CLI command inside one or more ALREADY OPEN shell/terminal panes.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("pane_ids".to_string(), ToolPropertySchema {
                        prop_type: "array".to_string(),
                        description: Some("Array of string IDs of the panes (from the Currently Running Terminals list).".to_string()),
                        items: Some(Box::new(ToolPropertySchema {
                            prop_type: "string".to_string(),
                            description: None,
                            items: None,
                            r#enum: None,
                        })),
                        r#enum: None,
                    });
                    props.insert("command".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The command string to execute in the shells.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["pane_ids".to_string(), "command".to_string()]),
            },
        },
        ToolDefinition {
            name: "launch_custom_agent".to_string(),
            description: "Launch one of the user's custom-defined agents using a direct CLI command.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("command".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The exact CLI command of the custom agent to launch.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("agent_count".to_string(), ToolPropertySchema {
                        prop_type: "number".to_string(),
                        description: Some("The number of custom agents to spawn.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["command".to_string(), "agent_count".to_string()]),
            },
        },
        ToolDefinition {
            name: "read_agent_output".to_string(),
            description: "Read the captured terminal output from a specific agent pane. Use this to see what an agent has been doing, check for errors, or read results.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("pane_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The pane ID of the agent to read output from.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("limit".to_string(), ToolPropertySchema {
                        prop_type: "number".to_string(),
                        description: Some("Maximum number of lines to return. Defaults to 100.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("since_line".to_string(), ToolPropertySchema {
                        prop_type: "number".to_string(),
                        description: Some("Only return lines after this line number (for pagination).".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("since_time".to_string(), ToolPropertySchema {
                        prop_type: "number".to_string(),
                        description: Some("Only return lines after this Unix ms timestamp.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["pane_id".to_string()]),
            },
        },
        ToolDefinition {
            name: "list_agents".to_string(),
            description: "List all currently running agent panes with their IDs, types, line counts, and last activity timestamps. Use this to discover which agents are available to monitor.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(HashMap::new()),
                required: None,
            },
        },
        ToolDefinition {
            name: "check_agent_status".to_string(),
            description: "Check the current status of a specific agent by its pane or agent ID. Returns connection status, last activity time, output line count, and whether the agent is waiting for input.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("agent_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The agent or pane ID to check status for.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["agent_id".to_string()]),
            },
        },
        ToolDefinition {
            name: "create_execution_plan".to_string(),
            description: "Create a structured execution plan before dispatching any agents. You MUST call this tool before launching agents for any non-trivial task. Each step must have a DISTINCT task_prompt tailored to what that specific agent should do. Never give the same prompt to multiple agents.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("goal".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The high-level goal this plan achieves.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("reasoning".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Your reasoning for why this plan structure was chosen.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("steps".to_string(), ToolPropertySchema {
                        prop_type: "array".to_string(),
                        description: None,
                        r#enum: None,
                        items: Some(Box::new(ToolPropertySchema {
                            prop_type: "object".to_string(),
                            description: None,
                            items: None,
                            r#enum: None,
                        })),
                    });
                    props
                }),
                required: Some(vec!["goal".to_string(), "reasoning".to_string(), "steps".to_string()]),
            },
        },
        ToolDefinition {
            name: "dispatch_plan_step".to_string(),
            description: "Launch an agent to execute a specific step from the active execution plan. The agent receives the step-specific task_prompt. Use this instead of launch_builtin_agent when executing a plan.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("step_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The step ID from the execution plan to dispatch.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["step_id".to_string()]),
            },
        },
        ToolDefinition {
            name: "prompt_agent".to_string(),
            description: "Send a specific prompt or instruction to an already-running agent pane. Use this to give follow-up instructions, ask clarifying questions, or re-direct an agent.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("pane_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The pane ID of the running agent.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("prompt".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The prompt or instruction to send to the agent.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["pane_id".to_string(), "prompt".to_string()]),
            },
        },
        ToolDefinition {
            name: "ask_user".to_string(),
            description: "Ask the user a clarifying question with selectable options. Use this when you need user input to proceed — choosing between approaches, confirming scope, selecting preferences. The user clicks an option and you immediately continue.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("question".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The question to ask.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("options".to_string(), ToolPropertySchema {
                        prop_type: "array".to_string(),
                        description: Some("Available choices (2-5 options). User can also type a custom response.".to_string()),
                        r#enum: None,
                        items: Some(Box::new(ToolPropertySchema {
                            prop_type: "object".to_string(),
                            description: None,
                            items: None,
                            r#enum: None,
                        })),
                    });
                    props
                }),
                required: Some(vec!["question".to_string(), "options".to_string()]),
            },
        },
        ToolDefinition {
            name: "evaluate_results".to_string(),
            description: "Evaluate the results of an execution plan. Read agent outputs for each completed step and assess whether the goal was achieved. This tool records the evaluation and determines next action.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("plan_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The plan ID to evaluate.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("overall_status".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Your assessment of the overall plan outcome.".to_string()),
                        items: None,
                        r#enum: Some(vec!["success".to_string(), "partial_success".to_string(), "failure".to_string(), "needs_replanning".to_string()]),
                    });
                    props.insert("step_evaluations".to_string(), ToolPropertySchema {
                        prop_type: "array".to_string(),
                        description: None,
                        r#enum: None,
                        items: Some(Box::new(ToolPropertySchema {
                            prop_type: "object".to_string(),
                            description: None,
                            items: None,
                            r#enum: None,
                        })),
                    });
                    props.insert("next_action".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("What to do next based on the evaluation.".to_string()),
                        items: None,
                        r#enum: Some(vec!["done".to_string(), "replan".to_string(), "retry_steps".to_string(), "escalate_to_user".to_string()]),
                    });
                    props.insert("reasoning".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Your reasoning for this evaluation.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec![
                    "plan_id".to_string(),
                    "overall_status".to_string(),
                    "step_evaluations".to_string(),
                    "next_action".to_string(),
                    "reasoning".to_string(),
                ]),
            },
        },
        ToolDefinition {
            name: "kanban_list_tasks".to_string(),
            description: "List all tasks on the active workspace's Kanban board.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some(HashMap::new()),
                required: None,
            },
        },
        ToolDefinition {
            name: "kanban_create_task".to_string(),
            description: "Create a new Kanban task in the specified workspace.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("title".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Title of the task.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("description".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional description of the task.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("status".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Status of the task. Must be one of: 'todo', 'in_progress', 'in_review', 'complete'.".to_string()),
                        items: None,
                        r#enum: Some(vec!["todo".to_string(), "in_progress".to_string(), "in_review".to_string(), "complete".to_string()]),
                    });
                    props.insert("space_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The workspace (space) ID to create the task in.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["title".to_string(), "space_id".to_string()]),
            },
        },
        ToolDefinition {
            name: "kanban_update_task".to_string(),
            description: "Update an existing Kanban task.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("task_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The ID of the task to update.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("title".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional new title for the task.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("description".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional new description for the task.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("status".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Optional new status. Must be one of: 'todo', 'in_progress', 'in_review', 'complete'.".to_string()),
                        items: None,
                        r#enum: Some(vec!["todo".to_string(), "in_progress".to_string(), "in_review".to_string(), "complete".to_string()]),
                    });
                    props
                }),
                required: Some(vec!["task_id".to_string()]),
            },
        },
        ToolDefinition {
            name: "kanban_delete_task".to_string(),
            description: "Delete a Kanban task by its ID.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("task_id".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The ID of the task to delete.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["task_id".to_string()]),
            },
        },
        ToolDefinition {
            name: "fs_read_file".to_string(),
            description: "Read the contents of a file from the workspace.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("path".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Path of the file to read (relative to workspace root or absolute).".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["path".to_string()]),
            },
        },
        ToolDefinition {
            name: "fs_list_dir".to_string(),
            description: "List the contents of a directory in the workspace.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("path".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("Path of the directory to list (relative to workspace root or absolute).".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["path".to_string()]),
            },
        },
        ToolDefinition {
            name: "fs_search".to_string(),
            description: "Search files in the workspace using ripgrep.".to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: Some({
                    let mut props = HashMap::new();
                    props.insert("pattern".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The regex or literal pattern to search for.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props.insert("path".to_string(), ToolPropertySchema {
                        prop_type: "string".to_string(),
                        description: Some("The directory path to search in.".to_string()),
                        items: None,
                        r#enum: None,
                    });
                    props
                }),
                required: Some(vec!["pattern".to_string(), "path".to_string()]),
            },
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
    pub parameters: OpenAIParameters,
}

/// Parameters for an OpenAI tool function.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAIParameters {
    #[serde(rename = "type")]
    pub schema_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<HashMap<String, ToolPropertySchema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<Vec<String>>,
}

/// Convert the orchestrator tools list into OpenAI function-calling format.
pub fn to_openai_tools() -> Vec<OpenAITool> {
    orchestrator_tools()
        .into_iter()
        .map(|t| OpenAITool {
            tool_type: "function".to_string(),
            function: OpenAIFunction {
                name: t.name,
                description: t.description,
                parameters: OpenAIParameters {
                    schema_type: "object".to_string(),
                    properties: t.input_schema.properties,
                    required: t.input_schema.required,
                },
            },
        })
        .collect()
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
    #[allow(dead_code)]
    notification_service: Arc<NotificationService>,
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
        notification_service: Arc<NotificationService>,
        plan_manager: Arc<PlanManager>,
        agent_comms: Arc<AgentComms>,
        event_sender: Arc<dyn ToolEventSender>,
        store: Arc<athena_store::KeyValueStore>,
    ) -> Self {
        let kanban_backend = KanbanBackend::new(Arc::clone(&store));
        Self {
            output_buffer,
            notification_service,
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

        let options = crate::types::SearchOptions {
            pattern: pattern.to_string(),
            path: validated.to_string_lossy().to_string(),
            glob: None,
            case_sensitive: false,
            max_results: Some(50),
            context_lines: Some(2),
        };

        match crate::search::search_code_sync(&options) {
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
            Arc::new(NotificationService::new()),
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
}
