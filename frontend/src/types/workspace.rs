use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Type of AI agent a pane can host.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, EnumString, Display, Default,
)]
#[strum(serialize_all = "lowercase")]
pub enum AgentType {
    #[default]
    Claude,
    Codex,
    Opencode,
    Gemini,
    Qwen,
    Aider,
    Cursor,
    Freebuff,
    Omp,
    Custom,
    Shell,
}

/// Configuration for a custom agent that the user adds via Settings > Agents.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CustomAgent {
    pub id: String,
    pub alias: String,
    pub command: String,
    /// When true, this custom agent is treated as Claude for resume: its
    /// extra command flags are preserved and `--resume <id>` appended, and it
    /// appears in the resume dropdown. `#[serde(default)]` so existing stored
    /// agents (without the field) deserialize to `false`.
    #[serde(default)]
    pub is_claude: bool,
    /// When true, this `is_claude` agent is the default / priority option
    /// in the resume banner dropdown. Only one agent should be priority
    /// at a time. `#[serde(default)]` for backward compat.
    #[serde(default)]
    pub priority: bool,
}

/// Grid layout template for a space.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display, Default, Copy,
)]
pub enum GridTemplate {
    #[default]
    #[strum(serialize = "1x1")]
    X1x1,
    #[strum(serialize = "1x2")]
    X1x2,
    #[strum(serialize = "2x2")]
    X2x2,
    #[strum(serialize = "2x3")]
    X2x3,
    #[strum(serialize = "3x3")]
    X3x3,
    #[strum(serialize = "3x4")]
    X3x4,
    #[strum(serialize = "4x4")]
    X4x4,
}

/// Shell write scope for a sandboxed agent (macOS `sandbox-exec`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ShellPolicy {
    #[default]
    Full,
    /// Agent can read/execute but not write inside its working directory
    /// subtree (best-effort macOS ACL via sandbox-exec).
    ReadOnly,
    /// Deny process-exec AND file writes under cwd (shell only, nothing runs).
    None,
}

/// Per-agent capability profile applied to the spawned command.
/// Default = full access (pre-M5 behavior); narrowing is opt-in per role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleCapabilities {
    #[serde(default)]
    pub shell: ShellPolicy,
    /// When false: spawn strips proxy env AND requests a `deny network*`
    /// sandbox profile (best-effort; apps reading their own proxy env may
    /// still tunnel out).
    #[serde(default = "default_true")]
    pub network: bool,
    /// When false: set `MCP_CONFIG_PATH=/dev/null` markers and strip known
    /// MCP env from the child env (best-effort; CLIs with their own config
    /// paths ignore it). Stored for preset round-tripping; Athena's own MCP
    /// tools are governed by the app-level permission system already.
    #[serde(default = "default_true")]
    pub mcp_tools: bool,
}

fn default_true() -> bool {
    true
}

impl Default for RoleCapabilities {
    /// All-allowed is the safe default: capabilities only ever tighten.
    fn default() -> Self {
        Self { shell: ShellPolicy::Full, network: true, mcp_tools: true }
    }
}

impl RoleCapabilities {
    pub fn shell_str(&self) -> &'static str {
        match self.shell {
            ShellPolicy::Full => "full",
            ShellPolicy::ReadOnly => "readonly",
            ShellPolicy::None => "none",
        }
    }
}

/// Configuration for a single pane within a space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaneConfig {
    pub id: String,
    pub agent_type: AgentType,
    /// Working-directory override. `None` = the space's root directory
    /// (set for swarm agents that run in an isolated git worktree).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Capability scoping for this pane's agent (swarm role policy).
    /// `None` = full access (existing behavior).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<RoleCapabilities>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_agent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bypass_mode: Option<bool>,
    /// Name of the project/workspace associated with this pane.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_name: Option<String>,
    /// Name of the model currently used by this pane's agent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_name: Option<String>,
    /// Resume session ID for agents that support session resumption (e.g. Claude Code).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_id: Option<String>,
    /// Full resume command extracted from PTY output (useful for Shell panes
    /// where the agent was started manually). When present, takes precedence
    /// over `resume_id` for displaying the banner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_cmd: Option<String>,
    /// Whether the user dismissed the resume banner for the current
    /// `resume_id`/`resume_cmd`. Persisted so a dismissed banner stays
    /// hidden across app restarts. A newly captured (different) id resets
    /// this to `Some(false)` so the banner reappears for the new session.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resume_dismissed: Option<bool>,
}

/// A workspace space (tab group) containing panes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Space {
    pub id: String,
    pub name: String,
    pub dir: String,
    pub grid: GridTemplate,
    pub panes: Vec<PaneConfig>,
    pub color: String,
    pub created_at: i64,
    pub last_opened_at: i64,
}
