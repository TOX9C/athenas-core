use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Category a command belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "snake_case")]
pub enum CommandCategory {
    Workspace,
    Panel,
    Athena,
    Terminal,
    File,
    Settings,
    Navigation,
}

/// A command palette entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub label: String,
    pub category: CommandCategory,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keywords: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
}

/// State of the command palette overlay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct CommandPaletteState {
    pub open: bool,
    pub query: String,
    pub selected_index: usize,
    pub filtered: Vec<Command>,
}
