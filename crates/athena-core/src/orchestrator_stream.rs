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

/// Upper bound on tool calls from one assistant turn executing at once.
/// Models rarely exceed 5 independent calls per turn; 4 keeps fan-out
/// predictable without starving the blocking pool.
pub(super) const MAX_CONCURRENT_TOOLS: usize = 4;

impl AthenaOrchestrator {
    /// Register a stream emitter used by the Tauri command adapter.
    pub fn set_stream_emitter(&self, emitter: Option<StreamEmitter>) {
        *self.stream_emitter.lock() = emitter.clone();
        self.stream_coalescer.set_emitter(emitter);
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
        self.stream_coalescer.emit(event);
    }

    /// Buffer one streamed text fragment; flushed per the coalescer policy.
    fn emit_delta(&self, request_id: &str, text: &str) {
        self.stream_coalescer.push(request_id, text);
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
                model_unavailable: false,
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
                    model_unavailable: false,
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
                    model_unavailable: false,
                });
                if let Some(executor) = self.tool_executor.as_ref() {
                    let executor = executor.read();
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
                model_unavailable: false,
            });
            if let Some(executor) = self.tool_executor.as_ref() {
                let executor = executor.read();
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
                    LLMProvider::NvidiaNim => base_url
                        .unwrap_or_else(|| "https://integrate.api.nvidia.com/v1".to_string()),
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
                self.try_auto_save();
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
                    model_unavailable: matches!(error, OrchestratorError::ModelUnavailable { .. }),
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
                return Err(super::orchestrator_support::classify_api_error(
                    "OpenAI", status, error,
                ));
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
                        self.emit_delta(request_id, part);
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
            // OpenAI's wire contract types `function.arguments` as a STRING
            // containing JSON (their SSE delivers it in string fragments), so
            // history and every request body must carry the raw string.
            // Parse results live in a side map: malformed model-emitted JSON
            // is surfaced at execution time as an error tool message —
            // self-correctable by the model — never via a request-aborting `?`.
            let mut parsed_inputs: BTreeMap<String, Result<serde_json::Value, String>> =
                BTreeMap::new();
            let mut tool_calls = Vec::new();
            for (_index, (id, name, args)) in calls {
                parsed_inputs.insert(
                    id.clone(),
                    match serde_json::from_str::<serde_json::Value>(&args) {
                        Ok(value @ serde_json::Value::Object(_)) => Ok(value),
                        Ok(_) => Err("expected a JSON object for tool call arguments".to_string()),
                        Err(error) => Err(error.to_string()),
                    },
                );
                tool_calls.push(serde_json::json!({
                    "id": id,
                    "type": "function",
                    "function": {"name": name, "arguments": args},
                }));
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
            let mut tool_reply_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            // Calls whose arguments failed to parse skip execution entirely;
            // they get an error tool message below (F3 contract).
            let mut executable: Vec<(String, serde_json::Value)> = Vec::new();
            for call in &tool_calls {
                let call_id = call["id"].as_str().unwrap_or("");
                match parsed_inputs.get(call_id) {
                    Some(Ok(value)) => executable.push((
                        call["function"]["name"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                        value.clone(),
                    )),
                    Some(Err(error)) => {
                        // Malformed model-emitted JSON surfaces as an error
                        // tool message (self-correctable by the model), never
                        // silently executes with empty arguments.
                        self.openai_messages.lock().push(OpenAIMessage {
                            role: "tool".to_string(),
                            content: serde_json::json!({
                                "error": format!("invalid JSON in tool arguments: {}", error)
                            }),
                            tool_calls: None,
                            tool_call_id: Some(call_id.to_string()),
                            name: Some(
                                call["function"]["name"]
                                    .as_str()
                                    .unwrap_or("unknown")
                                    .to_string(),
                            ),
                        });
                        tool_reply_ids.insert(call_id.to_string());
                    }
                    None => executable.push((
                        call["function"]["name"]
                            .as_str()
                            .unwrap_or("unknown")
                            .to_string(),
                        serde_json::Value::Object(Default::default()),
                    )),
                }
            }
            // Concurrent execution, results back in input order (`buffered`).
            // Cancellation fails not-yet-finished calls; every call id still
            // needs a matching tool-role reply before returning.
            if !executable.is_empty() {
                let batch_results = self
                    .execute_tool_batch(
                        std::mem::take(&mut executable),
                        request_id,
                        session_id,
                        &cancel,
                    )
                    .await;
                // Collect every reply first, then append under ONE lock
                // acquisition so history transitions atomically from "assistant
                // spoke" to "every tool_call answered" (no reader can observe an
                // intermediate unpaired state).
                let mut result_iter = batch_results.into_iter();
                let mut replies = Vec::<OpenAIMessage>::with_capacity(tool_calls.len());
                for call in &tool_calls {
                    let call_id = call["id"].as_str().unwrap_or("");
                    if tool_reply_ids.contains(call_id) {
                        continue;
                    }
                    let (result, is_error) = match result_iter.next() {
                        Some(Ok(pair)) => pair,
                        Some(Err(_)) | None => ("cancelled by user".to_string(), true),
                    };
                    replies.push(OpenAIMessage {
                        role: "tool".to_string(),
                        content: if is_error {
                            serde_json::json!({"error": result})
                        } else {
                            serde_json::Value::String(result)
                        },
                        tool_calls: None,
                        tool_call_id: Some(call_id.to_string()),
                        name: Some(
                            call["function"]["name"]
                                .as_str()
                                .unwrap_or("unknown")
                                .to_string(),
                        ),
                    });
                }
                self.openai_messages.lock().extend(replies);
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
                return Err(super::orchestrator_support::classify_api_error(
                    "Anthropic",
                    status,
                    sanitize_error_message(&response.text().await.unwrap_or_default()),
                ));
            }
            let mut stream = response.bytes_stream();
            let mut pending = Vec::<u8>::new();
            let mut reply = String::new();
            let mut blocks: BTreeMap<usize, serde_json::Value> = BTreeMap::new();
            // Accumulate `input_json_delta` fragments here instead of
            // read-modify-writing the block Value: appending to a String is
            // O(n) total, the Value round-trip was O(n²) per tool call.
            let mut streamed_inputs: BTreeMap<usize, String> = BTreeMap::new();
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
                                        self.emit_delta(request_id, part);
                                    }
                                }
                                "input_json_delta" => {
                                    if let Some(part) = delta["partial_json"].as_str() {
                                        streamed_inputs.entry(index).or_default().push_str(part);
                                    }
                                }
                                _ => {}
                            }
                        }
                        _ => {}
                    }
                }
            }
            let mut tool_blocks = Vec::<(usize, serde_json::Value)>::new();
            let mut content = Vec::new();
            // Malformed model-emitted JSON is recorded per index and surfaced
            // at execution time as an error tool_result — self-correctable by
            // the model on its next turn — never via a request-aborting `?`.
            let mut input_parse_errors: BTreeMap<usize, String> = BTreeMap::new();
            for (index, mut block) in blocks {
                if block["type"] == "tool_use" {
                    // Parse streamed fragments BEFORE the block enters
                    // history: providers require tool_use.input / arguments
                    // to be a JSON object, so the raw string must never leak
                    // into the assistant message or the next request body.
                    match streamed_inputs.get(&index) {
                        Some(raw) if !raw.is_empty() => match serde_json::from_str(raw) {
                            Ok(value) => block["input"] = value,
                            Err(error) => {
                                block["input"] = serde_json::json!({});
                                input_parse_errors.insert(index, error.to_string());
                            }
                        },
                        _ => {
                            if !block["input"].is_object() {
                                block["input"] = serde_json::json!({});
                            }
                        }
                    }
                    tool_blocks.push((index, block.clone()));
                }
                content.push(block);
            }
            // Parse-error blocks never execute; they get an error
            // tool_result below (F3 contract). Everything else runs
            // concurrently, results reassembled in block order.
            let mut tool_results = Vec::with_capacity(tool_blocks.len());
            let mut tool_result_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            let mut executable: Vec<(String, serde_json::Value)> = Vec::new();
            for (index, block) in &tool_blocks {
                if let Some(error) = input_parse_errors.get(index) {
                    tool_results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": format!("invalid JSON in tool arguments: {}", error),
                        "is_error": true,
                    }));
                    tool_result_ids.insert(block["id"].as_str().unwrap_or("").to_string());
                    continue;
                }
                executable.push((
                    block["name"].as_str().unwrap_or("unknown").to_string(),
                    block["input"].clone(),
                ));
            }
            self.anthropic_messages.lock().push(AnthropicMessage {
                role: "assistant".into(),
                content: serde_json::Value::Array(content),
            });
            if !executable.is_empty() {
                let batch_results = self
                    .execute_tool_batch(
                        std::mem::take(&mut executable),
                        request_id,
                        session_id,
                        &cancel,
                    )
                    .await;
                let mut result_iter = batch_results.into_iter();
                for (_index, block) in &tool_blocks {
                    let id = block["id"].as_str().unwrap_or("");
                    if tool_result_ids.contains(id) {
                        continue; // parse-error result already recorded
                    }
                    let (result, is_error) = match result_iter.next() {
                        Some(Ok(pair)) => pair,
                        Some(Err(_)) | None => {
                            // Cancelled mid-batch: synthetic error keeps the
                            // tool_use/tool_result pairing contract intact.
                            ("cancelled by user".to_string(), true)
                        }
                    };
                    tool_results.push(serde_json::json!({
                        "type": "tool_result",
                        "tool_use_id": block["id"],
                        "content": result,
                        "is_error": is_error,
                    }));
                    tool_result_ids.insert(id.to_string());
                }
            }
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

    fn count_ask_user_invocations(orch: &Arc<AthenaOrchestrator>) -> usize {
        // Observable proxy for "tool actually ran": a successful ask_user
        // leaves a non-error, non-cancelled tool reply in history. Malformed
        // args (F3) and cancellation (F1) replies don't count.
        let openai = serde_json::to_value(&*orch.openai_messages.lock()).unwrap_or_default();
        let anthropic = serde_json::to_value(&*orch.anthropic_messages.lock()).unwrap_or_default();
        let invoked_in = |msgs: serde_json::Value| -> bool {
            msgs.as_array().is_some_and(|msgs| {
                msgs.iter().any(|m| {
                    m["name"] == "ask_user"
                        && m["content"]
                            .get("error")
                            .and_then(|e| e.as_str())
                            .map(|e| !e.contains("invalid JSON") && !e.contains("cancelled"))
                            .unwrap_or(false)
                })
            })
        };
        usize::from(invoked_in(openai) || invoked_in(anthropic))
    }

    use crate::agent_comms::AgentComms;
    use crate::orchestrator::ProviderConfig;
    use crate::output_buffer::OutputBuffer;
    use crate::plan_manager::PlanManager;
    use crate::tool_executor::{ToolEventSender, ToolExecutor};
    use std::sync::Arc;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    // ── F1/F3 regression tests ──────────────────────────────────────────────
    //
    // These drive the real streaming tool loop against a local wiremock
    // server. F1: cancelling mid-tool must leave NO dangling tool_use /
    // tool_calls entry in history (every id gets a synthetic error reply),
    // and the NEXT request must serialize valid provider payloads. F3: a
    // tool_call whose `arguments` string is not valid JSON must become an
    // is_error tool reply — never a silent `{}` invocation.

    /// Event sender whose `ask_user` blocks long enough for the test to
    /// cancel the request mid-tool-execution.
    struct BlockingAskUserSender {
        block_ms: u64,
        invocations: InvocationCounter,
    }

    type InvocationCounter = std::sync::Arc<std::sync::atomic::AtomicUsize>;

    impl ToolEventSender for BlockingAskUserSender {
        fn agent_spawned(&self, _id: &str, _t: &str, _c: &str) {}
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
            self.invocations
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            std::thread::sleep(std::time::Duration::from_millis(self.block_ms));
            String::new()
        }
        fn plan_update(&self, _plan: &crate::plan_manager::ExecutionPlan) {}
        fn plan_evaluated(
            &self,
            _plan_id: &str,
            _s: &str,
            _steps: &[serde_json::Value],
            _n: &str,
            _r: &str,
        ) {
        }
    }

    fn blocking_orchestrator(
        base_url: String,
        block_ms: u64,
    ) -> (Arc<AthenaOrchestrator>, InvocationCounter) {
        let invocations: InvocationCounter = Arc::default();
        let ob = Arc::new(OutputBuffer::new());
        let pm = Arc::new(PlanManager::new());
        let ac = Arc::new(AgentComms::new());
        let store = Arc::new(athena_store::KeyValueStore::new_empty());
        let executor = Arc::new(parking_lot::RwLock::new(ToolExecutor::new(
            Arc::clone(&ob),
            Arc::clone(&pm),
            Arc::clone(&ac),
            Arc::new(BlockingAskUserSender {
                block_ms,
                invocations: Arc::clone(&invocations),
            }),
            Arc::clone(&store),
            None,
        )));
        let mut orch =
            AthenaOrchestrator::with_context(executor, ob, pm, ac, None, Some(store), None);
        orch.anthropic_base_url = base_url;
        (Arc::new(orch), invocations)
    }

    /// Same as [`blocking_orchestrator`] but wires a real (temp-dir)
    /// SessionStore so load_conversation/save_conversation behave as in
    /// production — required by the F7 per-session isolation test.
    fn blocking_orchestrator_with_store(
        base_url: String,
        block_ms: u64,
        session_store: Arc<athena_store::SessionStore>,
    ) -> (Arc<AthenaOrchestrator>, InvocationCounter) {
        let invocations: InvocationCounter = Arc::default();
        let ob = Arc::new(OutputBuffer::new());
        let pm = Arc::new(PlanManager::new());
        let ac = Arc::new(AgentComms::new());
        let store = Arc::new(athena_store::KeyValueStore::new_empty());
        let executor = Arc::new(parking_lot::RwLock::new(ToolExecutor::new(
            Arc::clone(&ob),
            Arc::clone(&pm),
            Arc::clone(&ac),
            Arc::new(BlockingAskUserSender {
                block_ms,
                invocations: Arc::clone(&invocations),
            }),
            Arc::clone(&store),
            None,
        )));
        let mut orch = AthenaOrchestrator::with_context(
            executor,
            ob,
            pm,
            ac,
            Some(session_store),
            Some(store),
            None,
        );
        orch.anthropic_base_url = base_url;
        (Arc::new(orch), invocations)
    }

    fn openai_config(server_uri: String) -> ProviderConfig {
        ProviderConfig::new(
            LLMProvider::OpenAI,
            "test-key",
            "gpt-test".to_string(),
            String::new(),
            Some(server_uri),
        )
    }

    fn anthropic_config() -> ProviderConfig {
        ProviderConfig::new(
            LLMProvider::Anthropic,
            "test-key",
            "claude-test".to_string(),
            String::new(),
            None,
        )
    }

    /// OpenAI chat-completions SSE carrying one complete tool call with
    /// VALID JSON arguments (`{"question":"hi?"}` as the arguments string).
    fn openai_tool_call_sse() -> String {
        let args = r#"{"question":"hi?"}"#;
        let chunk = serde_json::json!({
            "choices":[{"delta":{"role":"assistant","tool_calls":[
                {"index":0,"id":"call_1","type":"function",
                 "function":{"name":"ask_user","arguments":args}}
            ]}}]
        });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    /// OpenAI SSE with a malformed JSON arguments string (F3).
    fn openai_malformed_args_sse() -> String {
        let chunk = serde_json::json!({
            "choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"ask_user","arguments":"{oops"}}]}}]
        });
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    fn openai_plain_sse(text: &str) -> String {
        let chunk = serde_json::json!({"choices":[{"delta":{"content":text}}]});
        format!("data: {chunk}\n\ndata: [DONE]\n\n")
    }

    /// Anthropic Messages SSE: one complete tool_use block whose streamed
    /// `input_json_delta` fragments concatenate to valid JSON. The delta
    /// event is built with `serde_json::json!` so the raw JSON string is
    /// escaped correctly on the wire.
    fn anthropic_tool_use_sse() -> String {
        let input = r#"{"question":"hi?"}"#;
        let delta = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": input}
        });
        format!(
            concat!(
                "data: {{\"type\":\"message_start\",\"message\":{{}}}}\n\n",
                "data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"ask_user\",\"input\":{{}}}}}}\n\n",
                "data: {delta}\n\n",
                "data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
                "data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"tool_use\"}}}}\n\n",
                "data: {{\"type\":\"message_stop\"}}\n\n"
            ),
            delta = delta
        )
    }

    fn anthropic_malformed_sse() -> String {
        // The delta event is built with `serde_json::json!` so the wire line
        // is valid JSON whose `partial_json` VALUE is malformed tool input.
        let delta = serde_json::json!({
            "type": "content_block_delta",
            "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": "{oops"}
        });
        format!(
            concat!(
                "data: {{\"type\":\"message_start\",\"message\":{{}}}}\n\n",
                "data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"toolu_1\",\"name\":\"ask_user\",\"input\":{{}}}}}}\n\n",
                "data: {delta}\n\n",
                "data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
                "data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"tool_use\"}}}}\n\n",
                "data: {{\"type\":\"message_stop\"}}\n\n"
            ),
            delta = delta
        )
    }

    fn anthropic_plain_sse(text: &str) -> String {
        format!(
            concat!(
                "data: {{\"type\":\"message_start\",\"message\":{{}}}}\n\n",
                "data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"text\",\"text\":\"\"}}}}\n\n",
                "data: {{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{{\"type\":\"text_delta\",\"text\":\"{text}\"}}}}\n\n",
                "data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
                "data: {{\"type\":\"message_stop\"}}\n\n"
            ),
            text = text
        )
    }

    async fn mount_tool_then_text(server: &MockServer, tool_body: String, final_text: &str) {
        // wiremock serves the FIRST-registered matching mock; the one-shot
        // tool mock is mounted first so round 1 consumes it, and later
        // rounds fall through to the plain-text mock.
        Mock::given(method("POST"))
            .and(path("/chat/completions"))
            .respond_with(ResponseTemplate::new(200).set_body_string(tool_body.clone()))
            .up_to_n_times(1)
            .mount(server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(openai_plain_sse(final_text)))
            .mount(server)
            .await;
    }

    /// Every assistant tool_calls id in `messages` must be answered by a
    /// later role=="tool" message with the same tool_call_id.
    fn assert_openai_pairing(messages: &[serde_json::Value], context: &str) {
        let mut outstanding = Vec::<String>::new();
        for message in messages {
            match message["role"].as_str() {
                Some("assistant") => {
                    if let Some(calls) = message["tool_calls"].as_array() {
                        for call in calls {
                            if let Some(id) = call["id"].as_str() {
                                assert!(
                                    !outstanding.iter().any(|open| open == id),
                                    "{context}: duplicate unanswered tool_call id {id}"
                                );
                                outstanding.push(id.to_string());
                            }
                        }
                    }
                }
                Some("tool") => {
                    let id = message["tool_call_id"].as_str().unwrap_or("");
                    assert!(
                        !id.is_empty(),
                        "{context}: tool message missing tool_call_id"
                    );
                    let before = outstanding.len();
                    outstanding.retain(|open| open != id);
                    assert_eq!(
                        before - outstanding.len(),
                        1,
                        "{context}: tool reply for unknown/unpaired id {id}"
                    );
                }
                _ => {}
            }
        }
        assert!(
            outstanding.is_empty(),
            "{context}: dangling tool_use ids without replies: {outstanding:?}"
        );
    }

    /// Every tool_use block in an assistant content array must be answered by
    /// a later user message containing a matching tool_result block.
    fn assert_anthropic_pairing(messages: &[serde_json::Value], context: &str) {
        let mut outstanding = Vec::<String>::new();
        for message in messages {
            let Some(content) = message["content"].as_array() else {
                continue;
            };
            for block in content {
                match block["type"].as_str() {
                    Some("tool_use") => {
                        if let Some(id) = block["id"].as_str() {
                            outstanding.push(id.to_string());
                        }
                    }
                    Some("tool_result") => {
                        let id = block["tool_use_id"].as_str().unwrap_or("");
                        let before = outstanding.len();
                        outstanding.retain(|open| open != id);
                        assert!(
                            before - outstanding.len() == 1,
                            "{context}: tool_result for unknown/unpaired id {id}"
                        );
                    }
                    _ => {}
                }
            }
        }
        assert!(
            outstanding.is_empty(),
            "{context}: dangling tool_use ids: {outstanding:?}"
        );
    }

    async fn wait_until(mut predicate: impl FnMut() -> bool, timeout_ms: u64, what: &str) {
        let deadline = tokio::time::Instant::now() + std::time::Duration::from_millis(timeout_ms);
        while !predicate() {
            assert!(
                tokio::time::Instant::now() < deadline,
                "timed out waiting for {what}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn f1_openai_cancel_mid_tool_leaves_no_dangling_tool_calls() {
        let server = MockServer::start().await;
        mount_tool_then_text(&server, openai_tool_call_sse(), "all done").await;

        let (orch, invocations) = blocking_orchestrator(String::new(), 5_000);
        orch.set_provider_config(openai_config(server.uri()));

        let token = orch.register_request("f1-openai").expect("register");
        let orch_task = Arc::clone(&orch);
        let task = tokio::spawn(async move {
            orch_task
                .stream_message(
                    "f1-openai".to_string(),
                    "sess".to_string(),
                    "hello".to_string(),
                    None,
                    token,
                )
                .await
        });

        // Wait until ask_user is actually executing (the counter proves the
        // tool call parsed and started), then cancel mid-execution.
        wait_until(
            || invocations.load(std::sync::atomic::Ordering::SeqCst) == 1,
            5_000,
            "ask_user invocation",
        )
        .await;
        orch.cancel_request("f1-openai");

        let outcome = task.await.expect("task join");
        assert!(matches!(outcome, Err(OrchestratorError::UserCancellation)));

        let history = serde_json::to_value(&*orch.openai_messages.lock()).unwrap();
        let history = history.as_array().unwrap();
        assert_openai_pairing(history, "post-cancel history");
        let tool_reply = history
            .iter()
            .find(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
            .expect("synthetic tool reply");
        assert_eq!(tool_reply["content"]["error"], "cancelled by user");

        // Next turn must serialize a valid payload the provider accepts.
        let token2 = orch.register_request("f1-openai-2").expect("register");
        let orch2 = Arc::clone(&orch);
        let second = tokio::spawn(async move {
            orch2
                .stream_message(
                    "f1-openai-2".to_string(),
                    "sess".to_string(),
                    "continue".to_string(),
                    None,
                    token2,
                )
                .await
        })
        .await
        .expect("second join");
        assert_eq!(second.expect("second turn ok"), "all done");

        let received = server.received_requests().await.expect("received");
        assert!(received.len() >= 2, "expected two provider requests");
        let body: serde_json::Value =
            serde_json::from_slice(&received[1].body).expect("request json");
        assert_openai_pairing(body["messages"].as_array().unwrap(), "next-request payload");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn f1_anthropic_cancel_mid_tool_leaves_no_dangling_tool_use() {
        let server = MockServer::start().await;
        // Anthropic path hits POST /messages on anthropic_base_url.
        // First-registered mock wins: one-shot tool mock first, plain-text
        // catch-all second.
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(anthropic_tool_use_sse()))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(anthropic_plain_sse("done")))
            .mount(&server)
            .await;

        let (orch, invocations) = blocking_orchestrator(server.uri(), 5_000);
        orch.set_provider_config(anthropic_config());

        let token = orch.register_request("f1-anthropic").expect("register");
        let orch_run = Arc::clone(&orch);
        let task = tokio::spawn(async move {
            orch_run
                .stream_message(
                    "f1-anthropic".to_string(),
                    "sess".to_string(),
                    "hello".to_string(),
                    None,
                    token,
                )
                .await
        });

        // Wait until ask_user is actually executing, then cancel mid-tool.
        wait_until(
            || invocations.load(std::sync::atomic::Ordering::SeqCst) == 1,
            5_000,
            "ask_user invocation",
        )
        .await;
        orch.cancel_request("f1-anthropic");

        let outcome = task.await.expect("task join");
        assert!(matches!(outcome, Err(OrchestratorError::UserCancellation)));

        let history = serde_json::to_value(&*orch.anthropic_messages.lock()).unwrap();
        assert_anthropic_pairing(history.as_array().unwrap(), "post-cancel history");
        let cancelled_result = history
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
            .find(|b| b["type"] == "tool_result" && b["tool_use_id"] == "toolu_1")
            .expect("synthetic tool_result");
        assert_eq!(cancelled_result["is_error"], true);
        assert_eq!(cancelled_result["content"], "cancelled by user");

        let token2 = orch.register_request("f1-anthropic-2").expect("register");
        let orch2 = Arc::clone(&orch);
        let second = tokio::spawn(async move {
            orch2
                .stream_message(
                    "f1-anthropic-2".to_string(),
                    "sess".to_string(),
                    "go on".to_string(),
                    None,
                    token2,
                )
                .await
        })
        .await
        .expect("second join");
        assert_eq!(second.expect("second turn ok"), "done");

        let received = server.received_requests().await.expect("received");
        assert!(received.len() >= 2, "expected two provider requests");
        let body: serde_json::Value =
            serde_json::from_slice(&received[1].body).expect("request json");
        assert_anthropic_pairing(body["messages"].as_array().unwrap(), "next-request payload");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn f3_openai_malformed_arguments_become_error_reply_not_invocation() {
        let server = MockServer::start().await;
        mount_tool_then_text(&server, openai_malformed_args_sse(), "ok").await;

        let (orch, _invocations) = blocking_orchestrator(String::new(), 0);
        orch.set_provider_config(openai_config(server.uri()));

        let token = orch.register_request("f3-openai").expect("register");
        let orch_run = Arc::clone(&orch);
        let reply = tokio::spawn(async move {
            orch_run
                .stream_message(
                    "f3-openai".to_string(),
                    "sess".to_string(),
                    "hi".to_string(),
                    None,
                    token,
                )
                .await
        })
        .await
        .expect("join");
        assert_eq!(reply.expect("turn completes"), "ok");

        // The tool was never invoked…
        let sentinels = count_ask_user_invocations(&orch);
        assert_eq!(sentinels, 0, "malformed args must not invoke the tool");
        // …and the error tool reply is in history for the model.
        let history = serde_json::to_value(&*orch.openai_messages.lock()).unwrap();
        let history = history.as_array().unwrap();
        assert_openai_pairing(history, "malformed-args history");
        let error_reply = history
            .iter()
            .find(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
            .expect("error tool reply present");
        assert_eq!(
            error_reply["content"]["error"]
                .as_str()
                .map(|e| e.contains("invalid JSON")),
            Some(true)
        );

        // The follow-up request carried that error back to the provider.
        let received = server.received_requests().await.expect("received");
        assert!(received.len() >= 2, "model got a round-2 request");
        let body: serde_json::Value =
            serde_json::from_slice(&received[1].body).expect("request json");
        let carried = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["role"] == "tool" && m["tool_call_id"] == "call_1")
            .expect("error reply in next payload");
        assert_eq!(
            carried["content"]["error"]
                .as_str()
                .map(|e| e.contains("invalid JSON")),
            Some(true)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn f3_anthropic_malformed_input_becomes_is_error_tool_result() {
        let server = MockServer::start().await;
        // First-registered mock wins: malformed tool mock first, plain-text
        // catch-all second.
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(anthropic_malformed_sse()))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_string(anthropic_plain_sse("fine")))
            .mount(&server)
            .await;

        let (orch, _invocations) = blocking_orchestrator(server.uri(), 0);
        orch.set_provider_config(anthropic_config());

        let token = orch.register_request("f3-anthropic").expect("register");
        let orch_run = Arc::clone(&orch);
        let reply = tokio::spawn(async move {
            orch_run
                .stream_message(
                    "f3-anthropic".to_string(),
                    "sess".to_string(),
                    "hi".to_string(),
                    None,
                    token,
                )
                .await
        })
        .await
        .expect("join");
        assert_eq!(reply.expect("turn completes"), "fine");

        assert_eq!(count_ask_user_invocations(&orch), 0);
        let history = serde_json::to_value(&*orch.anthropic_messages.lock()).unwrap();
        assert_anthropic_pairing(history.as_array().unwrap(), "malformed history");
        let result = history
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|m| m["content"].as_array().cloned().unwrap_or_default())
            .find(|b| b["type"] == "tool_result" && b["tool_use_id"] == "toolu_1")
            .expect("error tool_result present");
        assert_eq!(result["is_error"], true);
        assert!(
            result["content"]
                .as_str()
                .unwrap_or("")
                .contains("invalid JSON"),
            "tool_result should name the parse failure: {}",
            result["content"]
        );

        let received = server.received_requests().await.expect("received");
        assert!(received.len() >= 2, "round-2 request happened");
        let body: serde_json::Value =
            serde_json::from_slice(&received[1].body).expect("request json");
        let carried = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .rev()
            .find(|m| {
                m["content"].as_array().is_some_and(|blocks| {
                    blocks.iter().any(|b| {
                        b["type"] == "tool_result"
                            && b["tool_use_id"] == "toolu_1"
                            && b["is_error"] == true
                    })
                })
            })
            .expect("error tool_result reached provider in round 2");
        assert!(carried["content"].as_array().unwrap()[0]["is_error"] == true);
    }

    /// F6: independent tool calls from one assistant turn execute
    /// concurrently. Observable proxy: two blocking ask_user calls (300 ms
    /// each) complete in well under their serial sum. The RwLock read guard
    /// is what makes the overlap possible; a Mutex here would serialize the
    /// batch and this test would fail at ~600 ms.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn f6_independent_tools_run_concurrently() {
        let server = MockServer::start().await;
        // Two tool_use blocks in ONE round, then a plain completion round.
        let tool_sse = {
            let d1 = serde_json::json!({
                "type": "content_block_delta", "index": 0,
                "delta": {"type": "input_json_delta", "partial_json": r#"{"question":"a?"}"#}
            });
            let d2 = serde_json::json!({
                "type": "content_block_delta", "index": 1,
                "delta": {"type": "input_json_delta", "partial_json": r#"{"question":"b?"}"#}
            });
            format!(
                concat!(
                    "data: {{\"type\":\"message_start\",\"message\":{{}}}}\n\n",
                    "data: {{\"type\":\"content_block_start\",\"index\":0,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"toolu_a\",\"name\":\"ask_user\",\"input\":{{}}}}}}\n\n",
                    "data: {d1}\n\n",
                    "data: {{\"type\":\"content_block_stop\",\"index\":0}}\n\n",
                    "data: {{\"type\":\"content_block_start\",\"index\":1,\"content_block\":{{\"type\":\"tool_use\",\"id\":\"toolu_b\",\"name\":\"ask_user\",\"input\":{{}}}}}}\n\n",
                    "data: {d2}\n\n",
                    "data: {{\"type\":\"content_block_stop\",\"index\":1}}\n\n",
                    "data: {{\"type\":\"message_delta\",\"delta\":{{\"stop_reason\":\"tool_use\"}}}}\n\n",
                    "data: {{\"type\":\"message_stop\"}}\n\n"
                ),
                d1 = d1,
                d2 = d2,
            )
        };
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(tool_sse))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(200).set_body_string(anthropic_plain_sse("both answered")),
            )
            .mount(&server)
            .await;

        let (orch, invocations) = blocking_orchestrator(server.uri(), 300);
        orch.set_provider_config(anthropic_config());

        let token = orch.register_request("f6-concurrent").expect("register");
        let orch_run = Arc::clone(&orch);
        let started = std::time::Instant::now();
        let reply = tokio::spawn(async move {
            orch_run
                .stream_message(
                    "f6-concurrent".to_string(),
                    "sess".to_string(),
                    "go".to_string(),
                    None,
                    token,
                )
                .await
        })
        .await
        .expect("join")
        .expect("turn completes");
        let elapsed = started.elapsed();

        assert_eq!(reply, "both answered");
        assert_eq!(invocations.load(std::sync::atomic::Ordering::SeqCst), 2);
        assert!(
            elapsed < std::time::Duration::from_millis(550),
            "two 300 ms tools must overlap under buffered(4); took {elapsed:?} (serial would be ~600 ms)"
        );

        // Pairing contract survives concurrent reassembly.
        let history = serde_json::to_value(&*orch.anthropic_messages.lock()).unwrap();
        assert_anthropic_pairing(history.as_array().unwrap(), "concurrent history");
    }

    /// perf#1/F9 at stream level: Delta fragments emitted during a turn are
    /// coalesced by the shared coalescer, and the terminal Completed event
    /// carries the full text with NO pending batch left behind. A broken
    /// flush-on-emit would truncate the visible reply or reorder events.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn streamed_deltas_coalesce_and_completed_carries_full_text() {
        let server = MockServer::start().await;
        // Multi-fragment text: several small text_deltas in one round.
        let sse = {
            let mut body = String::from("data: {\"type\":\"message_start\",\"message\":{}}\n\n");
            body.push_str("data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\n");
            for part in ["hel", "lo w", "orl", "d!"] {
                let delta = serde_json::json!({
                    "type": "content_block_delta",
                    "index": 0,
                    "delta": {"type": "text_delta", "text": part}
                });
                body.push_str(&format!("data: {delta}\n\n"));
            }
            body.push_str("data: {\"type\":\"content_block_stop\",\"index\":0}\n\n");
            body.push_str("data: {\"type\":\"message_stop\"}\n\n");
            body
        };
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(ResponseTemplate::new(200).set_body_string(sse))
            .mount(&server)
            .await;

        let (orch, _invocations) = blocking_orchestrator(server.uri(), 0);
        orch.set_provider_config(anthropic_config());

        let events = Arc::new(parking_lot::Mutex::<Vec<AthenaStreamEvent>>::default());
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().push(event);
        })));

        let token = orch.register_request("coalesce-order").expect("register");
        let orch_run = Arc::clone(&orch);
        let reply = tokio::spawn(async move {
            orch_run
                .stream_message(
                    "coalesce-order".to_string(),
                    "sess".to_string(),
                    "hi".to_string(),
                    None,
                    token,
                )
                .await
        })
        .await
        .expect("join")
        .expect("turn completes");
        assert_eq!(reply, "hello world!");

        let guard = events.lock();
        let mut deltas = Vec::<String>::new();
        let mut completed_text: Option<String> = None;
        let mut saw_terminal_before_delta = false;
        for event in guard.iter() {
            match event {
                AthenaStreamEvent::Delta { text, .. } => {
                    assert!(!saw_terminal_before_delta, "Delta after terminal event");
                    deltas.push(text.clone());
                }
                AthenaStreamEvent::Completed { text, .. } => {
                    completed_text = Some(text.clone());
                    saw_terminal_before_delta = true;
                }
                _ => {}
            }
        }
        // Coalescing happened: fewer Delta events than fragments, but the
        // concatenation is intact and Completed carries the same text.
        let joined: String = deltas.concat();
        assert_eq!(joined, "hello world!");
        assert!(
            deltas.len() < 4,
            "fragments must coalesce; got {} Deltas for 4 fragments",
            deltas.len()
        );
        assert_eq!(completed_text.as_deref(), Some("hello world!"));
    }

    /// F7: two concurrent stream_message calls for DIFFERENT sessions must
    /// serialize on the conversation lock — the second turn must not start
    /// until the first has fully finished (no interleaved history, no
    /// clobbered current_session_id).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_turns_for_different_sessions_serialize() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/messages"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_string(anthropic_plain_sse("done"))
                    .set_delay(std::time::Duration::from_millis(150)),
            )
            .mount(&server)
            .await;

        // Production wires a SessionStore: load_conversation replaces the
        // in-memory buffers per turn. Seed both sessions so each turn starts
        // from its OWN (empty) history — without the store, both turns share
        // one buffer and B would observe A's history.
        let store = Arc::new(athena_store::SessionStore::new_empty());
        store.create_session(Some("a")).await.expect("seed a");
        store.create_session(Some("b")).await.expect("seed b");
        let sessions = store.list_sessions().await.expect("list");
        let id_a = sessions
            .iter()
            .find(|s| s.title == "a")
            .expect("sess a")
            .id
            .clone();
        let id_b = sessions
            .iter()
            .find(|s| s.title == "b")
            .expect("sess b")
            .id
            .clone();

        let (orch, _invocations) =
            blocking_orchestrator_with_store(server.uri(), 0, Arc::clone(&store));
        orch.set_provider_config(anthropic_config());

        let token_a = orch.register_request("f7-a").expect("register");
        let token_b = orch.register_request("f7-b").expect("register");
        let orch_a = Arc::clone(&orch);
        let orch_b = Arc::clone(&orch);
        let turn_a = tokio::spawn(async move {
            orch_a
                .stream_message(
                    "f7-a".to_string(),
                    id_a.clone(),
                    "hi a".to_string(),
                    None,
                    token_a,
                )
                .await
        });
        let turn_b = tokio::spawn(async move {
            orch_b
                .stream_message(
                    "f7-b".to_string(),
                    id_b.clone(),
                    "hi b".to_string(),
                    None,
                    token_b,
                )
                .await
        });
        let (ra, rb) = tokio::join!(turn_a, turn_b);
        ra.expect("join a").expect("turn a ok");
        rb.expect("join b").expect("turn b ok");

        // Both turns completed against the SAME provider; each request body
        // must carry exactly ONE user message (its own) — if the lock were
        // broken, turn B could observe turn A's history or session state.
        let received = server.received_requests().await.expect("received");
        assert_eq!(received.len(), 2, "one request per turn");
        for (index, request) in received.iter().enumerate() {
            let body: serde_json::Value =
                serde_json::from_slice(&request.body).expect("request json");
            let messages = body["messages"].as_array().expect("messages array");
            assert_eq!(
                messages.len(),
                1,
                "request {index} must carry only its own turn's user message, got {messages:?}"
            );
            assert_eq!(messages[0]["role"], "user");
        }
        // Serialized, not overlapped: total wall time covers both 150 ms
        // delays. (A weak check on its own, but combined with the
        // single-message history assertion above it pins the contract.)
    }
}
