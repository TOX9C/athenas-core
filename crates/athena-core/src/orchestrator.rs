use crate::tool_executor::{to_openai_tools, ToolExecutor, ToolInput};
use crate::types::*;
use secrecy::ExposeSecret;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Session persistence types
use athena_store::MessageRole as StoreMessageRole;
use athena_store::SessionMessage as StoreMessage;

/// System prompt defining Athena as a workspace-aware orchestrator.
/// This prompt is injected into every LLM conversation unless the user
/// explicitly overrides it in their provider configuration.
pub const SYSTEM_PROMPT: &str = r#"You are Athena, the orchestrator of a developer desktop environment.

You have access to tools that let you interact with the user's workspace.
IMPORTANT: You are workspace-aware. When asked about agents, ALWAYS check which
workspace the user is on first.

NEVER launch an agent implicitly. Always check the current state first.
If asked to "prompt 4 agents" but only 3 exist, ask the user what the 4th should be.

When a command would modify state (spawn agent, run command, create task),
confirm with the user first.

Available tools:
- close_terminals: Close, remove, or replace terminal panes/agents from the UI.
- launch_builtin_agent: Launch standard background agents (claude, codex, opencode, gemini, shell).
- launch_custom_agent: Launch a user-defined custom agent via CLI command.
- run_command_in_terminals: Run a CLI command in already-open shell/terminal panes.
- read_agent_output: Read captured terminal output from a specific agent pane.
- list_agents: List all currently running agent panes with their IDs, types, line counts, and timestamps.
- check_agent_status: Check the current status of a specific agent by ID.
- create_execution_plan: Create a structured execution plan before dispatching agents.
- dispatch_plan_step: Launch an agent to execute a specific plan step.
- prompt_agent: Send a prompt to an already-running agent pane.
- ask_user: Ask the user a clarifying question with selectable options.
- evaluate_results: Evaluate the results of an execution plan.
- kanban_list_tasks: List all tasks on the active workspace's Kanban board.
- kanban_create_task: Create a new Kanban task (requires title, space_id).
- kanban_update_task: Update a Kanban task (requires task_id).
- kanban_delete_task: Delete a Kanban task by ID (requires task_id).
- fs_read_file: Read the contents of a file from the workspace.
- fs_list_dir: List the contents of a directory in the workspace.
- fs_search: Search files in the workspace using ripgrep."#;

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

fn validate_base_url(url: &str) -> Result<(), OrchestratorError> {
    if !url.starts_with("https://") {
        return Err(OrchestratorError::Generic(
            "Base URL must use HTTPS".to_string(),
        ));
    }
    let host = url
        .trim_start_matches("https://")
        .split('/')
        .next()
        .unwrap_or("");
    if host.is_empty() || !host.contains('.') {
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
///
/// Uses `tokio::sync::Mutex` so the lock can be held across `.await`
/// boundaries, preventing the TOCTOU race condition where two concurrent
/// requests could both bypass the limiter.
struct RateLimiter {
    last_request: tokio::sync::Mutex<Instant>,
    min_interval: std::time::Duration,
}

impl RateLimiter {
    fn new(min_interval_ms: u64) -> Self {
        Self {
            last_request: tokio::sync::Mutex::new(Instant::now()),
            min_interval: std::time::Duration::from_millis(min_interval_ms),
        }
    }

    async fn wait_if_needed(&self) {
        let mut last = self.last_request.lock().await;
        let elapsed = last.elapsed();
        if elapsed < self.min_interval {
            tokio::time::sleep(self.min_interval - elapsed).await;
        }
        *last = Instant::now();
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
        // Build current app state snapshot and prepend it to the user text
        let snapshot = self.build_app_state_snapshot();
        let full_text = format!("{}\n{}", snapshot, text);

        let (provider, api_key, model, system_prompt, base_url) = {
            let guard = self.provider_config.lock();
            match guard.as_ref() {
                Some(config) => {
                    let sp = if config.system_prompt.trim().is_empty() {
                        SYSTEM_PROMPT.to_string()
                    } else {
                        config.system_prompt.clone()
                    };
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
                        "claude-sonnet-4-20250514".to_string(),
                        SYSTEM_PROMPT.to_string(),
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
                    full_text,
                    images,
                    resolved_base_url,
                )
                .await
            }
            LLMProvider::Anthropic => {
                self.send_anthropic(api_key, model, system_prompt, full_text, images)
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

    /// Execute a single tool call through the configured executor.
    ///
    /// Returns a tuple of `(text, is_error)`. If no executor is configured,
    /// returns an error message with `is_error = true`.
    fn execute_tool(&self, name: &str, input: &serde_json::Value) -> (String, bool) {
        let tool_input = match json_to_tool_input(input) {
            Ok(ti) => ti,
            Err(e) => {
                return (
                    format!("Failed to deserialize tool input for '{}': {}", name, e),
                    true,
                )
            }
        };

        match &self.tool_executor {
            Some(executor_arc) => {
                let executor = executor_arc.lock();
                match executor.execute_tool_call(name, &tool_input) {
                    Ok(result) => (result.text, result.is_error.unwrap_or(false)),
                    Err(e) => (format!("Tool execution error: {}", e), true),
                }
            }
            None => (
                format!(
                    "Tool '{}' was requested but no tool executor is configured. \
                     Pass an executor via AthenaOrchestrator::new_with_executor().",
                    name
                ),
                true,
            ),
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
                    "input_schema": {
                        "type": t.function.parameters.schema_type,
                        "properties": t.function.parameters.properties,
                        "required": t.function.parameters.required,
                    }
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
                "max_tokens": 4096,
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

            // Execute each tool call and append tool_result messages.
            {
                let mut msgs = self.anthropic_messages.lock();

                for tool_call in &tool_calls {
                    let tool_use_id = tool_call["id"].as_str().unwrap_or("unknown");
                    let tool_name = tool_call["name"].as_str().unwrap_or("unknown");
                    let tool_input = &tool_call["input"];

                    let (result_text, is_error) = self.execute_tool(tool_name, tool_input);

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
                "max_tokens": 4096,
                "messages": body_messages,
                "tools": tools,
                "tool_choice": "auto",
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
                let mut msgs = self.openai_messages.lock();
                msgs.truncate(user_msg_index);
                return Err(OrchestratorError::Generic(format!(
                    "OpenAI API error {}: {}",
                    status, err_text
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

            {
                let mut msgs = self.openai_messages.lock();

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

                    let (result_text, is_error) = self.execute_tool(function_name, &function_args);

                    let tool_response_content = if is_error {
                        serde_json::json!({
                            "error": result_text,
                        })
                    } else {
                        serde_json::Value::String(result_text)
                    };

                    msgs.push(OpenAIMessage {
                        role: "tool".to_string(),
                        content: tool_response_content,
                        tool_calls: None,
                        tool_call_id: Some(call_id.to_string()),
                        name: Some(function_name.to_string()),
                    });
                }
            }

            // Continue loop: send the tool results back and get the next response.
        }
    }
}
