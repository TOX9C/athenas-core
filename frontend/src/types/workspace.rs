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
    Custom,
    Shell,
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

/// Configuration for a single pane within a space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct PaneConfig {
    pub id: String,
    pub agent_type: AgentType,
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
