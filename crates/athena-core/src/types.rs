use serde::{Deserialize, Serialize};

use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;

/// Shared event-emitter type used across crates for forwarding events to the frontend.
pub type EventEmitter = Arc<Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>;

/// Request-scoped events emitted while Athena is answering a message.
/// The request ID prevents late chunks from a cancelled or superseded turn
/// from mutating the active conversation in the frontend.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AthenaStreamEvent {
    Started {
        request_id: String,
        session_id: String,
    },
    Delta {
        request_id: String,
        text: String,
    },
    Status {
        request_id: String,
        message: String,
    },
    Completed {
        request_id: String,
        text: String,
    },
    Error {
        request_id: String,
        message: String,
        cancelled: bool,
        /// True when the provider reports the configured model as retired or
        /// removed (410 Gone, or a model-scoped 404). The frontend uses this
        /// to open model selection instead of just showing the error bubble.
        #[serde(default)]
        model_unavailable: bool,
    },
}

/// Callback used by the Tauri adapter to forward stream events.
pub type StreamEmitter = Arc<dyn Fn(AthenaStreamEvent) + Send + Sync>;

/// Batch size (bytes) and staleness window for coalesced Delta events.
const DELTA_FLUSH_BYTES: usize = 256;
const DELTA_FLUSH_INTERVAL: std::time::Duration = std::time::Duration::from_millis(16);

/// Amortizes per-chunk stream Delta events into fewer, larger events.
///
/// Providers deliver assistant text as many tiny SSE fragments (often
/// 1–10 chars); emitting one event per fragment saturates the IPC bridge
/// and the frontend scheduler. Batches flush when they reach
/// [`DELTA_FLUSH_BYTES`] or when the pending batch is at least
/// [`DELTA_FLUSH_INTERVAL`] old at the next push — whichever comes first —
/// bounding the event rate near `throughput / DELTA_FLUSH_BYTES` without
/// adding threads or delaying text beyond one inter-chunk gap.
///
/// Ordering invariant: Delta pushes hold the internal lock while appending,
/// and every non-Delta event MUST go through [`StreamDeltaCoalescer::emit`],
/// which flushes any pending batch before emitting. Because all emissions
/// happen under the same lock in arrival order, downstream observers see
/// Deltas interleaved with Status/Completed/Error exactly as produced.
/// Switching to a different request ID also flushes, so late events from a
/// superseded turn cannot bleed into a newer turn's batch.
#[derive(Default)]
pub struct StreamDeltaCoalescer {
    inner: Mutex<CoalescerInner>,
}

#[derive(Default)]
struct CoalescerInner {
    /// Swapped by `set_stream_emitter`; `None` discards all events.
    emit: Option<StreamEmitter>,
    /// Pending Delta batch: (request_id, accumulated text, first-push time).
    pending: Option<(String, String, std::time::Instant)>,
}

impl StreamDeltaCoalescer {
    /// Buffer one Delta fragment, flushing the batch when it is large
    /// enough or stale relative to [`DELTA_FLUSH_INTERVAL`].
    ///
    /// Ordering: callers MUST serialize `push`/`emit` for a given stream
    /// (stream_message holds the orchestrator's conversation_lock, which
    /// already serializes every turn). This type does not order concurrent
    /// pushes from different threads.
    pub fn push(&self, request_id: &str, text: &str) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if inner.emit.is_none() {
            return; // Nobody listens; do not buffer (matches emit_stream).
        }
        let mut flush_now: Option<(String, String)> = None;
        match &mut inner.pending {
            Some((id, buffer, opened_at)) if id == request_id => {
                buffer.push_str(text);
                if buffer.len() >= DELTA_FLUSH_BYTES || opened_at.elapsed() >= DELTA_FLUSH_INTERVAL
                {
                    if let Some((id, buffer, _)) = inner.pending.take() {
                        flush_now = Some((id, buffer));
                    }
                }
            }
            _ => {
                // A batch from another request ID (or none): flush the
                // foreign batch first, then open a fresh one with this
                // fragment so late chunks never merge across turns.
                if let Some((id, buffer, _)) = inner.pending.take() {
                    flush_now = Some((id, buffer));
                }
                inner.pending = Some((
                    request_id.to_string(),
                    text.to_string(),
                    std::time::Instant::now(),
                ));
            }
        }
        let emitter = inner.emit.clone();
        drop(inner);
        if let Some((request_id, text)) = flush_now {
            Self::call(&emitter, AthenaStreamEvent::Delta { request_id, text });
        }
    }

    /// Flush any pending batch, then emit a non-Delta event.
    pub fn emit(&self, event: AthenaStreamEvent) {
        let (batch, emitter) = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (inner.pending.take(), inner.emit.clone())
        };
        if let Some((request_id, text, _)) = batch {
            Self::call(&emitter, AthenaStreamEvent::Delta { request_id, text });
        }
        Self::call(&emitter, event);
    }

    /// Replace the underlying emitter. Any pending batch is flushed with
    /// the outgoing emitter first so no text is lost across the swap.
    pub fn set_emitter(&self, emit: Option<StreamEmitter>) {
        let (batch, outgoing) = {
            let mut inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            let batch = inner.pending.take();
            let outgoing = inner.emit.clone();
            inner.emit = emit;
            (batch, outgoing)
        };
        if let Some((request_id, text, _)) = batch {
            Self::call(&outgoing, AthenaStreamEvent::Delta { request_id, text });
        }
    }

    fn call(emitter: &Option<StreamEmitter>, event: AthenaStreamEvent) {
        if let Some(emit) = emitter {
            emit(event);
        }
    }
}

/// Handle for cancelling one in-flight assistant request.
#[derive(Clone)]
pub struct AthenaRequest {
    pub request_id: String,
    pub cancel: CancellationToken,
}

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
    #[error(
        "The LLM API returned an error ({status}). Check your API key and base URL in settings."
    )]
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
    /// The provider says the configured model is retired/removed (HTTP 410,
    /// or a model-scoped 404). Kept distinct from `Generic` so the frontend
    /// can route the user to model selection instead of a bare error message.
    #[error("The selected model is unavailable (HTTP {status}): {detail}. Pick another model in Settings.")]
    ModelUnavailable { status: u16, detail: String },
    /// A generic error with a human-readable message.
    #[error("{0}")]
    Generic(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn small_fragments_coalesce_until_byte_threshold() {
        let count = Arc::new(AtomicUsize::new(0));
        let sink = Arc::clone(&count);
        let coalescer = StreamDeltaCoalescer::default();
        coalescer.set_emitter(Some(Arc::new(move |_event| {
            sink.fetch_add(1, Ordering::SeqCst);
        })));
        // 32 fragments x 10 chars = 320 bytes >= 256: one threshold flush
        // inside the loop; the final fragment stays pending.
        for _ in 0..32 {
            coalescer.push("req", "0123456789");
        }
        assert_eq!(count.load(Ordering::SeqCst), 1);
        coalescer.emit(AthenaStreamEvent::Completed {
            request_id: "req".to_string(),
            text: String::new(),
        });
        assert_eq!(count.load(Ordering::SeqCst), 3); // flushed Delta + Completed
    }

    #[test]
    fn stale_batch_flushes_on_next_push() {
        let count = Arc::new(AtomicUsize::new(0));
        let sink = Arc::clone(&count);
        let coalescer = StreamDeltaCoalescer::default();
        coalescer.set_emitter(Some(Arc::new(move |_event| {
            sink.fetch_add(1, Ordering::SeqCst);
        })));
        coalescer.push("req", "a");
        assert_eq!(count.load(Ordering::SeqCst), 0);
        std::thread::sleep(DELTA_FLUSH_INTERVAL * 2);
        coalescer.push("req", "b");
        // First push flushed (stale); "b" opens a fresh, still-pending batch.
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn foreign_request_id_flushes_before_new_batch() {
        let events = Arc::new(parking_lot::Mutex::<Vec<AthenaStreamEvent>>::default());
        let sink = Arc::clone(&events);
        let coalescer = StreamDeltaCoalescer::default();
        coalescer.set_emitter(Some(Arc::new(move |event| {
            sink.lock().push(event);
        })));
        coalescer.push("old-request", "late chunk");
        coalescer.push("new-request", "first");
        coalescer.emit(AthenaStreamEvent::Completed {
            request_id: "new-request".to_string(),
            text: String::new(),
        });
        let guard = events.lock();
        let ids: Vec<&str> = guard
            .iter()
            .map(|event| match event {
                AthenaStreamEvent::Started { request_id, .. }
                | AthenaStreamEvent::Delta { request_id, .. }
                | AthenaStreamEvent::Status { request_id, .. }
                | AthenaStreamEvent::Completed { request_id, .. }
                | AthenaStreamEvent::Error { request_id, .. } => request_id.as_str(),
            })
            .collect();
        // Order: flushed old-request Delta, flushed new-request Delta,
        // then the Completed event itself.
        assert_eq!(ids, vec!["old-request", "new-request", "new-request"]);
    }

    #[test]
    fn cleared_emitter_discards_without_panic() {
        let coalescer = StreamDeltaCoalescer::default();
        coalescer.push("req", "text"); // no emitter set: dropped silently
        coalescer.set_emitter(None);
        coalescer.push("req", "more");
        coalescer.emit(AthenaStreamEvent::Error {
            request_id: "req".to_string(),
            message: "boom".to_string(),
            cancelled: false,
            model_unavailable: false,
        });
    }
}
