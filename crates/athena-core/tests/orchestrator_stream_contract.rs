//! Integration tests: orchestrator streaming contract (`stream_message`).
//!
//! Contract under defense: streamed assistant turns surface as ordered
//! `AthenaStreamEvent`s (Started → Delta* → Status* → Completed | Error),
//! provider tool calls pair 1:1 with tool-role replies in the follow-up
//! request history, and failures emit an `Error` event whose `cancelled`
//! flag matches the failure mode.
//!
//! These tests exercise the public API only. The Anthropic transport pins its
//! base URL internally, so the OpenAI-compatible path (configurable through
//! `ProviderConfig.base_url`) stands in as the mock-provider seam; both paths
//! share the same event-emission, SSE-parsing, and tool-loop code paths.

use athena_core::{AthenaOrchestrator, AthenaStreamEvent, LLMProvider, ProviderConfig};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const CHAT_PATH: &str = "/chat/completions";

type EventSink = Arc<std::sync::Mutex<Vec<AthenaStreamEvent>>>;

fn openai_config(server_uri: String) -> ProviderConfig {
    ProviderConfig::new(
        LLMProvider::OpenAI,
        "test-key",
        "gpt-test".to_string(),
        String::new(),
        Some(server_uri),
    )
}

fn sse(chunks: &[serde_json::Value]) -> String {
    let mut body = String::new();
    for chunk in chunks {
        body.push_str(&format!("data: {}\n\n", chunk));
    }
    body.push_str("data: [DONE]\n\n");
    body
}

fn content_delta(text: &str) -> serde_json::Value {
    serde_json::json!({"choices": [{"delta": {"content": text}}]})
}

fn tool_call_delta(
    index: u64,
    id: Option<&str>,
    name: Option<&str>,
    args: Option<&str>,
) -> serde_json::Value {
    let mut function = serde_json::Map::new();
    if let Some(name) = name {
        function.insert("name".into(), serde_json::Value::String(name.into()));
    }
    if let Some(args) = args {
        function.insert("arguments".into(), serde_json::Value::String(args.into()));
    }
    let mut call = serde_json::Map::new();
    call.insert("index".into(), serde_json::Value::from(index));
    if let Some(id) = id {
        call.insert("id".into(), serde_json::Value::String(id.into()));
    }
    call.insert("function".into(), serde_json::Value::Object(function));
    serde_json::json!({"choices": [{"delta": {"tool_calls": [serde_json::Value::Object(call)]}}]})
}

async fn run_stream(
    orch: &AthenaOrchestrator,
    request_id: &str,
    session_id: &str,
    cancel: CancellationToken,
) -> Result<String, athena_core::OrchestratorError> {
    tokio::time::timeout(
        Duration::from_secs(30),
        orch.stream_message(
            request_id.to_string(),
            session_id.to_string(),
            "hello".to_string(),
            None,
            cancel,
        ),
    )
    .await
    .expect("stream_message timed out")
}

#[tokio::test]
async fn streamed_deltas_arrive_in_order_and_end_in_completed() {
    // Observable contract: one user turn produces exactly one Started, then
    // ordered Delta fragments whose concatenation equals the final reply, and
    // a terminal Completed carrying that same reply.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(&[
            content_delta("Hel"),
            content_delta("lo wo"),
            content_delta("rld"),
        ])))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(openai_config(server.uri()));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let reply = run_stream(&orch, "req-order", "sess-1", CancellationToken::new())
        .await
        .expect("stream should complete");
    assert_eq!(reply, "Hello world");

    let events = events.lock().unwrap().clone();
    assert!(!events.is_empty());
    assert!(
        matches!(events.first(), Some(AthenaStreamEvent::Started { request_id, session_id })
            if request_id == "req-order" && session_id == "sess-1"),
        "first event must be Started for this request, got {:?}",
        events.first()
    );
    let deltas: String = events
        .iter()
        .filter_map(|event| match event {
            AthenaStreamEvent::Delta { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(
        deltas, "Hello world",
        "deltas must concatenate to the reply in order"
    );
    assert!(
        matches!(events.last(), Some(AthenaStreamEvent::Completed { request_id, text })
            if request_id == "req-order" && text == "Hello world"),
        "last event must be Completed with the full reply, got {:?}",
        events.last()
    );
}

#[tokio::test]
async fn http_error_yields_error_event_and_returns_err() {
    // Observable contract: a provider-level failure surfaces as an Error
    // event (cancelled=false) AND an Err return carrying the API status.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(500).set_body_string("boom"))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(openai_config(server.uri()));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let result = run_stream(&orch, "req-500", "sess-1", CancellationToken::new()).await;
    let message = match &result {
        Ok(reply) => panic!("expected failure, got Ok({reply:?})"),
        Err(error) => error.to_string(),
    };
    assert!(
        message.contains("500"),
        "error should carry the API status: {message}"
    );

    let events = events.lock().unwrap().clone();
    match events.last() {
        Some(AthenaStreamEvent::Error {
            request_id,
            message,
            cancelled,
            model_unavailable: _,
        }) => {
            assert_eq!(request_id, "req-500");
            assert!(message.contains("500"));
            assert!(!cancelled, "API failures are not user cancellations");
        }
        other => panic!("last event must be Error, got {other:?}"),
    }
}

/// Observable contract: a provider reporting the configured model as retired
/// (HTTP 410 Gone — seen with z-ai/glm model EOL) must surface a dedicated
/// `model_unavailable` error so the desktop can route the user to Settings
/// instead of showing a bare toast.
#[tokio::test]
async fn model_gone_yields_model_unavailable_error_event() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(410).set_body_string(
            r#"{"error":{"message":"model glm-5.2 has been retired"}}"#,
        ))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(openai_config(server.uri()));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let result = run_stream(&orch, "req-410", "sess-1", CancellationToken::new()).await;
    let message = match &result {
        Ok(reply) => panic!("expected failure, got Ok({reply:?})"),
        Err(error) => error.to_string(),
    };
    assert!(
        message.contains("unavailable"),
        "410 should read as model-unavailable guidance: {message}"
    );

    let events = events.lock().unwrap().clone();
    match events.last() {
        Some(AthenaStreamEvent::Error {
            model_unavailable, ..
        }) => {
            assert!(model_unavailable, "410 must set the model_unavailable flag");
        }
        other => panic!("last event must be Error, got {other:?}"),
    }
}

#[tokio::test]
async fn malformed_sse_json_fails_after_emitting_prior_deltas_in_order() {
    // Observable contract: a malformed data line aborts the turn with an
    // Error event, but deltas already emitted stay ordered before it.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(format!(
            "data: {}\n\ndata: {{broken json}}\n\n",
            content_delta("partial")
        )))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(openai_config(server.uri()));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let result = run_stream(&orch, "req-bad-sse", "sess-1", CancellationToken::new()).await;
    assert!(result.is_err(), "malformed SSE must fail the turn");

    let events = events.lock().unwrap().clone();
    let saw_partial_delta = events.iter().any(|event| {
        matches!(
            event,
            AthenaStreamEvent::Delta { text, .. } if text == "partial"
        )
    });
    assert!(
        saw_partial_delta,
        "delta emitted before the bad line must survive"
    );
    assert!(
        matches!(
            events.last(),
            Some(AthenaStreamEvent::Error {
                cancelled: false,
                ..
            })
        ),
        "turn must terminate with a non-cancelled Error, got {:?}",
        events.last()
    );
}

#[tokio::test]
async fn tool_loop_preserves_use_result_pairing_in_follow_up_history() {
    // Observable contract: when the model streams a tool call, the NEXT
    // request history carries (a) the raw-string arguments on the assistant
    // `tool_calls` entry and (b) a role="tool" reply whose `tool_call_id`
    // matches the call id — even though this orchestrator has no executor
    // configured (the failed execution must still produce the paired reply).
    let server = MockServer::start().await;
    // wiremock 0.6 sorts equal-priority mocks by insertion order (first
    // mounted wins). Mount the one-shot tool-call mock first so round 1
    // gets it; once exhausted it stops matching and the catch-all
    // plain-text fallback serves the completion round.
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(&[
            tool_call_delta(0, Some("call_pair_1"), Some("list_agen"), None),
            tool_call_delta(0, None, Some("ts"), None),
            tool_call_delta(0, None, None, Some("{\"quer\"")),
            tool_call_delta(0, None, None, Some(": \"x\"}")),
        ])))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(&[content_delta("all done")])))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(openai_config(server.uri()));

    let reply = run_stream(&orch, "req-tool", "sess-1", CancellationToken::new())
        .await
        .expect("two-round stream should complete");
    assert_eq!(reply, "all done");

    let requests = server.received_requests().await.expect("recorded requests");
    assert_eq!(
        requests.len(),
        2,
        "one tool round plus one completion round"
    );
    let second: serde_json::Value =
        serde_json::from_slice(&requests[1].body).expect("request body is JSON");
    let messages = second["messages"].as_array().expect("messages array");

    let assistant_tool_calls = messages
        .iter()
        .find(|message| message["role"] == "assistant" && message["tool_calls"].is_array())
        .expect("history must retain the assistant tool_calls message");
    let calls = assistant_tool_calls["tool_calls"].as_array().unwrap();
    assert_eq!(calls.len(), 1, "exactly one streamed tool call expected");
    // Arguments were streamed as string fragments; they must be reassembled
    // verbatim into the raw STRING form OpenAI's wire contract requires.
    assert_eq!(calls[0]["id"], "call_pair_1");
    assert_eq!(calls[0]["function"]["name"], "list_agents");
    assert_eq!(calls[0]["function"]["arguments"], "{\"quer\": \"x\"}");

    let tool_reply = messages
        .iter()
        .find(|message| message["role"] == "tool")
        .expect("every tool_call needs a paired tool reply");
    assert_eq!(
        tool_reply["tool_call_id"], "call_pair_1",
        "tool reply must reference the originating call id"
    );
}

#[tokio::test]
async fn pre_cancelled_request_never_hits_the_wire() {
    // Observable contract: cancelling before the turn starts yields
    // UserCancellation, emits Error{cancelled:true}, and performs no HTTP.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(200).set_body_string(sse(&[content_delta("hi")])))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(openai_config(server.uri()));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let token = CancellationToken::new();
    token.cancel();
    let result = run_stream(&orch, "req-cancel", "sess-1", token).await;
    assert!(
        matches!(
            &result,
            Err(athena_core::OrchestratorError::UserCancellation)
        ),
        "expected UserCancellation, got {result:?}"
    );

    let events = events.lock().unwrap().clone();
    match events.last() {
        Some(AthenaStreamEvent::Error {
            cancelled: true, ..
        }) => {}
        other => panic!("expected terminal Error{{cancelled:true}}, got {other:?}"),
    }
    let requests = server.received_requests().await.unwrap();
    assert!(
        requests.is_empty(),
        "a pre-cancelled turn must not reach the network"
    );
}

#[tokio::test]
async fn lmstudio_vision_rejection_happens_before_any_http() {
    // Observable contract: LM Studio + image attachments fails fast with the
    // dedicated error, emits Error, and issues no request.
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path(CHAT_PATH))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;

    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(ProviderConfig::new(
        LLMProvider::Lmstudio,
        "key",
        "local-model".to_string(),
        String::new(),
        Some(server.uri()),
    ));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let images = Some(vec![athena_core::ImageData {
        base64: "aGk=".to_string(),
        media_type: "image/png".to_string(),
    }]);
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        orch.stream_message(
            "req-vision".to_string(),
            "sess-1".to_string(),
            "describe".to_string(),
            images,
            CancellationToken::new(),
        ),
    )
    .await
    .expect("vision rejection must not block");
    assert!(
        matches!(
            &result,
            Err(athena_core::OrchestratorError::LmStudioVisionNotSupported)
        ),
        "got {result:?}"
    );
    assert!(matches!(
        events.lock().unwrap().last(),
        Some(AthenaStreamEvent::Error {
            cancelled: false,
            ..
        })
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn invalid_base_url_is_rejected_before_any_http() {
    // Observable contract: a base URL failing validation aborts the turn
    // with an Error event and zero network activity.
    let server = MockServer::start().await;
    let orch = AthenaOrchestrator::new();
    orch.set_provider_config(ProviderConfig::new(
        LLMProvider::OpenAI,
        "key",
        "model".to_string(),
        String::new(),
        Some("ftp://not-a-valid-endpoint.invalid".to_string()),
    ));
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    let result = run_stream(&orch, "req-badurl", "sess-1", CancellationToken::new()).await;
    assert!(result.is_err(), "invalid scheme must be rejected");
    assert!(matches!(
        events.lock().unwrap().last(),
        Some(AthenaStreamEvent::Error {
            cancelled: false,
            ..
        })
    ));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn missing_api_key_yields_missing_key_error_event() {
    // Observable contract: with no provider config and no ANTHROPIC_API_KEY,
    // the turn fails with MissingApiKey surfaced through an Error event.
    let orch = AthenaOrchestrator::new();
    let events: EventSink = Arc::default();
    {
        let sink = Arc::clone(&events);
        orch.set_stream_emitter(Some(Arc::new(move |event| {
            sink.lock().unwrap().push(event);
        })));
    }

    // SAFETY: single-threaded mutation scoped to this test; every other test
    // installs explicit provider config, so the variable is irrelevant there.
    let previous = std::env::var("ANTHROPIC_API_KEY").ok();
    std::env::remove_var("ANTHROPIC_API_KEY");
    let result = run_stream(&orch, "req-nokey", "sess-1", CancellationToken::new()).await;
    if let Some(value) = previous {
        std::env::set_var("ANTHROPIC_API_KEY", value);
    }

    assert!(
        matches!(&result, Err(athena_core::OrchestratorError::MissingApiKey)),
        "got {result:?}"
    );
    assert!(matches!(
        events.lock().unwrap().last(),
        Some(AthenaStreamEvent::Error {
            cancelled: false,
            ..
        })
    ));
}
