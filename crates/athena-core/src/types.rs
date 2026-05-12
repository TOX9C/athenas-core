use serde::{Deserialize, Serialize};

/// Represents an image attachment with base64 data and media type.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageData {
    pub base64: String,
    pub media_type: String, // e.g. "image/jpeg", "image/png", "image/gif", "image/webp"
}

/// Supported LLM providers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LLMProvider {
    Anthropic,
    OpenAI,
    #[serde(rename = "nvidia_nim")]
    NvidiaNim,
    Lmstudio,
}

impl std::fmt::Display for LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMProvider::Anthropic => write!(f, "anthropic"),
            LLMProvider::OpenAI => write!(f, "openai"),
            LLMProvider::NvidiaNim => write!(f, "nvidia_nim"),
            LLMProvider::Lmstudio => write!(f, "lmstudio"),
        }
    }
}

/// A single entry in the session history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHistoryEntry {
    pub role: String, // "user" or "assistant"
    pub content: String,
    pub images: Option<Vec<ImageData>>,
}

/// Options for performing a code search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    pub pattern: String,
    pub path: String,
    pub glob: Option<String>,
    pub case_sensitive: bool,
    pub max_results: Option<usize>,
    pub context_lines: Option<usize>,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            path: String::new(),
            glob: None,
            case_sensitive: false,
            max_results: None,
            context_lines: None,
        }
    }
}

/// A single match found during a code search.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub file_path: String,
    pub line_number: u32,
    pub column: u32,
    pub line_text: String,
    pub match_text: String,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

/// Aggregated result of a search operation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub matches: Vec<SearchMatch>,
    pub truncated: bool,
    pub stats: SearchStats,
}

/// Statistics about a search result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    pub files_matched: usize,
    pub total_matches: usize,
}

/// Error type for the orchestrator.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("API key is required. Please set it in Settings.")]
    MissingApiKey,
    #[error("Image attachments are not supported by LM Studio")]
    LmStudioVisionNotSupported,
    #[error("JSON serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),
    #[error("{0}")]
    Generic(String),
}
