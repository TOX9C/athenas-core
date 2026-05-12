use crate::types::*;
use std::sync::{Arc, Mutex};

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

/// The Athena orchestrator that dispatches messages to LLM providers.
pub struct AthenaOrchestrator {
    anthropic_messages: Arc<Mutex<Vec<AnthropicMessage>>>,
    openai_messages: Arc<Mutex<Vec<OpenAIMessage>>>,
    current_session_id: Arc<Mutex<Option<String>>>,
}

impl Default for AthenaOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

impl AthenaOrchestrator {
    pub fn new() -> Self {
        Self {
            anthropic_messages: Arc::new(Mutex::new(Vec::new())),
            openai_messages: Arc::new(Mutex::new(Vec::new())),
            current_session_id: Arc::new(Mutex::new(None)),
        }
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

    /// Send a message to the configured LLM provider.
    pub async fn send_message(
        &self,
        text: String,
        images: Option<Vec<ImageData>>,
    ) -> Result<String, OrchestratorError> {
        // For this migration we read a default provider config;
        // callers can wrap this to read from a config store.
        let provider = LLMProvider::Anthropic;
        let api_key = std::env::var("ANTHROPIC_API_KEY").ok();
        let model = "claude-sonnet-4-20250514".to_string();
        let system_prompt = "You are the Athena Orchestrator.".to_string();

        if api_key.is_none() {
            return Err(OrchestratorError::MissingApiKey);
        }
        let api_key = api_key.unwrap();

        if images.is_some() && !images.as_ref().unwrap().is_empty() && provider == LLMProvider::Lmstudio
        {
            return Err(OrchestratorError::LmStudioVisionNotSupported);
        }

        match provider {
            LLMProvider::NvidiaNim | LLMProvider::OpenAI | LLMProvider::Lmstudio => {
                let base_url = match provider {
                    LLMProvider::NvidiaNim => Some("https://integrate.api.nvidia.com/v1".to_string()),
                    LLMProvider::Lmstudio => {
                        Some(std::env::var("ATHENA_LMSTUDIO_BASE_URL")
                            .unwrap_or_else(|_| "http://localhost:1234/v1".to_string()))
                    }
                    _ => None,
                };
                self.send_openai(api_key, model, system_prompt, text, images, base_url).await
            }
            LLMProvider::Anthropic => {
                self.send_anthropic(api_key, model, system_prompt, text, images).await
            }
        }
    }

    /// Send a message using Anthropic's Messages API.
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
            let mut msgs = self.anthropic_messages.lock().map_err(|_| {
                OrchestratorError::Generic("Failed to lock messages".to_string())
            })?;
            msgs.push(AnthropicMessage {
                role: "user".to_string(),
                content: build_anthropic_content(&text, images.as_deref()),
            });
        }

        let client = reqwest::Client::new();
        let url = "https://api.anthropic.com/v1/messages";

        loop {
            let messages = {
                let msgs = self.anthropic_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
                msgs.clone()
            };

            let body = serde_json::json!({
                "model": model,
                "max_tokens": 4096,
                "system": system_prompt,
                "messages": messages
            });

            let response = client
                .post(url)
                .header("x-api-key", &api_key)
                .header("anthropic-version", "2023-06-01")
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
                OrchestratorError::Generic("Invalid Anthropic response: no content array".to_string())
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

            // Push assistant response
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

            // Execute tool calls and append results
            for _tool_call in &tool_calls {
                // Tool execution would go here; for the port we append a placeholder.
                let tool_result = serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": "",
                    "content": "Tool execution not yet implemented in Rust port."
                });

                let mut msgs = self.anthropic_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
                // For simplicity, we keep the tool result in the history
                msgs.push(AnthropicMessage {
                    role: "user".to_string(),
                    content: tool_result,
                });
            }

            // Continue loop for next iteration after tool results
            continue;
        }
    }

    /// Send a message using an OpenAI-compatible chat completions API.
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
            Some(base) => format!("{}/chat/completions", base.trim_end_matches("/")),
            None => "https://api.openai.com/v1/chat/completions".to_string(),
        };

        let client = reqwest::Client::new();

        // Build or update messages with system prompt
        {
            let mut msgs = self.openai_messages.lock().map_err(|_| {
                OrchestratorError::Generic("Failed to lock messages".to_string())
            })?;
            if msgs.is_empty() || msgs.first().map_or(true, |m| m.role != "system") {
                let new_system = OpenAIMessage {
                    role: "system".to_string(),
                    content: serde_json::Value::String(system_prompt),
                };
                if msgs.is_empty() {
                    msgs.push(new_system);
                } else {
                    msgs.insert(0, new_system);
                }
            } else {
                // Update existing system prompt
                msgs[0].content = serde_json::Value::String(system_prompt);
            }

            msgs.push(OpenAIMessage {
                role: "user".to_string(),
                content: build_openai_content(&text, images.as_deref()),
            });
        }

        loop {
            let body_messages: Vec<serde_json::Value> = {
                let msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
                msgs.iter()
                    .map(|m| {
                        serde_json::json!({
                            "role": &m.role,
                            "content": &m.content
                        })
                    })
                    .collect()
            };

            let body = serde_json::json!({
                "model": model,
                "max_tokens": 4096,
                "messages": body_messages
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
                if !msgs.is_empty() {
                    msgs.pop();
                }
                return Err(OrchestratorError::Generic(format!(
                    "OpenAI API error {}: {}",
                    status, err_text
                )));
            }

            let json: serde_json::Value = response.json().await?;
            let choice = &json["choices"][0];
            let message = &choice["message"];
            let raw_content = message["content"].as_str().unwrap_or("").trim();

            // Store assistant message
            {
                let mut msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
                msgs.push(OpenAIMessage {
                    role: "assistant".to_string(),
                    content: serde_json::Value::String(raw_content.to_string()),
                });
            }

            if message["tool_calls"].is_null() || message["tool_calls"].as_array().map_or(true, |a| a.is_empty())
            {
                return Ok(raw_content.to_string());
            }

            // Execute tool calls (placeholder)
            {
                let mut msgs = self.openai_messages.lock().map_err(|_| {
                    OrchestratorError::Generic("Failed to lock messages".to_string())
                })?;
                // Tool results placeholder
                msgs.push(OpenAIMessage {
                    role: "user".to_string(),
                    content: serde_json::Value::String(
                        "Tool execution not yet implemented in Rust port.".to_string(),
                    ),
                });
            }

            // Continue loop after tool results
            continue;
        }
    }
}
