//! Request-scoped provider streaming for Athena.
//!
//! This module deliberately keeps streaming separate from the legacy
//! request/response methods. The legacy commands remain compatible while the
//! desktop composer uses this path with a request ID and cancellation token.

use super::{
    build_anthropic_content, build_openai_content, sanitize_error_message, to_openai_tools,
    AnthropicMessage, AthenaOrchestrator, OpenAIMessage, OrchestratorError, ANTHROPIC_VERSION,
    MAX_OUTPUT_TOKENS, SYSTEM_PROMPT,
};
use crate::types::{AthenaStreamEvent, ImageData, LLMProvider, StreamEmitter};
use futures_util::StreamExt;
use secrecy::ExposeSecret;
use std::collections::BTreeMap;
use tokio_util::sync::CancellationToken;

const MAX_TOOL_ROUNDS: usize = 12;

impl AthenaOrchestrator {
    /// Register a stream emitter used by the Tauri command adapter.
    pub fn set_stream_emitter(&self, emitter: Option<StreamEmitter>) {
        *self.stream_emitter.lock() = emitter;
    }

    /// Start a request cancellation handle. The caller owns the returned token.
    pub fn register_request(&self, request_id: &str) -> Result<CancellationToken, String> {
        let mut requests = self.active_requests.lock();
        if requests.contains_key(request_id) {
            return Err(format!("request '{}' is already active", request_id));
        }
        let token = CancellationToken::new();
        requests.insert(request_id.to_string(), token.clone());
        Ok(token)
    }

    /// Cancel an active request. Returns false when the request has already ended.
    pub fn cancel_request(&self, request_id: &str) -> bool {
        let cancelled = self
            .active_requests
            .lock()
            .get(request_id)
            .map(|token| token.cancel())
            .is_some();
        if cancelled {
            if let Some(sender) = self.tool_event_sender.lock().clone() {
                sender.cancel_request(request_id);
            }
        }
        cancelled
    }

    fn finish_request(&self, request_id: &str) {
        self.active_requests.lock().remove(request_id);
    }

    fn emit_stream(&self, event: AthenaStreamEvent) {
        if let Some(emitter) = self.stream_emitter.lock().clone() {
            emitter(event);
        }
    }

    /// Stream one assistant turn. Conversation state is serialized for this
    /// path so two sessions cannot interleave their histories or late chunks.
    pub async fn stream_message(
        &self,
        request_id: String,
        session_id: String,
        text: String,
        images: Option<Vec<ImageData>>,
        cancel: CancellationToken,
    ) -> Result<String, OrchestratorError> {
        let _conversation_guard = self.conversation_lock.lock().await;
        self.emit_stream(AthenaStreamEvent::Started {
            request_id: request_id.clone(),
            session_id: session_id.clone(),
        });

        if let Err(error) = self.load_conversation(&session_id).await {
            self.emit_stream(AthenaStreamEvent::Error {
                request_id: request_id.clone(),
                message: error.to_string(),
                cancelled: false,
            });
            if let Some(sender) = self.tool_event_sender.lock().clone() {
                sender.finish_request(&request_id);
            }
            self.finish_request(&request_id);
            return Err(error);
        }
        self.set_current_session_id(session_id.clone());

        let (provider, api_key, model, system_prompt, base_url) = match self.stream_config() {
            Ok(config) => config,
            Err(error) => {
                self.emit_stream(AthenaStreamEvent::Error {
                    request_id: request_id.clone(),
                    message: error.to_string(),
                    cancelled: false,
                });
                if let Some(sender) = self.tool_event_sender.lock().clone() {
                    sender.finish_request(&request_id);
                }
                self.finish_request(&request_id);
                return Err(error);
            }
        };
        if let Some(ref base_url) = base_url {
            if let Err(error) = super::validate_base_url(base_url) {
                self.emit_stream(AthenaStreamEvent::Error {
                    request_id: request_id.clone(),
                    message: error.to_string(),
                    cancelled: false,
                });
                if let Some(executor) = self.tool_executor.as_ref() {
                    let executor = executor.lock();
                    executor.clear_request_context();
                    executor.finish_request(&request_id);
                }
                self.finish_request(&request_id);
                return Err(error);
            }
        }
        if matches!(provider, LLMProvider::Lmstudio)
            && images.as_ref().is_some_and(|v| !v.is_empty())
        {
            let error = OrchestratorError::LmStudioVisionNotSupported;
            self.emit_stream(AthenaStreamEvent::Error {
                request_id: request_id.clone(),
                message: error.to_string(),
                cancelled: false,
            });
            if let Some(executor) = self.tool_executor.as_ref() {
                let executor = executor.lock();
                executor.clear_request_context();
                executor.finish_request(&request_id);
            }
            self.finish_request(&request_id);
            return Err(error);
        }
        let result = match provider {
            LLMProvider::Anthropic => {
                self.stream_anthropic(
                    &request_id,
                    &session_id,
                    api_key,
                    model,
                    system_prompt,
                    text,
                    images,
                    cancel.clone(),
                )
                .await
            }
            LLMProvider::OpenAI | LLMProvider::NvidiaNim | LLMProvider::Lmstudio => {
                let base = match provider {
                    LLMProvider::NvidiaNim => "https://integrate.api.nvidia.com/v1".to_string(),
                    LLMProvider::OpenAI => {
                        base_url.unwrap_or_else(|| "https://api.openai.com/v1".to_string())
                    }
                    LLMProvider::Lmstudio => {
                        base_url.unwrap_or_else(|| "http://localhost:1234/v1".to_string())
                    }
                    LLMProvider::Anthropic => unreachable!(),
                };
                self.stream_openai(
                    &request_id,
                    &session_id,
                    api_key,
                    model,
                    system_prompt,
                    text,
                    images,
                    base,
                    cancel.clone(),
                )
                .await
            }
        };

        match &result {
            Ok(reply) => {
                if let Err(error) = self.try_auto_save().await {
                    log::warn!("Failed to auto-save streamed conversation: {}", error);
                }
                self.emit_stream(AthenaStreamEvent::Completed {
                    request_id: request_id.clone(),
                    text: reply.clone(),
                });
            }
            Err(error) => {
                self.emit_stream(AthenaStreamEvent::Error {
                    request_id: request_id.clone(),
                    message: error.to_string(),
                    cancelled: matches!(error, OrchestratorError::UserCancellation),
                });
            }
        }
        if let Some(sender) = self.tool_event_sender.lock().clone() {
            sender.finish_request(&request_id);
        }
        self.finish_request(&request_id);
        result
    }

    fn stream_config(
        &self,
    ) -> Result<
        (
            LLMProvider,
            secrecy::SecretString,
            String,
            String,
            Option<String>,
        ),
        OrchestratorError,
    > {
        let snapshot = self.build_app_state_snapshot();
        let guard = self.provider_config.lock();
        if let Some(config) = guard.as_ref() {
            let base = if config.system_prompt.trim().is_empty() {
                SYSTEM_PROMPT
            } else {
                config.system_prompt.as_str()
            };
            return Ok((
                config.provider.clone(),
                config.api_key().clone(),
                config.model.clone(),
                format!("{}\n\n{}", base, snapshot),
                config.base_url.clone(),
            ));
        }
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map(secrecy::SecretString::from)
            .map_err(|_| OrchestratorError::MissingApiKey)?;
        Ok((
            LLMProvider::Anthropic,
            key,
            super::DEFAULT_ANTHROPIC_MODEL.to_string(),
            format!("{}\n\n{}", SYSTEM_PROMPT, snapshot),
            None,
        ))
    }

    // Provider streaming keeps request fields explicit to preserve the
    // provider-specific call boundary and cancellation semantics.
    #[allow(clippy::too_many_arguments)]
    async fn stream_openai(
        &self,
        request_id: &str,
        session_id: &str,
        api_key: secrecy::SecretString,
        model: String,
        system_prompt: String,
        text: String,
        images: Option<Vec<ImageData>>,
        base_url: String,
        cancel: CancellationToken,
    ) -> Result<String, OrchestratorError> {
        let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));
        {
            let mut messages = self.openai_messages.lock();
            messages.insert(
                0,
                OpenAIMessage {
                    role: "system".to_string(),
                    content: serde_json::Value::String(system_prompt),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                },
            );
            messages.push(OpenAIMessage {
                role: "user".to_string(),
                content: build_openai_content(&text, images.as_deref()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }

        for round in 0..MAX_TOOL_ROUNDS {
            if cancel.is_cancelled() {
                return Err(OrchestratorError::UserCancellation);
            }
            self.emit_stream(AthenaStreamEvent::Status {
                request_id: request_id.to_string(),
                message: if round == 0 {
                    "Thinking…".to_string()
                } else {
                    "Running tools…".to_string()
                },
            });
            let body_messages: Vec<serde_json::Value> = self
                .openai_messages
                .lock()
                .iter()
                .map(|message| {
                    let mut value = serde_json::json!({
                        "role": message.role,
                        "content": message.content,
                    });
                    if let Some(calls) = &message.tool_calls {
                        value["tool_calls"] = calls.clone();
                    }
                    if let Some(id) = &message.tool_call_id {
                        value["tool_call_id"] = serde_json::Value::String(id.clone());
                    }
                    if let Some(name) = &message.name {
                        value["name"] = serde_json::Value::String(name.clone());
                    }
                    value
                })
                .collect();
            let body = serde_json::json!({
                "model": model,
                "max_tokens": MAX_OUTPUT_TOKENS,
                "messages": body_messages,
                "tools": to_openai_tools(),
                "tool_choice": "auto",
                "stream": true,
            });
            let response = tokio::select! {
                _ = cancel.cancelled() => return Err(OrchestratorError::UserCancellation),
                result = self.http_client.post(&url)
                    .header("Authorization", format!("Bearer {}", api_key.expose_secret()))
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send() => result?,
            };
            if !response.status().is_success() {
                let status = response.status();
                let error = sanitize_error_message(&response.text().await.unwrap_or_default());
                return Err(OrchestratorError::Generic(format!(
                    "OpenAI API error {}: {}",
                    status, error
                )));
            }

            let mut stream = response.bytes_stream();
            let mut pending = Vec::<u8>::new();
            let mut reply = String::new();
            let mut calls: BTreeMap<String, (String, String, String)> = BTreeMap::new();
            while let Some(chunk) = tokio::select! {
                _ = cancel.cancelled() => return Err(OrchestratorError::UserCancellation),
                chunk = stream.next() => chunk,
            } {
                pending.extend_from_slice(&chunk?);
                while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                    let line = String::from_utf8(pending.drain(..=newline).collect()).map_err(
                        |error| OrchestratorError::Generic(format!("Invalid SSE UTF-8: {error}")),
                    )?;
                    let data = line.trim().strip_prefix("data: ").unwrap_or("").trim();
                    if data.is_empty() || data == "[DONE]" {
                        continue;
                    }
                    let value: serde_json::Value = serde_json::from_str(data)?;
                    let delta = &value["choices"][0]["delta"];
                    if let Some(part) = delta["content"].as_str() {
                        reply.push_str(part);
                        self.emit_stream(AthenaStreamEvent::Delta {
                            request_id: request_id.to_string(),
                            text: part.to_string(),
                        });
                    }
                    if let Some(items) = delta["tool_calls"].as_array() {
                        for item in items {
                            let index = item["index"].as_u64().unwrap_or(0).to_string();
                            let entry = calls
                                .entry(index)
                                .or_insert_with(|| (String::new(), String::new(), String::new()));
                            if let Some(id) = item["id"].as_str() {
                                entry.0 = id.to_string();
                            }
                            if let Some(name) = item["function"]["name"].as_str() {
                                entry.1.push_str(name);
                            }
                            if let Some(args) = item["function"]["arguments"].as_str() {
                                entry.2.push_str(args);
                            }
                        }
                    }
                }
            }
            if calls.is_empty() {
                self.openai_messages.lock().push(OpenAIMessage {
                    role: "assistant".to_string(),
                    content: serde_json::Value::String(reply.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                return Ok(reply.trim().to_string());
            }
            let mut tool_calls = Vec::new();
            for (_index, (id, name, args)) in calls {
                tool_calls.push(serde_json::json!({"id": id, "type": "function", "function": {"name": name, "arguments": args}}));
            }
            self.openai_messages.lock().push(OpenAIMessage {
                role: "assistant".to_string(),
                content: if reply.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(reply)
                },
                tool_calls: Some(serde_json::Value::Array(tool_calls.clone())),
                tool_call_id: None,
                name: None,
            });
            for call in tool_calls {
                let name = call["function"]["name"].as_str().unwrap_or("unknown");
                let args = call["function"]["arguments"].as_str().unwrap_or("{}");
                let input = serde_json::from_str(args).unwrap_or_else(|_| serde_json::json!({}));
                let (result, is_error) = tokio::select! {
                    _ = cancel.cancelled() => return Err(OrchestratorError::UserCancellation),
                    result = self.execute_tool_with_context(name, &input, Some((request_id, session_id))) => result,
                };
                self.openai_messages.lock().push(OpenAIMessage {
                    role: "tool".to_string(),
                    content: if is_error {
                        serde_json::json!({"error": result})
                    } else {
                        serde_json::Value::String(result)
                    },
                    tool_calls: None,
                    tool_call_id: call["id"].as_str().map(str::to_string),
                    name: Some(name.to_string()),
                });
            }
        }
        Err(OrchestratorError::Generic(format!(
            "Tool loop exceeded {} rounds",
            MAX_TOOL_ROUNDS
        )))
    }

    // Provider streaming keeps request fields explicit to preserve the
    // provider-specific call boundary and cancellation semantics.
    #[allow(clippy::too_many_arguments)]
    async fn stream_anthropic(
        &self,
        request_id: &str,
        session_id: &str,
        api_key: secrecy::SecretString,
        model: String,
        system_prompt: String,
        text: String,
        images: Option<Vec<ImageData>>,
        cancel: CancellationToken,
    ) -> Result<String, OrchestratorError> {
        {
            self.anthropic_messages.lock().push(AnthropicMessage {
                role: "user".to_string(),
                content: build_anthropic_content(&text, images.as_deref()),
            });
            self.openai_messages.lock().push(OpenAIMessage {
                role: "user".to_string(),
                content: build_openai_content(&text, images.as_deref()),
                tool_calls: None,
                tool_call_id: None,
                name: None,
            });
        }
        for round in 0..MAX_TOOL_ROUNDS {
            if cancel.is_cancelled() {
                return Err(OrchestratorError::UserCancellation);
            }
            self.emit_stream(AthenaStreamEvent::Status {
                request_id: request_id.to_string(),
                message: if round == 0 {
                    "Thinking…".into()
                } else {
                    "Running tools…".into()
                },
            });
            let messages = self.anthropic_messages.lock().clone();
            let body = serde_json::json!({
                "model": model,
                "max_tokens": MAX_OUTPUT_TOKENS,
                "system": system_prompt,
                "messages": messages,
                "tools": to_openai_tools().iter().map(|tool| serde_json::json!({"name": tool.function.name, "description": tool.function.description, "input_schema": tool.function.parameters})).collect::<Vec<_>>(),
                "tool_choice": {"type": "auto"},
                "stream": true,
            });
            let response = tokio::select! {
                _ = cancel.cancelled() => return Err(OrchestratorError::UserCancellation),
                result = self.http_client.post(format!("{}/messages", self.anthropic_base_url.trim_end_matches('/')))
                    .header("x-api-key", api_key.expose_secret())
                    .header("anthropic-version", ANTHROPIC_VERSION)
                    .header("Content-Type", "application/json")
                    .json(&body)
                    .send() => result?,
            };
            if !response.status().is_success() {
                let status = response.status();
                return Err(OrchestratorError::Generic(format!(
                    "Anthropic API error {}: {}",
                    status,
                    sanitize_error_message(&response.text().await.unwrap_or_default())
                )));
            }
            let mut stream = response.bytes_stream();
            let mut pending = Vec::<u8>::new();
            let mut reply = String::new();
            let mut blocks: BTreeMap<usize, serde_json::Value> = BTreeMap::new();
            while let Some(chunk) = tokio::select! {
                _ = cancel.cancelled() => return Err(OrchestratorError::UserCancellation),
                chunk = stream.next() => chunk,
            } {
                pending.extend_from_slice(&chunk?);
                while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                    let line = String::from_utf8(pending.drain(..=newline).collect()).map_err(
                        |error| OrchestratorError::Generic(format!("Invalid SSE UTF-8: {error}")),
                    )?;
                    let data = line.trim().strip_prefix("data: ").unwrap_or("").trim();
                    if data.is_empty() {
                        continue;
                    }
                    let event: serde_json::Value = serde_json::from_str(data)?;
                    let index = event["index"].as_u64().unwrap_or(0) as usize;
                    match event["type"].as_str().unwrap_or("") {
                        "content_block_start" => {
                            blocks.insert(index, event["content_block"].clone());
                        }
                        "content_block_delta" => {
                            let delta = &event["delta"];
                            match delta["type"].as_str().unwrap_or("") {
                                "text_delta" => {
                                    if let Some(part) = delta["text"].as_str() {
                                        reply.push_str(part);
                                        self.emit_stream(AthenaStreamEvent::Delta {
                                            request_id: request_id.to_string(),
                                            text: part.to_string(),
                                        });
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(part) = delta["partial_json"].as_str() {
                                        if let Some(block) = blocks.get_mut(&index) {
                                            let current = block["input_json"]
                                                .as_str()
                                                .unwrap_or("")
                                                .to_string();
                                            block["input_json"] = serde_json::Value::String(
                                                format!("{}{}", current, part),
                                            );
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut tool_blocks = Vec::new();
            let mut content = Vec::new();
            for (_index, mut block) in blocks {
                if block["type"] == "tool_use" {
                    let input = block["input"].clone();
                    let streamed_json = block["input_json"].as_str().unwrap_or("");
                    let parsed = if !streamed_json.is_empty() {
                        serde_json::from_str(streamed_json)?
                    } else {
                        input
                    };
                    block["input"] = parsed;
                    tool_blocks.push(block.clone());
                }
                content.push(block);
            }
            self.anthropic_messages.lock().push(AnthropicMessage {
                role: "assistant".into(),
                content: serde_json::Value::Array(content),
            });
            if tool_blocks.is_empty() {
                self.openai_messages.lock().push(OpenAIMessage {
                    role: "assistant".into(),
                    content: serde_json::Value::String(reply.clone()),
                    tool_calls: None,
                    tool_call_id: None,
                    name: None,
                });
                return Ok(reply.trim().to_string());
            }
            let mut tool_results = Vec::with_capacity(tool_blocks.len());
            for block in tool_blocks {
                let name = block["name"].as_str().unwrap_or("unknown");
                let (result, is_error) = tokio::select! {
                    _ = cancel.cancelled() => return Err(OrchestratorError::UserCancellation),
                    result = self.execute_tool_with_context(name, &block["input"], Some((request_id, session_id))) => result,
                };
                tool_results.push(serde_json::json!({
                    "type": "tool_result",
                    "tool_use_id": block["id"],
                    "content": result,
                    "is_error": is_error,
                }));
            }
            self.anthropic_messages.lock().push(AnthropicMessage {
                role: "user".into(),
                content: serde_json::Value::Array(tool_results),
            });
        }
        Err(OrchestratorError::Generic(format!(
            "Tool loop exceeded {} rounds",
            MAX_TOOL_ROUNDS
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_request_ids_are_rejected() {
        let orchestrator = AthenaOrchestrator::new();
        let first = orchestrator
            .register_request("request-1")
            .expect("first request");
        assert!(orchestrator.register_request("request-1").is_err());
        assert!(orchestrator.cancel_request("request-1"));
        assert!(first.is_cancelled());
        assert!(!orchestrator.cancel_request("missing"));
    }
}
