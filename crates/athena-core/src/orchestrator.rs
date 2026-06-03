use crate::tool_executor::{to_openai_tools, ToolExecutor, ToolInput};
use crate::types::*;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Configuration for a specific LLM provider.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderConfig {
    pub provider: LLMProvider,
    pub api_key: String,
    pub model: String,
    pub system_prompt: String,
    pub base_url: Option<String>,
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
    anthropic_messages: Arc<Mutex<Vec<AnthropicMessage>>>,
    openai_messages: Arc<Mutex<Vec<OpenAIMessage>>>,
    current_session_id: Arc<Mutex<Option<String>>>,
    tool_executor: Option<Arc<Mutex<ToolExecutor>>>,
    http_client: reqwest::Client,
    provider_config: Arc<Mutex<Option<ProviderConfig>>>,
    rate_limiter: RateLimiter,
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
            anthropic_messages: Arc::new(Mutex::new(Vec::new())),
            openai_messages: Arc::new(Mutex::new(Vec::new())),
            current_session_id: Arc::new(Mutex::new(None)),
            tool_executor: None,
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
            provider_config: Arc::new(Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000), // 1 second minimum between requests
        }
    }

    /// Create a new orchestrator with a tool executor wired in.
    ///
    /// When the LLM returns `tool_use` / `tool_calls`, the executor
    /// dispatches them and the results are fed back into the conversation
    /// loop automatically.
    pub fn new_with_executor(executor: Arc<Mutex<ToolExecutor>>) -> Self {
        Self {
            anthropic_messages: Arc::new(Mutex::new(Vec::new())),
            openai_messages: Arc::new(Mutex::new(Vec::new())),
            current_session_id: Arc::new(Mutex::new(None)),
            tool_executor: Some(executor),
            http_client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("Failed to build HTTP client"),
            provider_config: Arc::new(Mutex::new(None)),
            rate_limiter: RateLimiter::new(1000), // 1 second minimum between requests
        }
    }

    /// Replace or clear the tool executor at runtime.
    pub fn set_tool_executor(&mut self, executor: Option<Arc<Mutex<ToolExecutor>>>) {
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

        if let Ok(mut a) = self.anthropic_messages.lock() {
            *a = anthropic;
        }
        if let Ok(mut o) = self.openai_messages.lock() {
            *o = openai;
        }
    }

    /// Clear all stored conversation context.
    pub fn clear_context(&self) {
        if let Ok(mut a) = self.anthropic_messages.lock() {
            a.clear();
        }
        if let Ok(mut o) = self.openai_messages.lock() {
            o.clear();
        }
        if let Ok(mut id) = self.current_session_id.lock() {
            *id = None;
        }
    }

    /// Set the current session identifier.
    pub fn set_current_session_id(&self, id: String) {
        if let Ok(mut s) = self.current_session_id.lock() {
            *s = Some(id);
        }
    }

    /// Get the current session identifier, if any.
    pub fn get_current_session_id(&self) -> Option<String> {
        self.current_session_id.lock().ok().and_then(|s| s.clone())
    }

    /// Set the LLM provider configuration.
    pub fn set_provider_config(&self, config: ProviderConfig) {
        if let Ok(mut guard) = self.provider_config.lock() {
            *guard = Some(config);
        }
    }

    /// Get the current LLM provider configuration, if set.
    pub fn get_provider_config(&self) -> Option<ProviderConfig> {
        self.provider_config.lock().ok().and_then(|g| g.clone())
    }

    /// Send a message to the configured LLM provider.
    pub async fn send_message(
        &self,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        let (provider, api_key, model, system_prompt, base_url) = {
            let guard = self.provider_config.lock().map_err(|_| {
                OrchestratorError::Generic("Failed to lock provider config".to_string())
            })?;
            match guard.as_ref() {
                Some(config) => (
                    config.provider.clone(),
                    config.api_key.clone(),
                    config.model.clone(),
                    config.system_prompt.clone(),
                    config.base_url.clone(),
                ),
                None => {
                    let api_key = std::env::var("ANTHROPIC_API_KEY")
                        .ok()
                        .ok_or(OrchestratorError::MissingApiKey)?;
                    (
                        LLMProvider::Anthropic,
                        api_key,
                        "claude-sonnet-4-20250514".to_string(),
                        "You are the Athena Orchestrator.".to_string(),
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

        match provider {
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
        }
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
                let executor = match executor_arc.lock() {
                    Ok(guard) => guard,
                    Err(e) => {
                        return (format!("Tool executor lock poisoned: {}", e), true);
                    }
                };
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
        api_key: String,
        model: String,
        system_prompt: String,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        // Append user message
        {
            let mut msgs = self
                .anthropic_messages
                .lock()
                .map_err(|_| OrchestratorError::Generic("Failed to lock messages".to_string()))?;
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
                let msgs = self.anthropic_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
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
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2024-10-22")
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response.text().await.unwrap_or_default();
                return Err(OrchestratorError::Generic(format!(
                    "Anthropic API error {}: {}",
                    status, err_text
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
                let mut msgs = self.anthropic_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
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
                let mut msgs = self.anthropic_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;

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
        api_key: String,
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
            let mut msgs = self
                .openai_messages
                .lock()
                .map_err(|_| OrchestratorError::Generic("Failed to lock messages".to_string()))?;
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
            let msgs = self
                .openai_messages
                .lock()
                .map_err(|_| OrchestratorError::Generic("Failed to lock messages".to_string()))?;
            msgs.len() - 1
        };

        loop {
            // Enforce rate limiting before each API request
            self.rate_limiter.wait_if_needed().await;

            let body_messages: Vec<serde_json::Value> = {
                let msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;

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
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&body)
                .send()
                .await?;

            if !response.status().is_success() {
                let status = response.status();
                let err_text = response.text().await.unwrap_or_default();
                let mut msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
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
                let mut msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;

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
                let mut msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;

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
