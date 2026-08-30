use crate::notification::NotificationType as NotifType;
use crate::tool_executor::{to_openai_tools, ToolExecutor};
use crate::types::*;
use secrecy::ExposeSecret;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

// Session persistence types
use athena_store::MessageRole as StoreMessageRole;
use athena_store::SessionMessage as StoreMessage;

/// Default model used when no provider config is set (env-key fallback path).
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";

#[path = "orchestrator_messages.rs"]
mod orchestrator_messages;
#[path = "orchestrator_session.rs"]
mod orchestrator_session;
use orchestrator_messages::{AnthropicMessage, OpenAIMessage};
#[path = "orchestrator_snapshot.rs"]
mod orchestrator_snapshot;
#[path = "orchestrator_stream.rs"]
mod orchestrator_stream;
/// Stable Anthropic API version header value.
#[path = "orchestrator_support.rs"]
mod orchestrator_support;
#[path = "orchestrator_title.rs"]
mod orchestrator_title;
#[path = "orchestrator_tool_loop.rs"]
mod orchestrator_tool_loop;
pub use orchestrator_support::ProviderConfig;
use orchestrator_support::{
    build_anthropic_content, build_openai_content, estimate_tokens, json_to_tool_input, RateLimiter,
};
// `sanitize_error_message` is the crate's canonical content-level redactor.
// Re-exported publicly so the Tauri logging backend (`src-tauri/src/main.rs`)
// can reuse it as a log-message safety net instead of duplicating the
// credential-matching regex set (duplicated regex sets are how divergent
// redaction drifts). `validate_base_url` stays crate-visible only.
pub use orchestrator_support::sanitize_error_message;
pub(crate) use orchestrator_support::validate_base_url;

const ANTHROPIC_VERSION: &str = "2023-06-01";

/// Backoff schedule (ms) between title-summarization retries. ~7s ceiling.
const DEFAULT_BACKOFF_DELAYS_MS: &[u64] = &[1000, 2000, 4000];

/// Maximum output tokens per LLM request. The orchestrator reads agent
/// terminal output and writes multi-step plans, so this needs headroom.
pub const MAX_OUTPUT_TOKENS: u32 = 8192;

/// System prompt defining Athena as a workspace-aware orchestrator.
/// This prompt is injected into every LLM conversation unless the user
/// explicitly overrides it in their provider configuration. The live state
/// snapshot (see `build_app_state_snapshot`) is appended to it per request.
pub const SYSTEM_PROMPT: &str = r#"You are Athena, the orchestrator of a developer desktop environment. You manage background agents, terminals, an execution-plan engine, and a Kanban board on the user's behalf, using the tools provided to you.

## State snapshot
Each request ends with a STATE SNAPSHOT showing the active workspace, the currently running agent panes (with their pane IDs), and the active execution plan. Treat it as ground truth. The pane IDs you pass to tools come from this snapshot — never invent them, and don't call a tool just to re-discover what the snapshot already shows.

## Launching work
- Use launch_builtin_agent for the standard agents (claude, codex, opencode, gemini, shell). Omit task_prompt to open an interactive shell.
- Use launch_custom_agent only for a user-defined CLI agent.
- For any multi-step task, call create_execution_plan first, then dispatch_plan_step for each step, then evaluate_results. Give every step a unique id, a self-contained description (the description is what the agent receives as its prompt), and an agent_type (claude, codex, opencode, gemini, or shell — use shell to run the description as a shell command; omit to default to claude).
- Never launch an agent the user didn't ask for. If asked for N agents but only M exist or are defined, use ask_user to resolve the gap before launching.

## Talking to agents & terminals
- prompt_agent sends an instruction to one already-running agent.
- run_command_in_terminals runs a shell command in already-open panes.

## Confirmation
Proceed autonomously for read-only actions and for launching/dispatching agents the user requested. Confirm with the user first ONLY for destructive actions: close_terminals and kanban_delete_task.

## Asking the user
Use ask_user when you need a decision to proceed. Each option must have a short `label` (shown on the button) and may include a longer `description`.

## Reporting on agents
The snapshot only lists agents in the user's CURRENT space — report on those, never agents from other spaces. When asked what the agents/terminals are doing, read each one's recent output (read_agent_output) and reply with ONE short line per agent: its name/type and a few-word summary of what it's doing right now (e.g. "Shell 1 (claude): cleaning up dead code", "Shell 2: idle at prompt"). Do not paste raw terminal output.

## Response style
Be terse. Lead with the answer — no preamble, no recap of the question. Prefer a short bullet list or a few sentences. Never produce multi-paragraph walls of text unless the user explicitly asks for detail."#;

/// The Athena orchestrator that dispatches messages to LLM providers
/// and executes tool calls via the `ToolExecutor`.
/// Pending auto-save slot: (session id, JoinHandle of the detached save task).
/// A reschedule for the SAME session aborts and replaces the pending task;
/// a reschedule for a DIFFERENT session leaves the old task running detached.
type AutoSaveSlot = (String, tokio::task::JoinHandle<()>);

pub struct AthenaOrchestrator {
    anthropic_messages: Arc<parking_lot::Mutex<Vec<AnthropicMessage>>>,
    openai_messages: Arc<parking_lot::Mutex<Vec<OpenAIMessage>>>,
    current_session_id: Arc<parking_lot::Mutex<Option<String>>>,
    tool_executor: Option<Arc<parking_lot::RwLock<ToolExecutor>>>,
    http_client: reqwest::Client,
    /// Base URL for the Anthropic Messages API. Overridable in tests so the
    /// LLM calls can be mocked with wiremock.
    anthropic_base_url: String,
    provider_config: Arc<parking_lot::Mutex<Option<ProviderConfig>>>,
    rate_limiter: RateLimiter,
    /// Reference to the output buffer for reading agent pane state.
    output_buffer: Option<Arc<crate::output_buffer::OutputBuffer>>,
    /// Reference to the plan manager for reading the active execution plan.
    plan_manager: Option<Arc<crate::plan_manager::PlanManager>>,
    /// Reference to the agent comms service for reading active sessions.
    agent_comms: Option<Arc<crate::agent_comms::AgentComms>>,
    /// The name of the currently active workspace (updated at runtime).
    workspace_name: Arc<parking_lot::Mutex<Option<String>>>,
    /// Optional session store for persisting conversations.
    session_store: Option<Arc<athena_store::SessionStore>>,
    /// Optional key-value store, used to resolve the active space and its
    /// panes so the state snapshot can be scoped to the current workspace.
    kv_store: Option<Arc<athena_store::KeyValueStore>>,
    /// TTL cache for the app-state snapshot. Rebuilding the snapshot touches
    /// the output buffer, plan manager, agent comms, and active-space reads,
    /// so caching for ~1 s avoids redundant work on back-to-back chat turns.
    snapshot_cache: parking_lot::Mutex<Option<(String, Instant)>>,
    /// Optional notification service for pushing status alerts.
    notification_service: Option<Arc<crate::notification::NotificationService>>,
    /// Active request cancellation handles, keyed by request ID.
    active_requests: Arc<parking_lot::Mutex<HashMap<String, tokio_util::sync::CancellationToken>>>,
    /// Serializes conversation mutation and provider turns. The legacy API
    /// keeps process-global histories, so concurrent turns must not interleave.
    conversation_lock: Arc<tokio::sync::Mutex<()>>,
    /// Optional callback used by the application adapter to emit stream events.
    stream_emitter: Arc<parking_lot::Mutex<Option<crate::types::StreamEmitter>>>,
    /// Coalesces per-chunk Delta events into fewer, larger events before
    /// they reach `stream_emitter`.
    stream_coalescer: crate::types::StreamDeltaCoalescer,
    /// Event sender handle used to cancel blocking UI tools without waiting on
    /// the executor mutex.
    tool_event_sender:
        Arc<parking_lot::Mutex<Option<Arc<dyn crate::tool_executor::ToolEventSender>>>>,
    /// Debounced auto-save: `try_auto_save` schedules the store write ~2 s
    /// out instead of synchronously per turn, so rapid consecutive turns
    /// coalesce into one full-file rewrite. The slot carries the session id
    /// the save is for: a reschedule for the SAME session aborts and
    /// replaces the pending task (newer snapshot wins); a reschedule for a
    /// DIFFERENT session leaves the old task running detached — its
    /// snapshot is self-contained and must still be persisted.
    auto_save_task: Arc<parking_lot::Mutex<Option<AutoSaveSlot>>>,
}

impl Default for AthenaOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl AthenaOrchestrator {
    /// Build the shared HTTP client used for every provider call.
    ///
    /// Panics only if the system TLS stack fails to initialize — a
    /// startup-fatal condition in every environment this app supports.
    fn build_http_client() -> reqwest::Client {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("HTTP client init failed (system TLS unavailable)")
    }

    /// Create a new orchestrator without a tool executor.
    ///
    /// Tool calls from the LLM will still be detected but will return
    /// an error indicating no executor is configured.
    pub fn new() -> Self {
        Self {
            anthropic_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            openai_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            current_session_id: Arc::new(parking_lot::Mutex::new(None)),
            tool_executor: None,
            http_client: Self::build_http_client(),
            anthropic_base_url: "https://api.anthropic.com/v1".to_string(),
            provider_config: Arc::new(parking_lot::Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000), // 1 second minimum between requests
            output_buffer: None,
            plan_manager: None,
            agent_comms: None,
            workspace_name: Arc::new(parking_lot::Mutex::new(None)),
            session_store: None,
            kv_store: None,
            snapshot_cache: parking_lot::Mutex::new(None),
            notification_service: None,
            active_requests: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            conversation_lock: Arc::new(tokio::sync::Mutex::new(())),
            stream_emitter: Arc::new(parking_lot::Mutex::new(None)),
            stream_coalescer: crate::types::StreamDeltaCoalescer::default(),
            tool_event_sender: Arc::new(parking_lot::Mutex::new(None)),
            auto_save_task: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Test-only constructor that points the Anthropic base URL at a mock
    /// server (e.g. wiremock). Not compiled into release paths that matter,
    /// but available to `#[cfg(test)]` callers.
    #[cfg(test)]
    pub fn new_for_test(anthropic_base_url: String) -> Self {
        let mut orch = Self::new();
        orch.anthropic_base_url = anthropic_base_url;
        orch
    }

    /// Create an orchestrator with service references for building
    /// the app state snapshot injected before every LLM call.
    pub fn with_context(
        executor: Arc<parking_lot::RwLock<ToolExecutor>>,
        output_buffer: Arc<crate::output_buffer::OutputBuffer>,
        plan_manager: Arc<crate::plan_manager::PlanManager>,
        agent_comms: Arc<crate::agent_comms::AgentComms>,
        session_store: Option<Arc<athena_store::SessionStore>>,
        kv_store: Option<Arc<athena_store::KeyValueStore>>,
        notification_service: Option<Arc<crate::notification::NotificationService>>,
    ) -> Self {
        let tool_event_sender = executor.read().event_sender_handle();
        Self {
            anthropic_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            openai_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            current_session_id: Arc::new(parking_lot::Mutex::new(None)),
            tool_executor: Some(executor),
            http_client: Self::build_http_client(),
            anthropic_base_url: "https://api.anthropic.com/v1".to_string(),
            provider_config: Arc::new(parking_lot::Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000),
            output_buffer: Some(output_buffer),
            plan_manager: Some(plan_manager),
            agent_comms: Some(agent_comms),
            workspace_name: Arc::new(parking_lot::Mutex::new(None)),
            session_store,
            kv_store,
            snapshot_cache: parking_lot::Mutex::new(None),
            notification_service,
            active_requests: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            conversation_lock: Arc::new(tokio::sync::Mutex::new(())),
            stream_emitter: Arc::new(parking_lot::Mutex::new(None)),
            stream_coalescer: crate::types::StreamDeltaCoalescer::default(),
            tool_event_sender: Arc::new(parking_lot::Mutex::new(Some(tool_event_sender))),
            auto_save_task: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Create a new orchestrator with a tool executor wired in.
    ///
    /// When the LLM returns `tool_use` / `tool_calls`, the executor
    /// dispatches them and the results are fed back into the conversation
    /// loop automatically.
    pub fn new_with_executor(executor: Arc<parking_lot::RwLock<ToolExecutor>>) -> Self {
        let tool_event_sender = executor.read().event_sender_handle();
        Self {
            anthropic_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            openai_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            current_session_id: Arc::new(parking_lot::Mutex::new(None)),
            tool_executor: Some(executor),
            http_client: Self::build_http_client(),
            anthropic_base_url: "https://api.anthropic.com/v1".to_string(),
            provider_config: Arc::new(parking_lot::Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000), // 1 second minimum between requests
            output_buffer: None,
            plan_manager: None,
            agent_comms: None,
            workspace_name: Arc::new(parking_lot::Mutex::new(None)),
            session_store: None,
            kv_store: None,
            snapshot_cache: parking_lot::Mutex::new(None),
            notification_service: None,
            active_requests: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            conversation_lock: Arc::new(tokio::sync::Mutex::new(())),
            stream_emitter: Arc::new(parking_lot::Mutex::new(None)),
            stream_coalescer: crate::types::StreamDeltaCoalescer::default(),
            tool_event_sender: Arc::new(parking_lot::Mutex::new(Some(tool_event_sender))),
            auto_save_task: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Replace or clear the tool executor at runtime.
    pub fn set_tool_executor(&mut self, executor: Option<Arc<parking_lot::RwLock<ToolExecutor>>>) {
        let sender = executor
            .as_ref()
            .map(|value| value.read().event_sender_handle());
        *self.tool_event_sender.lock() = sender;
        self.tool_executor = executor;
    }

    /// Send a notification if the notification service is configured.
    fn notify(
        &self,
        ntype: crate::notification::NotificationType,
        title: impl Into<String>,
        message: impl Into<String>,
    ) {
        if let Some(ref svc) = self.notification_service {
            // Limit notification title to a reasonable length
            let t = title.into();
            let m = message.into();
            let _ = svc.notify(ntype, t, m);
        }
    }

    /// Set the LLM provider configuration.
    pub fn set_provider_config(&self, config: ProviderConfig) {
        *self.provider_config.lock() = Some(config);
    }

    /// Get the current LLM provider configuration, if set.
    pub fn get_provider_config(&self) -> Option<ProviderConfig> {
        self.provider_config.lock().clone()
    }

    /// Set the active workspace name for state snapshot injection.
    pub fn set_workspace_name(&self, name: String) {
        *self.workspace_name.lock() = Some(name);
    }

    /// Estimate token count from a UTF-8 string using the heuristic
    /// 1 token ≈ 4 characters for English.
    fn estimate_tokens(text: &str) -> usize {
        estimate_tokens(text)
    }

    /// Send a message to the configured LLM provider, serializing it with
    /// streamed turns that share the legacy conversation buffers.
    pub async fn send_message(
        &self,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        let _conversation_guard = self.conversation_lock.lock().await;
        self.send_message_locked(text, images).await
    }

    /// Send a legacy turn within a specific session. Session selection and
    /// provider mutation happen under the same lock as the request, avoiding
    /// cross-session auto-save races.
    pub async fn send_message_with_session(
        &self,
        session_id: String,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        let _conversation_guard = self.conversation_lock.lock().await;
        self.set_current_session_id(session_id);
        self.send_message_locked(text, images).await
    }

    async fn send_message_locked(
        &self,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        // Build a fresh app-state snapshot and append it to the SYSTEM prompt
        // for this request. Keeping it out of the persisted user text means the
        // message history doesn't accumulate stale, conflicting snapshots — the
        // model always sees exactly one, current, snapshot.
        let snapshot = self.build_app_state_snapshot();

        let (provider, api_key, model, system_prompt, base_url) = {
            let guard = self.provider_config.lock();
            match guard.as_ref() {
                Some(config) => {
                    let base = if config.system_prompt.trim().is_empty() {
                        SYSTEM_PROMPT
                    } else {
                        config.system_prompt.as_str()
                    };
                    let sp = format!("{}\n\n{}", base, snapshot);
                    (
                        config.provider.clone(),
                        config.api_key().clone(),
                        config.model.clone(),
                        sp,
                        config.base_url.clone(),
                    )
                }
                None => {
                    let api_key = std::env::var("ANTHROPIC_API_KEY")
                        .ok()
                        .ok_or(OrchestratorError::MissingApiKey)?;
                    (
                        LLMProvider::Anthropic,
                        secrecy::SecretString::from(api_key),
                        DEFAULT_ANTHROPIC_MODEL.to_string(),
                        format!("{}\n\n{}", SYSTEM_PROMPT, snapshot),
                        None,
                    )
                }
            }
        };

        if let Some(ref imgs) = images {
            if !imgs.is_empty() && provider == LLMProvider::Lmstudio {
                return Err(OrchestratorError::LmStudioVisionNotSupported);
            }
        }

        if let Some(ref burl) = &base_url {
            validate_base_url(burl)?;
        }

        let resolved_base_url = match &provider {
            LLMProvider::NvidiaNim => {
                Some(base_url.unwrap_or_else(|| "https://integrate.api.nvidia.com/v1".to_string()))
            }
            LLMProvider::OpenAI => {
                // User custom base_url takes precedence over hardcoded default.
                Some(base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string()))
            }
            LLMProvider::Lmstudio => {
                Some(base_url.unwrap_or_else(|| "http://localhost:1234/v1".to_string()))
            }
            LLMProvider::Anthropic => None,
        };

        let result = match provider {
            LLMProvider::NvidiaNim | LLMProvider::OpenAI | LLMProvider::Lmstudio => {
                self.send_openai(
                    api_key,
                    model,
                    system_prompt,
                    text,
                    images,
                    resolved_base_url,
                )
                .await
            }
            LLMProvider::Anthropic => {
                self.send_anthropic(api_key, model, system_prompt, text, images)
                    .await
            }
        };

        if result.is_ok() {
            if let Err(e) = self.flush_auto_save().await {
                log::warn!("Failed to auto-save conversation: {}", e);
            }
        }

        match &result {
            Ok(reply) => {
                self.notify(
                    NotifType::Success,
                    "Athena Ready",
                    reply.chars().take(80).collect::<String>(),
                );
            }
            Err(err) => {
                self.notify(
                    NotifType::Error,
                    "Athena Error",
                    err.to_string().chars().take(80).collect::<String>(),
                );
            }
        }

        result
    }

    /// Send a message using Anthropic's Messages API.
    ///
    /// If the assistant responds with `tool_use` blocks, each one is
    /// dispatched through the `ToolExecutor` and the results are appended
    /// as `tool_result` messages. The loop continues until the assistant
    /// returns a pure text response (no tool calls).
    pub async fn send_anthropic(
        &self,
        api_key: secrecy::SecretString,
        model: String,
        system_prompt: String,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        // Append user message
        {
            let mut msgs = self.anthropic_messages.lock();
            msgs.push(AnthropicMessage {
                role: "user".to_string(),
                content: build_anthropic_content(&text, images.as_deref()),
            });
        }

        let client = &self.http_client;
        let url = "https://api.anthropic.com/v1/messages";
        let tools = to_openai_tools();

        // Build the Anthropic-format tools list from the OpenAI-compatible schema.
        let anthropic_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.function.name,
                    "description": t.function.description,
                    "input_schema": t.function.parameters,
                })
            })
            .collect();

        loop {
            // Enforce rate limiting before each API request
            self.rate_limiter.wait_if_needed().await;

            let messages = {
                let msgs = self.anthropic_messages.lock();
                msgs.clone()
            };

            let mut body = serde_json::json!({
                "model": model,
                "max_tokens": MAX_OUTPUT_TOKENS,
                "system": system_prompt,
                "messages": messages,
                "tools": anthropic_tools,
            });

            // Anthropic requires `tool_choice` to enable tool use.
            body["tool_choice"] = serde_json::json!({"type": "auto"});

            let response = client
                .post(url)
                .header("x-api-key", api_key.expose_secret())
                .header("anthropic-version", ANTHROPIC_VERSION)
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response.text().await.unwrap_or_default();
                let sanitized = sanitize_error_message(&err_text);
                return Err(OrchestratorError::Generic(format!(
                    "Anthropic API error {}: {}",
                    status, sanitized
                )));
            }

            let json: serde_json::Value = response.json().await?;
            let content = json["content"].as_array().ok_or_else(|| {
                OrchestratorError::Generic(
                    "Invalid Anthropic response: no content array".to_string(),
                )
            })?;

            let mut response_text = String::new();
            let mut tool_calls: Vec<serde_json::Value> = Vec::new();

            for block in content {
                let block_type = block["type"].as_str().unwrap_or("");
                if block_type == "text" {
                    if let Some(t) = block["text"].as_str() {
                        response_text.push_str(t);
                    }
                } else if block_type == "tool_use" {
                    tool_calls.push(block.clone());
                }
            }

            // Push assistant response (the full content array, including tool_use blocks)
            {
                let mut msgs = self.anthropic_messages.lock();
                msgs.push(AnthropicMessage {
                    role: "assistant".to_string(),
                    content: serde_json::json!(content),
                });
            }

            if tool_calls.is_empty() {
                return Ok(response_text.trim().to_string());
            }

            // Execute each tool call WITHOUT holding the message lock across
            // the (potentially long, up to 30s for ask_user) await, then push
            // all results at once under a fresh lock. Holding a sync
            // parking_lot::Mutex across `.await` stalls the Tokio worker and
            // serializes the whole runtime against any other task that touches
            // `anthropic_messages` (auto-save, load_conversation, a second
            // send_message).
            let mut tool_results: Vec<serde_json::Value> = Vec::with_capacity(tool_calls.len());
            for tool_call in &tool_calls {
                let tool_use_id = tool_call["id"].as_str().unwrap_or("unknown");
                let tool_name = tool_call["name"].as_str().unwrap_or("unknown");
                let tool_input = &tool_call["input"];

                // No lock held here — execute_tool may block or await.
                let (result_text, is_error) = self.execute_tool(tool_name, tool_input).await;

                let tool_result = if is_error {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "is_error": true,
                        "content": result_text,
                    })
                } else {
                    serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": tool_use_id,
                        "content": result_text,
                    })
                };
                tool_results.push(tool_result);
            }

            // Append all tool results under a single short-lived lock.
            {
                let mut msgs = self.anthropic_messages.lock();
                for tool_result in tool_results {
                    msgs.push(AnthropicMessage {
                        role: "user".to_string(),
                        content: tool_result,
                    });
                }
            }

            // Continue loop: send the tool results back and get the next response.
        }
    }

    /// Send a message using an OpenAI-compatible chat completions API.
    ///
    /// If the assistant responds with `tool_calls`, each one is dispatched
    /// through the `ToolExecutor` and the results are appended as function
    /// response messages. The loop continues until the assistant returns
    /// a pure text response (no tool calls).
    pub async fn send_openai(
        &self,
        api_key: secrecy::SecretString,
        model: String,
        system_prompt: String,
        text: String,
        images: Option<Vec<ImageData>>,
        base_url: Option<String>,
    ) -> Result<String, OrchestratorError> {
        let url = match base_url {
            Some(ref base) => format!("{}/chat/completions", base.trim_end_matches('/')),
            None => "https://api.openai.com/v1/chat/completions".to_string(),
        };

        let client = &self.http_client;
        let tools = to_openai_tools();

        // Build or update messages with system prompt
        {
            let mut msgs = self.openai_messages.lock();
            if msgs.first().is_none_or(|m| m.role != "system") {
                let new_system = OpenAIMessage {
                    role: "system".to_string(),
                    content: serde_json::Value::String(system_prompt),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                };
                if msgs.is_empty() {
                    msgs.push(new_system);
                } else {
                    msgs.insert(0, new_system);
                }
            } else {
                msgs[0].content = serde_json::Value::String(system_prompt);
            }

            msgs.push(OpenAIMessage {
                role: "user".to_string(),
                content: build_openai_content(&text, images.as_deref()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        // Track the index of the initial user message so that on API error we
        // can remove it (and any tool-call round-trip messages after it).
        let user_msg_index: usize = {
            let msgs = self.openai_messages.lock();
            msgs.len() - 1
        };

        loop {
            // Enforce rate limiting before each API request
            self.rate_limiter.wait_if_needed().await;

            let body_messages: Vec<serde_json::Value> = {
                let msgs = self.openai_messages.lock();

                msgs.iter()
                    .map(|m| {
                        let mut obj = serde_json::json!({
                            "role": &m.role,
                            "content": &m.content,
                        });

                        // Attach tool_calls for assistant messages that used tools.
                        if let Some(ref tc) = m.tool_calls {
                            obj["tool_calls"] = tc.clone();
                        }

                        // Attach tool_call_id and name for tool-response messages.
                        if let Some(ref id) = m.tool_call_id {
                            obj["tool_call_id"] = serde_json::Value::String(id.clone());
                        }
                        if let Some(ref n) = m.name {
                            obj["name"] = serde_json::Value::String(n.clone());
                        }

                        obj
                    })
                    .collect()
            };

            let body = serde_json::json!({
                "model": model,
                "max_tokens": MAX_OUTPUT_TOKENS,
                "messages": body_messages,
                "tools": tools,
                "tool_choice": "auto",
                "stream": false,
            });

            let response = client
                .post(&url)
                .header(
                    "Authorization",
                    format!("Bearer {}", api_key.expose_secret()),
                )
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response.text().await.unwrap_or_default();
                // Sanitize provider error bodies before embedding them: provider
                // responses can echo request headers (e.g. `Authorization: Bearer
                // sk-…`) or contain other identifying material. The Anthropic
                // path at ~line 918 already does this; the OpenAI/NvidiaNim/
                // Lmstudio path (which all flow through here) must too.
                let sanitized = sanitize_error_message(&err_text);
                let mut msgs = self.openai_messages.lock();
                msgs.truncate(user_msg_index);
                return Err(OrchestratorError::Generic(format!(
                    "OpenAI API error {}: {}",
                    status, sanitized
                )));
            }

            let json: serde_json::Value = response.json().await?;
            let choice = &json["choices"][0];
            let message = &choice["message"];
            let raw_content = message["content"]
                .as_str()
                .unwrap_or_else(|| {
                    log::warn!("OpenAI response content is not a string, defaulting to empty");
                    ""
                })
                .trim();

            // Check for tool calls in the response.
            let tool_calls_value = message.get("tool_calls");
            let has_tool_calls = tool_calls_value
                .and_then(|v| v.as_array())
                .is_some_and(|a| !a.is_empty());

            // Store assistant message, including tool_calls if present.
            {
                let mut msgs = self.openai_messages.lock();

                let tool_calls_json = if has_tool_calls {
                    tool_calls_value.cloned()
                } else {
                    None
                };

                // OpenAI may return null content when tool_calls are present.
                let content_value = if raw_content.is_empty() && has_tool_calls {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(raw_content.to_string())
                };

                msgs.push(OpenAIMessage {
                    role: "assistant".to_string(),
                    content: content_value,
                    tool_calls: tool_calls_json,
                    tool_call_id: None,
                    name: None,
                });
            }

            if !has_tool_calls {
                return Ok(raw_content.to_string());
            }

            // Execute each tool call and append function response messages.
            let tool_calls_array =
                tool_calls_value.and_then(|v| v.as_array()).ok_or_else(|| {
                    OrchestratorError::Generic(
                        "OpenAI response has tool_calls field but it is not a valid array"
                            .to_string(),
                    )
                })?;

            // Execute each tool call WITHOUT holding the message lock across
            // the (potentially long, up to 30s for ask_user) await, then push
            // all results at once under a fresh lock. Holding a sync
            // parking_lot::Mutex across `.await` stalls the Tokio worker and
            // serializes the whole runtime against any other task that touches
            // `openai_messages` (auto-save, load_conversation, a second
            // send_message).
            let mut tool_responses: Vec<OpenAIMessage> = Vec::with_capacity(tool_calls_array.len());
            for tool_call in tool_calls_array {
                let call_id = tool_call["id"].as_str().unwrap_or("unknown");
                let function = &tool_call["function"];
                let function_name = function["name"].as_str().unwrap_or("unknown");
                let function_args_str = function["arguments"].as_str().unwrap_or("{}");

                // Parse the arguments string into a JSON value.
                let function_args: serde_json::Value = serde_json::from_str(function_args_str)
                    .unwrap_or_else(|e| {
                        log::warn!(
                            "Failed to parse tool call arguments for '{}': {}. \
                             Raw arguments: {}. Using empty object.",
                            function_name,
                            e,
                            function_args_str
                        );
                        serde_json::json!({})
                    });

                // No lock held here — execute_tool may block or await.
                let (result_text, is_error) =
                    self.execute_tool(function_name, &function_args).await;

                let tool_response_content = if is_error {
                    serde_json::json!({
                        "error": result_text,
                    })
                } else {
                    serde_json::Value::String(result_text)
                };

                tool_responses.push(OpenAIMessage {
                    role: "tool".to_string(),
                    content: tool_response_content,
                    tool_calls: None,
                    tool_call_id: Some(call_id.to_string()),
                    name: Some(function_name.to_string()),
                });
            }

            // Append all tool responses under a single short-lived lock.
            {
                let mut msgs = self.openai_messages.lock();
                for resp in tool_responses {
                    msgs.push(resp);
                }
            }

            // Continue loop: send the tool results back and get the next response.
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_comms::AgentComms;
    use crate::output_buffer::OutputBuffer;
    use crate::plan_manager::PlanManager;
    use crate::tool_executor::{ToolEventSender, ToolExecutor};
    use std::sync::Arc;
    use std::time::Duration;

    /// Test event sender whose `ask_user` blocks the calling thread for
    /// `BLOCK_FOR`. Used to verify that the orchestrator's `execute_tool`
    /// does not block the Tokio runtime when the underlying tool does
    /// blocking I/O (here: a synchronous channel receive).
    struct BlockingEventSender;

    impl ToolEventSender for BlockingEventSender {
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
            // Simulate blocking I/O (e.g. fs_read_file on a slow disk, or
            // a long-lived DB query). This call must NOT block the async
            // runtime when dispatched via `execute_tool`.
            std::thread::sleep(Duration::from_secs(2));
            "blocked-answer".to_string()
        }
        fn plan_update(&self, _plan: &crate::plan_manager::ExecutionPlan) {}
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

    fn build_orchestrator_with_blocking_executor() -> AthenaOrchestrator {
        let executor = ToolExecutor::new(
            Arc::new(OutputBuffer::new()),
            Arc::new(PlanManager::new()),
            Arc::new(AgentComms::new()),
            Arc::new(BlockingEventSender),
            Arc::new(athena_store::KeyValueStore::new_empty()),
            None,
        );
        AthenaOrchestrator::new_with_executor(Arc::new(parking_lot::RwLock::new(executor)))
    }

    /// Verify that `execute_tool` does not block the Tokio runtime when
    /// the underlying tool performs blocking work. We run two concurrent
    /// tasks: one calls `execute_tool("ask_user", ...)` which takes ~2s
    /// inside the tool's blocking callback, and the other sleeps for 50ms
    /// and checks the clock. With `spawn_blocking`, the short task should
    /// complete well before the long task.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn execute_tool_does_not_block_async_runtime() {
        let orchestrator = build_orchestrator_with_blocking_executor();

        let input_json = serde_json::json!({
            "question": "Pick one",
            "options": ["a", "b"],
        });

        // Spawn the long-blocking execute_tool in a background task.
        let orch = Arc::new(orchestrator);
        let orch_for_long = Arc::clone(&orch);
        let long_task =
            tokio::spawn(async move { orch_for_long.execute_tool("ask_user", &input_json).await });

        // The runtime should still service other tasks while the long
        // task is in-flight. We give the long task a head start, then
        // run a separate task and assert it completes before the long
        // task finishes.
        tokio::time::sleep(Duration::from_millis(50)).await;
        let started_at = std::time::Instant::now();
        let short_completed = tokio::time::timeout(Duration::from_millis(500), async {
            tokio::time::sleep(Duration::from_millis(100)).await;
        })
        .await
        .is_ok();
        let short_elapsed = started_at.elapsed();

        assert!(
            short_completed,
            "short task timed out — runtime appears to be blocked by execute_tool"
        );
        assert!(
            short_elapsed < Duration::from_millis(400),
            "short task took {:?} (expected <400ms); runtime likely blocked",
            short_elapsed
        );

        // Finally, the long task itself should be cancellable via tokio
        // timeouts (proving the orchestrator is awaiting, not spinning
        // on a synchronous call).
        let long_result = tokio::time::timeout(Duration::from_secs(5), long_task)
            .await
            .expect("long task should finish within 5s (2s sleep + overhead)")
            .expect("long task should not panic");
        assert_eq!(long_result.0, "User selected: blocked-answer");
        assert!(!long_result.1, "ask_user should not be flagged as error");
    }

    /// Verify the no-executor error path returns synchronously (no await
    /// needed beyond what the call itself does) and produces the
    /// documented error message.
    #[tokio::test]
    async fn execute_tool_no_executor_returns_error() {
        let orchestrator = AthenaOrchestrator::new();
        let input = serde_json::json!({});
        let (text, is_error) = orchestrator.execute_tool("any_tool", &input).await;
        assert!(is_error, "no-executor path must be flagged as error");
        assert!(
            text.contains("no tool executor is configured"),
            "unexpected error text: {}",
            text
        );
    }

    /// Verify that `execute_tool` deserializes bad input into a graceful
    /// error rather than panicking.
    #[tokio::test]
    async fn execute_tool_bad_input_is_handled() {
        let orchestrator = build_orchestrator_with_blocking_executor();
        // `ask_user` requires a `question` field; supply an empty object
        // so the executor's `MissingParam` error path runs.
        let input = serde_json::json!({});
        let (text, is_error) = orchestrator.execute_tool("ask_user", &input).await;
        // json_to_tool_input succeeds with default fields, then
        // ToolExecutor::ask_user returns an error which is mapped to
        // is_error=true with the error string in `text`.
        assert!(is_error, "missing-param path must be flagged as error");
        assert!(
            !text.is_empty(),
            "error text should not be empty when tool fails"
        );
    }

    /// The first call to a fresh limiter must not block — it acquires the
    /// single permit immediately and proceeds. Subsequent calls arriving
    /// within the interval must wait for the spawned refiller task to
    /// release the permit.
    ///
    /// Uses real wall-clock time (not `start_paused`) because
    /// `std::time::Instant` measures real time, not the Tokio virtual
    /// clock — `start_paused` would let the test "succeed" trivially
    /// because the assertion is in real milliseconds while virtual
    /// time auto-advances inside the runtime.
    #[tokio::test]
    async fn rate_limiter_first_call_proceeds_immediately() {
        // 200ms interval keeps the wall-clock cost of the test low while
        // still giving a measurable gap between the first and second
        // calls.
        let limiter = RateLimiter::new(200);
        let started = std::time::Instant::now();
        limiter.wait_if_needed().await;
        let first_elapsed = started.elapsed();
        // First call: permit was free, no spawn-then-wait blocking.
        assert!(
            first_elapsed < std::time::Duration::from_millis(100),
            "first call should be near-instant, got {:?}",
            first_elapsed
        );

        let started = std::time::Instant::now();
        limiter.wait_if_needed().await;
        let second_elapsed = started.elapsed();
        // Second call within the interval: must wait for the refiller to
        // release the permit. Allow a generous lower bound (50% of
        // interval) for the case where `Instant::now()` is sampled after
        // some scheduling latency, and a generous upper bound (2.5x the
        // interval) for CI flakiness.
        assert!(
            second_elapsed >= std::time::Duration::from_millis(100),
            "second call should be throttled (~200ms), got {:?}",
            second_elapsed
        );
        assert!(
            second_elapsed < std::time::Duration::from_millis(500),
            "second call should not exceed interval by more than 2.5x, got {:?}",
            second_elapsed
        );
    }

    /// Ten concurrent callers all call `wait_if_needed` at the same
    /// moment. All must eventually complete (the semaphore is never
    /// closed), and the total wall-clock time should reflect the rate
    /// limit: roughly `(N - 1) * min_interval` for single-permit
    /// semantics. The lower bound catches a regression where the
    /// limiter collapses to a no-op (instant return).
    ///
    /// Real wall-clock time is used (no `start_paused`) — see the note
    /// in `rate_limiter_first_call_proceeds_immediately`. With a 50ms
    /// interval, ten throttled calls should take roughly 9 * 50 = 450ms.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn rate_limiter_concurrent_callers_are_throttled_globally() {
        let limiter = Arc::new(RateLimiter::new(50));
        let started = std::time::Instant::now();

        let mut handles = Vec::new();
        for _ in 0..10 {
            let l = Arc::clone(&limiter);
            handles.push(tokio::spawn(async move {
                l.wait_if_needed().await;
            }));
        }
        for h in handles {
            h.await.expect("rate-limiter task should not panic");
        }
        let elapsed = started.elapsed();

        // 10 callers, 1 permit, 50ms interval -> first at t=0,
        // subsequent at t=50, 100, ..., 450. Lower bound (300ms = 6
        // intervals) catches a no-op regression. Upper bound (5s) is
        // loose enough for any reasonable CI machine.
        assert!(
            elapsed >= std::time::Duration::from_millis(300),
            "concurrent calls must be throttled (>= 300ms), got {:?}",
            elapsed
        );
        assert!(
            elapsed < std::time::Duration::from_secs(5),
            "concurrent calls should not be doubly-serialised (< 5s), got {:?}",
            elapsed
        );
    }

    /// Regression test for H1: provider error bodies must have API-key fragments
    /// redacted before being embedded into `OrchestratorError::Generic`. The
    /// OpenAI/NvidiaNim/Lmstudio error path previously embedded `err_text`
    /// verbatim; it now routes through `sanitize_error_message` like the
    /// Anthropic path. This test pins the sanitizer's contract directly so a
    /// future regression in either path is caught.
    #[test]
    fn sanitize_error_message_redacts_key_fragments() {
        // OpenAI-style key in an echoed Authorization header.
        let raw = r#"{"error":{"message":"Unauthorized","header":"Authorization: Bearer sk-proj-AbCdEf1234567890GhIjKl"}}"#;
        let sanitized = sanitize_error_message(raw);
        assert!(
            !sanitized.contains("sk-proj-AbCdEf1234567890GhIjKl"),
            "raw key leaked into sanitized output: {}",
            sanitized
        );
        assert!(
            sanitized.contains("Bearer [REDACTED]"),
            "expected Bearer redaction, got: {}",
            sanitized
        );

        // Bare sk- key embedded in body text.
        let raw2 = "Invalid API key sk-abcdefghijklmnopqrstuvwxyz1234567890ABCDEFGHIJ";
        let sanitized2 = sanitize_error_message(raw2);
        assert!(
            !sanitized2.contains("sk-abcdefghijklmnopqrstuvwxyz"),
            "bare key leaked: {}",
            sanitized2
        );
        assert!(sanitized2.contains("sk-[REDACTED]"));

        // Anthropic-style x-api-key header.
        let raw3 = "x-api-key: sk-ant-api03-XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX";
        let sanitized3 = sanitize_error_message(raw3);
        assert!(
            sanitized3.contains("x-api-key: [REDACTED]"),
            "x-api-key not redacted: {}",
            sanitized3
        );

        // Non-secret text passes through unmodified.
        let benign = "HTTP 429 Too Many Requests: rate limit exceeded";
        assert_eq!(sanitize_error_message(benign), benign);
    }

    #[test]
    fn validate_base_url_accepts_https_public() {
        assert!(validate_base_url("https://api.openai.com/v1").is_ok());
        assert!(validate_base_url("https://api.groq.com/openai/v1").is_ok());
        assert!(validate_base_url("https://api.anthropic.com").is_ok());
    }

    #[test]
    fn validate_base_url_accepts_http_loopback() {
        // Local LLM servers (LM Studio, Ollama, vLLM) documented in the
        // Settings placeholder must work over plain HTTP.
        assert!(validate_base_url("http://localhost:1234/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:11434/v1").is_ok());
        assert!(validate_base_url("http://[::1]:8080/v1").is_ok());
    }

    #[test]
    fn validate_base_url_rejects_http_public_host() {
        // No plaintext API keys over the public internet.
        assert!(validate_base_url("http://api.openai.com/v1").is_err());
        assert!(validate_base_url("http://example.com/v1").is_err());
    }

    #[test]
    fn validate_base_url_rejects_bad_scheme_and_empty_host() {
        assert!(validate_base_url("ftp://api.openai.com/v1").is_err());
        assert!(validate_base_url("api.openai.com").is_err()); // no scheme
        assert!(validate_base_url("https://").is_err()); // no host
                                                         // Single-label non-loopback host: almost certainly a typo.
        assert!(validate_base_url("https://internalhost/v1").is_err());
    }
}

#[cfg(test)]
mod title_tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // A tiny provider config so summarize_title finds a key without env vars.
    fn test_provider_config(mock_server_url: String) -> ProviderConfig {
        ProviderConfig::new(
            LLMProvider::Anthropic,
            "test-key",
            "claude-3-5-sonnet-20241022".to_string(),
            String::new(),
            Some(mock_server_url),
        )
    }

    #[tokio::test]
    async fn summarize_title_retries_on_5xx_then_succeeds() {
        let server = MockServer::start().await;
        let orch = AthenaOrchestrator::new_for_test(server.uri());
        orch.set_provider_config(test_provider_config(server.uri()));

        // First two calls fail 500, third succeeds.
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "analyzing the codebase" }]
            })))
            .mount(&server)
            .await;

        let title = orch
            .summarize_title_for_test("analyze the codebase")
            .await
            .unwrap();
        assert_eq!(title, "analyzing the codebase");
    }

    #[tokio::test]
    async fn summarize_title_fails_after_max_attempts() {
        let server = MockServer::start().await;
        let orch = AthenaOrchestrator::new_for_test(server.uri());
        orch.set_provider_config(test_provider_config(server.uri()));

        // Every call fails 500.
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
            .mount(&server)
            .await;

        let result = orch.summarize_title_for_test("analyze the codebase").await;
        assert!(result.is_err());
        // 3 attempts made (test backoff = [1,1,1]).
        // wiremock records request count:
        let received = server.received_requests().await.unwrap();
        assert_eq!(
            received.len(),
            3,
            "expected exactly 3 attempts, got {}",
            received.len()
        );
    }

    #[tokio::test]
    async fn summarize_title_missing_key_is_non_retryable() {
        // No provider config AND no ANTHROPIC_API_KEY env (tests run without it).
        let orch = AthenaOrchestrator::new_for_test("http://unused.invalid".to_string());
        // provider_config stays None.
        // SAFETY: tests run single-threaded within a process; we save & restore.
        let prev = std::env::var("ANTHROPIC_API_KEY").ok();
        std::env::remove_var("ANTHROPIC_API_KEY");
        let result = orch.summarize_title_for_test("analyze the codebase").await;
        // Restore.
        if let Some(v) = prev {
            std::env::set_var("ANTHROPIC_API_KEY", v);
        }

        assert!(matches!(result, Err(OrchestratorError::MissingApiKey)));
    }

    #[tokio::test]
    async fn summarize_title_trims_output() {
        let server = MockServer::start().await;
        let orch = AthenaOrchestrator::new_for_test(server.uri());
        orch.set_provider_config(test_provider_config(server.uri()));

        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "content": [{ "type": "text", "text": "  analyzing the codebase  " }]
            })))
            .mount(&server)
            .await;

        let title = orch
            .summarize_title_for_test("analyze the codebase")
            .await
            .unwrap();
        assert_eq!(title, "analyzing the codebase");
    }
}

#[cfg(test)]
mod snapshot_tests {
    use super::*;
    use crate::agent_comms::AgentComms;
    use crate::output_buffer::OutputBuffer;
    use crate::plan_manager::{ExecutionPlan, PlanManager};
    use crate::tool_executor::{ToolEventSender, ToolExecutor};
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

    fn build_test_orchestrator() -> AthenaOrchestrator {
        let ob = Arc::new(OutputBuffer::new());
        let pm = Arc::new(PlanManager::new());
        let ac = Arc::new(AgentComms::new());
        let store = Arc::new(athena_store::KeyValueStore::new_empty());

        let executor = Arc::new(parking_lot::RwLock::new(ToolExecutor::new(
            Arc::clone(&ob),
            Arc::clone(&pm),
            Arc::clone(&ac),
            Arc::new(MockEventSender),
            Arc::clone(&store),
            None,
        )));

        AthenaOrchestrator::with_context(executor, ob, pm, ac, None, Some(store), None)
    }

    /// Test that an empty snapshot is under 800 tokens.
    #[test]
    fn empty_snapshot_under_token_budget() {
        let orch = build_test_orchestrator();
        let snapshot = orch.build_app_state_snapshot();
        let tokens = AthenaOrchestrator::estimate_tokens(&snapshot);
        assert!(
            tokens <= 800,
            "snapshot is {} tokens, expected <= 800. snapshot:\n{}",
            tokens,
            snapshot
        );
    }

    /// Test that a populated snapshot with agents still fits in budget.
    #[test]
    fn snapshot_with_agents_fits_budget() {
        let ob = Arc::new(OutputBuffer::new());
        let pm = Arc::new(PlanManager::new());
        let ac = Arc::new(AgentComms::new());
        let store = Arc::new(athena_store::KeyValueStore::new_empty());

        // Seed workspace and active space
        store
            .set_sync(
                "workspaces",
                &serde_json::json!({
                    "active_space_id": "test-space",
                    "spaces": [
                        {
                            "id": "test-space",
                            "name": "Integration Test",
                            "panes": [{"id": "pane-1"}, {"id": "pane-2"}]
                        }
                    ]
                }),
            )
            .unwrap();

        let orch = AthenaOrchestrator::with_context(
            Arc::new(parking_lot::RwLock::new(ToolExecutor::new(
                Arc::clone(&ob),
                Arc::clone(&pm),
                Arc::clone(&ac),
                Arc::new(MockEventSender),
                Arc::clone(&store),
                None,
            ))),
            ob,
            pm,
            ac,
            None,
            Some(store),
            None,
        );

        let snapshot = orch.build_app_state_snapshot();
        let tokens = AthenaOrchestrator::estimate_tokens(&snapshot);
        assert!(
            tokens <= 800,
            "snapshot with agents is {} tokens, expected <= 800. snapshot:\n{}",
            tokens,
            snapshot
        );
    }
}
