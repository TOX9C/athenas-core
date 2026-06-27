use serde::{Deserialize, Serialize};

use std::sync::{Arc, Mutex};

/// Shared event-emitter type used across crates for forwarding events to the frontend.
pub type EventEmitter = Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>;

/// Represents an image attachment with base64 data and media type.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ImageData {
    pub base64: String,
    pub media_type: String, // e.g. "image/jpeg", "image/png", "image/gif", "image/webp"
}

/// Supported LLM providers for the Athena orchestrator.
///
/// Each provider has different API endpoints and message formats.
/// The orchestrator handles these differences internally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LLMProvider {
    /// Anthropic Claude via the Messages API.
    Anthropic,
    /// OpenAI via the Chat Completions API. Also compatible with any OpenAI-compatible endpoint.
    OpenAI,
    /// NVIDIA NIM hosted models via the integrate.api.nvidia.com endpoint.
    #[serde(rename = "nvidia_nim")]
    NvidiaNim,
    /// LM Studio local server. Requires a running LM Studio instance.
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

/// A single entry in the session history, representing one turn in a conversation.
///
/// Used to restore conversation context when switching between sessions
/// or when the app restarts.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SessionHistoryEntry {
    /// The role of the message sender: `"user"` or `"assistant"`.
    pub role: String,
    /// The text content of the message.
    pub content: String,
    /// Optional image attachments included with the message.
    pub images: Option<Vec<ImageData>>,
}

/// Options for performing a code search via ripgrep.
///
/// Used by both the `search_code` and `search_files` functions,
/// as well as the MCP `code_search` and `search_files` tools.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SearchOptions {
    /// The regex pattern to search for.
    pub pattern: String,
    /// The directory to search within.
    pub path: String,
    /// Optional glob pattern to filter files (e.g., `*.rs`).
    pub glob: Option<String>,
    /// Whether to perform case-sensitive matching.
    pub case_sensitive: bool,
    /// Maximum number of results to return. `None` means no limit.
    pub max_results: Option<usize>,
    /// Number of context lines to include before and after each match.
    pub context_lines: Option<usize>,
}

impl SearchOptions {
    /// Maximum allowed value for `context_lines` (prevents ripgrep OOM).
    pub const MAX_CONTEXT_LINES: usize = 100;
    /// Maximum allowed value for `max_results` (bounds memory usage).
    pub const MAX_RESULTS: usize = 5000;

    /// Cap `context_lines` and `max_results` to safe upper bounds, and
    /// escape any leading `-` in `pattern` to prevent argument injection
    /// into ripgrep.
    ///
    /// Defense in depth: the actual `rg` invocation already passes
    /// `pattern` after a `--` end-of-options marker, but escaping here
    /// keeps the option safe if it is ever reused in a path without
    /// that separator.
    pub fn validate(&mut self) {
        if let Some(ctx) = self.context_lines {
            self.context_lines = Some(ctx.min(Self::MAX_CONTEXT_LINES));
        }
        if let Some(max) = self.max_results {
            self.max_results = Some(max.min(Self::MAX_RESULTS));
        }
        if self.pattern.starts_with('-') {
            // Prefix with a backslash so ripgrep treats the dash literally
            // (e.g. `--version` matches the literal string `--version`).
            self.pattern = format!("\\{}", self.pattern);
        }
    }
}

/// A single match found during a code search.
///
/// Contains the matched line plus optional context lines before and after.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    /// Path to the file containing the match, relative to the search root.
    pub file_path: String,
    /// 1-based line number of the match.
    pub line_number: u32,
    /// 1-based column where the match starts.
    pub column: u32,
    /// The full text of the matched line.
    pub line_text: String,
    /// The specific text that matched the pattern.
    pub match_text: String,
    /// Lines before the match, if `context_lines` was specified.
    pub context_before: Vec<String>,
    /// Lines after the match, if `context_lines` was specified.
    pub context_after: Vec<String>,
}

/// Aggregated result of a search operation.
///
/// Contains all matches plus summary statistics.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    /// Individual match entries.
    pub matches: Vec<SearchMatch>,
    /// Whether results were truncated due to `max_results`.
    pub truncated: bool,
    /// Summary statistics about the search.
    pub stats: SearchStats,
}

/// Statistics about a search result.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct SearchStats {
    /// Number of unique files that contained matches.
    pub files_matched: usize,
    /// Total number of individual matches found.
    pub total_matches: usize,
}

/// Error type for the Athena orchestrator.
///
/// Returned by `send_message`, `send_anthropic`, and `send_openai`
/// when an LLM API request fails.
#[derive(Debug, thiserror::Error)]
pub enum OrchestratorError {
    /// The underlying HTTP request to the LLM API failed.
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    /// No API key was configured for the selected provider.
    #[error("API key is required. Please set it in Settings.")]
    MissingApiKey,
    /// Image attachments were requested but the provider (LM Studio) does not support them.
    #[error("Image attachments are not supported by LM Studio")]
    LmStudioVisionNotSupported,
    /// JSON serialization or deserialization failed.
    #[error("JSON serialization failed: {0}")]
    SerializationError(#[from] serde_json::Error),
    /// ------------------------------------
    /// Error Taxonomy (Phase 4)
    /// ------------------------------------
    ///
    /// A tool executed but the operation failed (file not found, agent already dead).
    #[error("I couldn't {action} — {reason}.")]
    ToolFailure { action: String, reason: String },
    /// The LLM API returned a non-200, rate limit, invalid key, or malformed JSON.
    #[error("The LLM API returned an error ({status}). Check your API key and base URL in settings.")]
    LLMApiFailure { status: u16 },
    /// Two operations raced on the same workspace / agent state.
    #[error("Two operations conflicted on {resource}. I cancelled {operation}. Try again.")]
    StateConflict { resource: String, operation: String },
    /// The user cancelled a pending confirmation. No error to report.
    #[error("Cancelled by user")]
    UserCancellation,
    /// A tool call exceeded the configured timeout.
    #[error("The {tool_name} call timed out after {timeout}s. {partial_result}")]
    ToolTimeout {
        tool_name: String,
        timeout: u32,
        partial_result: String,
    },
    /// A generic error with a human-readable message.
    #[error("{0}")]
    Generic(String),
}
