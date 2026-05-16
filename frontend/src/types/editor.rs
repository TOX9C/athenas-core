use serde::{Deserialize, Serialize};
use strum::{Display, EnumString};

/// Known file types for the editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, EnumString, Display)]
#[strum(serialize_all = "lowercase")]
pub enum FileType {
    TypeScript,
    TypeScriptReact,
    JavaScript,
    JavaScriptReact,
    Json,
    Markdown,
    Css,
    Scss,
    Html,
    Yaml,
    Toml,
    Rust,
    Python,
    Go,
    Shell,
    Plain,
}

/// Position of the cursor in an editor buffer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CursorPosition {
    pub line: usize,
    pub column: usize,
}

/// A file node in the project tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileNode {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<FileNode>>,
}

/// An open file in the editor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditorFile {
    pub path: String,
    pub content: String,
    pub language: String,
    pub is_dirty: bool,
    pub cursor_position: CursorPosition,
}

/// State of the editor panel.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct EditorState {
    pub open_files: Vec<EditorFile>,
    pub active_file_index: Option<usize>,
    pub file_tree: Vec<FileNode>,
}
