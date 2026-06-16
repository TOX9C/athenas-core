use crate::tool_executor::{to_openai_tools, ToolExecutor, ToolInput};
use crate::types::*;
use secrecy::ExposeSecret;
use std::sync::Arc;
use std::time::Duration;

// Session persistence types
use athena_store::MessageRole as StoreMessageRole;
use athena_store::SessionMessage as StoreMessage;

/// Default model used when no provider config is set (env-key fallback path).
pub const DEFAULT_ANTHROPIC_MODEL: &str = "claude-sonnet-4-20250514";

/// Stable Anthropic API version header value.
pub const ANTHROPIC_VERSION: &str = "2023-06-01";

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
- For any multi-step task, call create_execution_plan first, then dispatch_plan_step for each step, then evaluate_results. Give every step a unique id and a self-contained description (the description is what the agent receives as its prompt).
- Never launch an agent the user didn't ask for. If asked for N agents but only M exist or are defined, use ask_user to resolve the gap before launching.

## Talking to agents & terminals
- prompt_agent sends an instruction to one already-running agent.
- run_command_in_terminals runs a shell command in already-open panes.

## Confirmation
Proceed autonomously for read-only actions and for launching/dispatching agents the user requested. Confirm with the user first ONLY for destructive actions: close_terminals and kanban_delete_task.

## Asking the user
Use ask_user when you need a decision to proceed. Each option must have a short `label` (shown on the button) and may include a longer `description`."#;

/// Configuration for a specific LLM provider.
///
/// The `api_key` is stored in a zero-on-drop `SecretString` to prevent
/// accidental exposure in logs, error messages, or memory dumps.
#[derive(Clone)]
pub struct ProviderConfig {
    pub provider: LLMProvider,
    api_key: secrecy::SecretString,
    pub model: String,
    pub system_prompt: String,
    pub base_url: Option<String>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("provider", &self.provider)
            .field("api_key", &"[REDACTED]")
            .field("model", &self.model)
            .field("system_prompt", &"[...]")
            .field("base_url", &self.base_url)
            .finish()
    }
}

impl std::fmt::Display for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ProviderConfig(provider={:?}, model={}, base_url={:?})",
            self.provider, self.model, self.base_url
        )
    }
}

impl ProviderConfig {
    /// Create a new ProviderConfig with the given API key.
    pub fn new(
        provider: LLMProvider,
        api_key: impl Into<String>,
        model: String,
        system_prompt: String,
        base_url: Option<String>,
    ) -> Self {
        Self {
            provider,
            api_key: secrecy::SecretString::from(api_key.into()),
            model,
            system_prompt,
            base_url,
        }
    }

    /// Get the API key as a SecretString reference.
    pub fn api_key(&self) -> &secrecy::SecretString {
        &self.api_key
    }

    /// Get the API key exposure (use sparingly, only for actual API calls).
    pub fn expose_api_key(&self) -> &String {
        use secrecy::ExposeSecret;
        self.api_key.expose_secret()
    }
}

impl serde::Serialize for ProviderConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::Serialize;

        #[derive(Serialize)]
        struct ProviderConfigSer<'a> {
            provider: &'a LLMProvider,
            api_key: &'a str,
            model: &'a str,
            system_prompt: &'a str,
            base_url: &'a Option<String>,
        }

        let ser = ProviderConfigSer {
            provider: &self.provider,
            api_key: "[REDACTED]",
            model: &self.model,
            system_prompt: "[...]",
            base_url: &self.base_url,
        };
        ser.serialize(serializer)
    }
}

impl<'de> serde::Deserialize<'de> for ProviderConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ProviderConfigDe {
            provider: LLMProvider,
            api_key: String,
            model: String,
            system_prompt: String,
            base_url: Option<String>,
        }

        let de = ProviderConfigDe::deserialize(deserializer)?;
        Ok(ProviderConfig {
            provider: de.provider,
            api_key: secrecy::SecretString::from(de.api_key),
            model: de.model,
            system_prompt: de.system_prompt,
            base_url: de.base_url,
        })
    }
}

/// Sanitize an error message by redacting potential API key fragments.
///
/// Looks for common API key prefixes (sk-, x-api, etc.) and replaces
/// them with [REDACTED].
fn sanitize_error_message(msg: &str) -> String {
    let patterns = [
        (r"sk-[a-zA-Z0-9]{20,}", "sk-[REDACTED]"),
        (r"x-api-key: [^\s]+", "x-api-key: [REDACTED]"),
        (r"Bearer [^\s]+", "Bearer [REDACTED]"),
        (r"api[_-]?key[:=][^\s]+", "api_key=[REDACTED]"),
    ];
    let mut result = msg.to_string();
    for (pat, repl) in &patterns {
        if let Ok(re) = regex::Regex::new(pat) {
            result = re.replace_all(&result, *repl).to_string();
        }
    }
    result
}

/// Internal representation of a message for Anthropic's API.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AnthropicMessage {
    role: String,
    content: serde_json::Value,
}

/// Internal representation of a message for OpenAI-compatible APIs.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct OpenAIMessage {
    role: String,
    content: serde_json::Value,
    /// OpenAI requires `tool_calls` at the top level of the assistant message,
    /// and `tool_call_id` on the tool-response message. We store them here so
    /// we can serialize correctly without ad-hoc JSON patching.
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_calls: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
}

fn build_anthropic_content(text: &str, images: Option<&[ImageData]>) -> serde_json::Value {
    match images {
        None | Some(&[]) => serde_json::Value::String(text.to_string()),
        Some(imgs) => {
            let mut blocks = Vec::new();
            for img in imgs {
                blocks.push(serde_json::json!({
                    "type": "image",
                    "source": {
                        "type": "base64",
                        "media_type": img.media_type,
                        "data": img.base64
                    }
                }));
            }
            blocks.push(serde_json::json!({
                "type": "text",
                "text": text
            }));
            serde_json::Value::Array(blocks)
        }
    }
}

/// Extract the host portion from a URL authority, stripping userinfo and
/// port, and handling IPv6 literals (`[::1]:8080`). Returned without brackets.
fn url_host(url: &str) -> Option<String> {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest)?;
    let authority = after_scheme.split('/').next()?;
    let hostport = authority.rsplit('@').next()?;
    if let Some(rest) = hostport.strip_prefix('[') {
        // IPv6 literal: host is everything up to the closing ']'.
        let v6 = rest.split(']').next().unwrap_or("");
        return Some(v6.to_string());
    }
    Some(hostport.split(':').next().unwrap_or("").to_string())
}

fn validate_base_url(url: &str) -> Result<(), OrchestratorError> {
    // Accept either scheme. We enforce HTTPS for any host that isn't a
    // loopback / private address, so an API key is never sent in cleartext
    // over the public internet — but local LLM servers (LM Studio, Ollama,
    // vLLM, …) documented as `http://localhost:1234/v1` still work.
    let (scheme, _rest) = url
        .split_once("://")
        .ok_or_else(|| OrchestratorError::Generic("Base URL must include a scheme (https:// or http://)".to_string()))?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "https" && scheme != "http" {
        return Err(OrchestratorError::Generic(format!(
            "Base URL must use http:// or https:// (got '{scheme}://')"
        )));
    }

    let host = url_host(url).unwrap_or_default();
    if host.is_empty() {
        return Err(OrchestratorError::Generic(
            "Base URL must have a valid hostname".to_string(),
        ));
    }

    // A loopback IP (IPv4 or IPv6) or the "localhost" label identifies a
    // local server and is exempt from the HTTPS requirement.
    let is_loopback = host
        .parse::<std::net::IpAddr>()
        .map_or(false, |ip| ip.is_loopback())
        || host == "localhost";

    if scheme == "http" && !is_loopback {
        return Err(OrchestratorError::Generic(
            "Base URL must use HTTPS for non-local hosts".to_string(),
        ));
    }

    // Public hostnames need a dot (e.g. api.openai.com). Single-label names
    // other than localhost are almost certainly a typo.
    if !is_loopback && !host.contains('.') {
        return Err(OrchestratorError::Generic(
            "Base URL must have a valid hostname".to_string(),
        ));
    }
    Ok(())
}

fn build_openai_content(text: &str, images: Option<&[ImageData]>) -> serde_json::Value {
    match images {
        None | Some(&[]) => serde_json::Value::String(text.to_string()),
        Some(imgs) => {
            let mut parts = Vec::new();
            for img in imgs {
                let url = format!("data:{};base64,{}", img.media_type, img.base64);
                parts.push(serde_json::json!({
                    "type": "image_url",
                    "image_url": { "url": url }
                }));
            }
            parts.push(serde_json::json!({
                "type": "text",
                "text": text
            }));
            serde_json::Value::Array(parts)
        }
    }
}

/// Deserialize a JSON value into a `ToolInput`, filling only the fields present.
///
/// Returns an error instead of silently falling back to defaults so the LLM
/// can retry with corrected arguments.
fn json_to_tool_input(value: &serde_json::Value) -> Result<ToolInput, OrchestratorError> {
    serde_json::from_value(value.clone()).map_err(OrchestratorError::SerializationError)
}

/// Rate limiter to prevent hammering the LLM API.
/// Global rate limiter for outbound LLM API requests.
///
/// Backed by a `tokio::sync::Semaphore` with a single permit. The first
/// caller acquires the permit immediately, then spawns a background task
/// that sleeps for `min_interval` and releases the permit. Subsequent
/// callers arriving during that window wait on the semaphore — this
/// guarantees the limiter is enforced globally across concurrent tasks
/// (not per-task), without serializing the entire hot path on a mutex.
struct RateLimiter {
    semaphore: Arc<tokio::sync::Semaphore>,
    min_interval: std::time::Duration,
}

impl RateLimiter {
    fn new(min_interval_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(tokio::sync::Semaphore::new(1)),
            min_interval: std::time::Duration::from_millis(min_interval_ms),
        }
    }

    async fn wait_if_needed(&self) {
        // Acquire the single permit. If it's currently held (by a
        // recent call's refiller task), this awaits until that task
        // releases it. The semaphore is never closed, so `acquire`
        // cannot fail in practice; we still handle the error path to
        // satisfy the type system and surface unexpected closes loudly.
        let permit = match self.semaphore.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => {
                // The semaphore was closed (should never happen during
                // normal operation). Fall back to a no-op wait so we
                // don't panic in a request hot path.
                return;
            }
        };
        // Spawn a task that returns the permit after `min_interval`.
        // Because we used `acquire_owned`, the permit can move into
        // the spawned task and be dropped there — releasing the
        // semaphore slot — without us needing to hold it across the
        // caller's actual work.
        let interval = self.min_interval;
        tokio::spawn(async move {
            tokio::time::sleep(interval).await;
            drop(permit);
        });
    }
}

/// The Athena orchestrator that dispatches messages to LLM providers
/// and executes tool calls via the `ToolExecutor`.
pub struct AthenaOrchestrator {
    anthropic_messages: Arc<parking_lot::Mutex<Vec<AnthropicMessage>>>,
    openai_messages: Arc<parking_lot::Mutex<Vec<OpenAIMessage>>>,
    current_session_id: Arc<parking_lot::Mutex<Option<String>>>,
    tool_executor: Option<Arc<parking_lot::Mutex<ToolExecutor>>>,
    http_client: reqwest::Client,
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
}

impl Default for AthenaOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl AthenaOrchestrator {
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
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
            provider_config: Arc::new(parking_lot::Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000), // 1 second minimum between requests
            output_buffer: None,
            plan_manager: None,
            agent_comms: None,
            workspace_name: Arc::new(parking_lot::Mutex::new(None)),
            session_store: None,
        }
    }

    /// Create an orchestrator with service references for building
    /// the app state snapshot injected before every LLM call.
    pub fn with_context(
        executor: Arc<parking_lot::Mutex<ToolExecutor>>,
        output_buffer: Arc<crate::output_buffer::OutputBuffer>,
        plan_manager: Arc<crate::plan_manager::PlanManager>,
        agent_comms: Arc<crate::agent_comms::AgentComms>,
        session_store: Option<Arc<athena_store::SessionStore>>,
    ) -> Self {
        Self {
            anthropic_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            openai_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            current_session_id: Arc::new(parking_lot::Mutex::new(None)),
            tool_executor: Some(executor),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
            provider_config: Arc::new(parking_lot::Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000),
            output_buffer: Some(output_buffer),
            plan_manager: Some(plan_manager),
            agent_comms: Some(agent_comms),
            workspace_name: Arc::new(parking_lot::Mutex::new(None)),
            session_store,
        }
    }

    /// Create a new orchestrator with a tool executor wired in.
    ///
    /// When the LLM returns `tool_use` / `tool_calls`, the executor
    /// dispatches them and the results are fed back into the conversation
    /// loop automatically.
    pub fn new_with_executor(executor: Arc<parking_lot::Mutex<ToolExecutor>>) -> Self {
        Self {
            anthropic_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            openai_messages: Arc::new(parking_lot::Mutex::new(Vec::new())),
            current_session_id: Arc::new(parking_lot::Mutex::new(None)),
            tool_executor: Some(executor),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
            provider_config: Arc::new(parking_lot::Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000), // 1 second minimum between requests
            output_buffer: None,
            plan_manager: None,
            agent_comms: None,
            workspace_name: Arc::new(parking_lot::Mutex::new(None)),
            session_store: None,
        }
    }

    /// Replace or clear the tool executor at runtime.
    pub fn set_tool_executor(&mut self, executor: Option<Arc<parking_lot::Mutex<ToolExecutor>>>) {
        self.tool_executor = executor;
    }

    /// Set the conversation history from a list of session entries.
    pub fn set_session_context(&self, history: Vec<SessionHistoryEntry>) {
        let anthropic: Vec<AnthropicMessage> = history
            .iter()
            .map(|entry| AnthropicMessage {
                role: entry.role.clone(),
                content: build_anthropic_content(&entry.content, entry.images.as_deref()),
            })
            .collect();

        let openai: Vec<OpenAIMessage> = history
            .iter()
            .map(|entry| OpenAIMessage {
                role: entry.role.clone(),
                content: build_openai_content(&entry.content, entry.images.as_deref()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();

        {
            let mut a = self.anthropic_messages.lock();
            *a = anthropic;
        }
        {
            let mut o = self.openai_messages.lock();
            *o = openai;
        }
    }

    /// Clear all stored conversation context.
    pub fn clear_context(&self) {
        self.anthropic_messages.lock().clear();
        self.openai_messages.lock().clear();
        *self.current_session_id.lock() = None;
    }

    /// Set the current session identifier.
    pub fn set_current_session_id(&self, id: String) {
        *self.current_session_id.lock() = Some(id);
    }

    /// Get the current session identifier, if any.
    pub fn get_current_session_id(&self) -> Option<String> {
        self.current_session_id.lock().clone()
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

    /// Set the session store for persisting conversations.
    pub fn set_session_store(&mut self, store: Arc<athena_store::SessionStore>) {
        self.session_store = Some(store);
    }

    /// Get a reference to the session store, if configured.
    pub fn session_store(&self) -> Option<&Arc<athena_store::SessionStore>> {
        self.session_store.as_ref()
    }

    /// Save the current conversation history to the session store.
    pub async fn save_conversation(&self, session_id: &str) -> Result<(), OrchestratorError> {
        let Some(ref store) = self.session_store else {
            return Ok(());
        };

        // Prefer openai messages (default format) for persistence.
        let store_messages: Vec<StoreMessage> = {
            let openai = self.openai_messages.lock();

            let mut store_messages = Vec::new();
            for msg in openai.iter() {
                if msg.role == "system" || msg.role == "tool" {
                    continue;
                }
                let role = if msg.role == "user" {
                    StoreMessageRole::User
                } else {
                    StoreMessageRole::Athena
                };
                let content = match &msg.content {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Null => String::new(),
                    other => other.to_string(),
                };
                store_messages.push(StoreMessage {
                    id: uuid::Uuid::new_v4().to_string(),
                    role,
                    content,
                    timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    is_error: None,
                    image_refs: None,
                });
            }
            store_messages
        }; // guard dropped here before any await

        store
            .update_session(session_id, None, Some(store_messages))
            .await
            .map_err(|e| OrchestratorError::Generic(e.to_string()))?;
        Ok(())
    }

    /// Load a previous conversation from the session store into the orchestrator.
    pub async fn load_conversation(&self, session_id: &str) -> Result<(), OrchestratorError> {
        let Some(ref store) = self.session_store else {
            return Ok(());
        };

        let session = store
            .get_session(session_id)
            .await
            .map_err(|e| OrchestratorError::Generic(e.to_string()))?
            .ok_or_else(|| {
                OrchestratorError::Generic(format!("Session '{}' not found", session_id))
            })?;

        let anthropic: Vec<AnthropicMessage> = session
            .messages
            .iter()
            .map(|msg| AnthropicMessage {
                role: match msg.role {
                    StoreMessageRole::User => "user".to_string(),
                    StoreMessageRole::Athena => "assistant".to_string(),
                },
                content: serde_json::Value::String(msg.content.clone()),
            })
            .collect();

        let openai: Vec<OpenAIMessage> = session
            .messages
            .iter()
            .map(|msg| OpenAIMessage {
                role: match msg.role {
                    StoreMessageRole::User => "user".to_string(),
                    StoreMessageRole::Athena => "assistant".to_string(),
                },
                content: serde_json::Value::String(msg.content.clone()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            })
            .collect();

        *self.anthropic_messages.lock() = anthropic;
        *self.openai_messages.lock() = openai;
        *self.current_session_id.lock() = Some(session_id.to_string());

        Ok(())
    }

    /// Attempt to auto-save the current conversation to the session store.
    pub async fn try_auto_save(&self) -> Result<(), OrchestratorError> {
        if let Some(session_id) = self.get_current_session_id() {
            self.save_conversation(&session_id).await
        } else {
            Ok(())
        }
    }

    /// Build a snapshot of the current app state for context injection.
    fn build_app_state_snapshot(&self) -> String {
        let mut lines: Vec<String> = Vec::new();
        lines.push("====== ATHENA STATE SNAPSHOT ======".to_string());
        lines.push(String::new());

        // Active workspace
        let workspace = self
            .workspace_name
            .lock()
            .clone()
            .unwrap_or_else(|| "Unknown".to_string());
        lines.push(format!("Active Workspace: {}", workspace));
        lines.push(String::new());

        // Running agents from output buffer
        lines.push("--- Running Agents ---".to_string());
        let mut has_agents = false;
        if let Some(ref ob) = self.output_buffer {
            let panes = ob.get_agent_list();
            if !panes.is_empty() {
                has_agents = true;
                for pane in &panes {
                    let activity =
                        chrono::DateTime::from_timestamp_millis(pane.last_activity_at as i64)
                            .map(|dt| dt.to_rfc3339())
                            .unwrap_or_else(|| "unknown".to_string());
                    lines.push(format!(
                        "  {} | type={} | lines={} | last_activity={}",
                        pane.pane_id, pane.agent_type, pane.line_count, activity
                    ));
                }
            }
        }
        if let Some(ref ac) = self.agent_comms {
            let sessions = ac.get_agent_sessions();
            if !sessions.is_empty() {
                has_agents = true;
                for s in &sessions {
                    let connected = chrono::DateTime::from_timestamp_millis(s.connected_at as i64)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_else(|| "unknown".to_string());
                    let status = &s.status;
                    lines.push(format!(
                        "  session={} | agent={} | plugin={} | status={} | connected_at={}",
                        s.id, s.agent_id, s.plugin_id, status, connected
                    ));
                }
            }
        }
        if !has_agents {
            lines.push("  (no agents currently running)".to_string());
        }
        lines.push(String::new());

        // Active execution plan
        lines.push("--- Active Execution Plan ---".to_string());
        if let Some(ref pm) = self.plan_manager {
            if let Some(plan) = pm.get_active_plan() {
                lines.push(format!("  ID: {}", plan.id));
                lines.push(format!("  Goal: {}", plan.goal));
                lines.push(format!("  Status: {:?}", plan.status));
                if !plan.steps.is_empty() {
                    lines.push("  Steps:".to_string());
                    for step in &plan.steps {
                        lines.push(format!(
                            "    {}: {} — status={:?}",
                            step.id, step.description, step.status
                        ));
                    }
                }
            } else {
                lines.push("  (no active execution plan)".to_string());
            }
        } else {
            lines.push("  (no active execution plan)".to_string());
        }
        lines.push(String::new());

        // Kanban tasks (backend persistence is now available via kanban_list_tasks)
        lines.push("--- Kanban Tasks ---".to_string());
        lines.push(
            "  (tasks are persisted to the backend; use kanban_list_tasks to query them)"
                .to_string(),
        );
        lines.push(String::new());

        lines.push("=====================================".to_string());
        lines.push(String::new());

        lines.join("\n")
    }

    /// Send a message to the configured LLM provider.
    pub async fn send_message(
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
            LLMProvider::NvidiaNim => Some("https://integrate.api.nvidia.com/v1".to_string()),
            LLMProvider::OpenAI => Some("https://api.openai.com/v1".to_string()),
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
            if let Err(e) = self.try_auto_save().await {
                log::warn!("Failed to auto-save conversation: {}", e);
            }
        }

        result
    }

    /// Send a one-shot request to the configured LLM to summarize a prompt
    /// into a short (2–3 word) title. Does NOT touch conversation history.
    pub async fn summarize_title(&self, raw_prompt: &str) -> Result<String, OrchestratorError> {
        const SYSTEM: &str = "You summarize prompts into 2-3 word titles. Output ONLY the title, no quotes.";
        let prompt = format!("Summarize in 2-3 words: {}", raw_prompt);

        let config = { self.provider_config.lock().as_ref().cloned() };

        let (provider, api_key, model, base_url) = match config {
            Some(c) => (c.provider.clone(), c.api_key().clone(), c.model.clone(), c.base_url.clone()),
            None => {
                let api_key = std::env::var("ANTHROPIC_API_KEY")
                    .ok()
                    .ok_or(OrchestratorError::MissingApiKey)?;
                (LLMProvider::Anthropic, secrecy::SecretString::from(api_key), DEFAULT_ANTHROPIC_MODEL.to_string(), None)
            }
        };

        let client = &self.http_client;

        match provider {
            LLMProvider::Anthropic => {
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": 20,
                    "system": SYSTEM,
                    "messages": [{"role": "user", "content": prompt}]
                });
                let response = client
                    .post("https://api.anthropic.com/v1/messages")
                    .header("x-api-key", api_key.expose_secret())
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response.text().await.unwrap_or_default();
                    return Err(OrchestratorError::Generic(format!("Anthropic API error {}: {}", status, err_text)));
                }
                let json: serde_json::Value = response.json().await?;
                let content = json["content"].as_array().ok_or_else(|| OrchestratorError::Generic("Invalid Anthropic response: no content array".to_string()))?;
                let summary = content.iter()
                    .filter(|b| b["type"].as_str() == Some("text"))
                    .map(|b| b["text"].as_str().unwrap_or(""))
                    .collect::<Vec<_>>()
                    .join(" ")
                    .trim()
                    .to_string();
                Ok(summary)
            }
            LLMProvider::OpenAI | LLMProvider::NvidiaNim | LLMProvider::Lmstudio => {
                let url = match &provider {
                    LLMProvider::NvidiaNim => "https://integrate.api.nvidia.com/v1".to_string(),
                    LLMProvider::OpenAI => "https://api.openai.com/v1".to_string(),
                    LLMProvider::Lmstudio => base_url.unwrap_or_else(|| "http://localhost:1234/v1".to_string()),
                    _ => unreachable!(),
                };
                let body = serde_json::json!({
                    "model": model,
                    "max_tokens": 20,
                    "messages": [
                        {"role": "system", "content": SYSTEM},
                        {"role": "user", "content": prompt}
                    ],
                    "stream": false,
                });
                let response = client
                    .post(format!("{}/chat/completions", url))
                    .header("Authorization", format!("Bearer {}", api_key.expose_secret()))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send()
                    .await?;

                if !response.status().is_success() {
                    let status = response.status();
                    let err_text = response.text().await.unwrap_or_default();
                    return Err(OrchestratorError::Generic(format!("OpenAI API error {}: {}", status, err_text)));
                }
                let json: serde_json::Value = response.json().await?;
                let summary = json["choices"][0]["message"]["content"]
                    .as_str()
                    .unwrap_or("Summary")
                    .trim()
                    .to_string();
                Ok(summary)
            }
        }
    }

    /// Execute a single tool call through the configured executor.
    ///
    /// Returns a tuple of `(text, is_error)`. If no executor is configured,
    /// returns an error message with `is_error = true`.
    ///
    /// The synchronous `ToolExecutor::execute_tool_call` performs blocking work
    /// (filesystem reads, kanban DB ops, the `ask_user` mpsc receive, etc.) that
    /// would otherwise stall the Tokio runtime worker thread. We offload the
    /// dispatch to `tokio::task::spawn_blocking` so the async runtime stays
    /// responsive to other tasks (HTTP, rate limiter, cancellation, etc.).
    async fn execute_tool(&self, name: &str, input: &serde_json::Value) -> (String, bool) {
        let tool_input = match json_to_tool_input(input) {
            Ok(ti) => ti,
            Err(e) => {
                return (
                    format!("Failed to deserialize tool input for '{}': {}", name, e),
                    true,
                )
            }
        };

        let Some(executor_arc) = self.tool_executor.clone() else {
            return (
                format!(
                    "Tool '{}' was requested but no tool executor is configured. \
                     Pass an executor via AthenaOrchestrator::new_with_executor().",
                    name
                ),
                true,
            );
        };

        let name = name.to_string();
        match tokio::task::spawn_blocking(move || {
            let executor = executor_arc.lock();
            executor.execute_tool_call(&name, &tool_input)
        })
        .await
        {
            Ok(Ok(result)) => (result.text, result.is_error.unwrap_or(false)),
            Ok(Err(e)) => (format!("Tool execution error: {}", e), true),
            Err(join_err) => (format!("Tool execution task panicked: {}", join_err), true),
        }
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
                .header("anthropic-version", "2024-10-22")
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
    use crate::notification::NotificationService;
    use crate::output_buffer::OutputBuffer;
    use crate::plan_manager::PlanManager;
    use crate::tool_executor::{ToolEventSender, ToolExecutor, ToolInput};
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
            Arc::new(NotificationService::new()),
            Arc::new(PlanManager::new()),
            Arc::new(AgentComms::new()),
            Arc::new(BlockingEventSender),
            Arc::new(athena_store::KeyValueStore::new_empty()),
        );
        AthenaOrchestrator::new_with_executor(Arc::new(parking_lot::Mutex::new(executor)))
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

    // Reference ToolInput to silence unused import warnings if all tests
    // are stripped at compile time.
    #[allow(dead_code)]
    fn _tool_input_ref(_t: &ToolInput) {}

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
