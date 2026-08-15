use dioxus::prelude::*;

#[path = "athena_model.rs"]
mod athena_model;

pub use athena_model::{
    AskUserBlock, AskUserOption, AthenaMessage, ContentBlock, DraggableItem, EvaluationBlock,
    ImageAttachment, ImageMediaType, MessageRole, PlanBlock, PlanStatus, PlanStepBlock,
    PlanStepStatus, StepEvaluation,
};
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Maximum number of messages kept in memory.
const MAX_MESSAGES: usize = 100;

const DEFAULT_MODEL: &str = "claude";
const DEFAULT_PROVIDER: &str = "anthropic";
const DEFAULT_BYPASS_MODE: bool = true;
const DEFAULT_AUTO_LAUNCH: bool = true;

/// Global Athena chat state.
#[derive(Clone, PartialEq, Default)]
pub struct AthenaState {
    /// Bounded message log. VecDeque gives O(1) front-pop for ring-buffer
    /// eviction in add_message (vs Vec O(n) shift on drain(0..n)).
    pub messages: VecDeque<AthenaMessage>,
    pub is_open: bool,
    pub is_loading: bool,
    pub is_streaming: bool,
    pub streaming_status: Option<String>,
    /// Rolling trace of streaming status messages, newest last. Appended on
    /// each distinct `status` event and reset per request; rendered as the
    /// expandable agent trace in the thinking indicator.
    pub streaming_trace: Vec<String>,
    /// Request ID currently allowed to mutate this conversation. Late events
    /// from cancelled or superseded turns are ignored.
    pub active_request_id: Option<String>,
    pub error: Option<String>,
    pub model: String,
    pub provider: String,
    pub bypass_mode: bool,
    pub auto_launch: bool,
    pub session_id: Option<String>,
    pub session_title: String,
    /// Items dragged/dropped or explicitly pinned to Athena's context for the current conversation.
    pub dropped_context: Vec<DraggableItem>,
    /// Whether an API key is configured, as reported by the backend
    /// (keyring-backed `llm.api_key` probe). `None` = not yet checked
    /// (panel still mounting); `Some(true)` = key present; `Some(false)` =
    /// no key, so the panel shows a "configure in Settings" banner and the
    /// input is disabled. This is the *only* signal the UI trusts for
    /// "can I send?" — the in-memory `model`/`provider` defaults above are
    /// deliberately NOT used for that decision.
    pub api_configured: Option<bool>,
    /// The model string actually persisted in the store (`llm.model`),
    /// loaded on panel mount and refreshed when Settings saves. Distinct
    /// from the in-memory `model` field, which historically carried a stale
    /// default ("claude") that was never synced to the backend.
    pub configured_model: Option<String>,
    /// If the keyring probe failed (e.g. keychain locked), this holds the
    /// error message so the UI can show a warning instead of silently
    /// looking like everything is fine. `None` = no error or not checked.
    pub api_keyring_error: Option<String>,
}

impl AthenaState {
    pub fn new() -> Self {
        Self {
            messages: VecDeque::new(),
            is_open: false,
            is_loading: false,
            is_streaming: false,
            streaming_status: None,
            streaming_trace: Vec::new(),
            active_request_id: None,
            error: None,
            model: DEFAULT_MODEL.to_string(),
            provider: DEFAULT_PROVIDER.to_string(),
            bypass_mode: DEFAULT_BYPASS_MODE,
            auto_launch: DEFAULT_AUTO_LAUNCH,
            session_id: None,
            session_title: String::new(),
            dropped_context: Vec::new(),
            api_configured: None,
            configured_model: None,
            api_keyring_error: None,
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn add_message(&mut self, msg: AthenaMessage) {
        // O(1) ring-buffer append + O(1) front eviction via VecDeque.
        self.messages.push_back(msg);
        while self.messages.len() > MAX_MESSAGES {
            self.messages.pop_front();
        }
    }

    pub fn set_open(&mut self, open: bool) {
        self.is_open = open;
    }

    pub fn toggle_open(&mut self) {
        self.is_open = !self.is_open;
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }

    pub fn set_streaming(&mut self, streaming: bool) {
        self.is_streaming = streaming;
    }

    pub fn set_streaming_status(&mut self, status: Option<String>) {
        self.streaming_status = status.clone();
        // Keep a distinct-status history for the thinking trace so the UI can
        // show where the agent has been, not just where it is now.
        if let Some(s) = status {
            if self.streaming_trace.last().map(String::as_str) != Some(s.as_str()) {
                self.streaming_trace.push(s);
            }
        }
    }

    pub fn begin_stream(&mut self, request_id: String) {
        self.active_request_id = Some(request_id);
        self.is_loading = true;
        self.is_streaming = true;
        self.streaming_status = Some("Connecting…".to_string());
        self.streaming_trace = vec!["Connecting…".to_string()];
        self.error = None;
    }

    pub fn accepts_stream_event(&self, request_id: &str) -> bool {
        self.active_request_id.as_deref() == Some(request_id)
    }

    pub fn append_stream_delta(&mut self, request_id: &str, delta: &str) {
        if !self.accepts_stream_event(request_id) {
            return;
        }
        if let Some(message) = self.messages.back_mut() {
            if message.role == MessageRole::Athena && !message.is_error {
                message.content.push_str(delta);
            }
        }
    }

    pub fn finish_stream(&mut self, request_id: &str, final_text: Option<&str>) {
        if !self.accepts_stream_event(request_id) {
            return;
        }
        if let Some(final_text) = final_text {
            if let Some(message) = self.messages.back_mut() {
                if message.role == MessageRole::Athena && !message.is_error {
                    message.content = final_text.to_string();
                }
            }
        }
        self.active_request_id = None;
        self.is_loading = false;
        self.is_streaming = false;
        self.streaming_status = None;
        self.streaming_trace.clear();
    }

    /// Invalidate an active request before loading another session. A late
    /// provider event must never mutate the newly selected conversation.
    pub fn invalidate_active_request(&mut self) -> Option<String> {
        let request_id = self.active_request_id.take();
        self.is_loading = false;
        self.is_streaming = false;
        self.streaming_status = None;
        self.streaming_trace.clear();
        request_id
    }

    pub fn fail_stream(&mut self, request_id: &str, message: String, cancelled: bool) {
        if !self.accepts_stream_event(request_id) {
            return;
        }
        self.active_request_id = None;
        self.is_loading = false;
        self.is_streaming = false;
        self.streaming_status = None;
        self.streaming_trace.clear();
        if !cancelled {
            self.error = Some(message.clone());
            if let Some(last) = self.messages.back_mut() {
                if last.role == MessageRole::Athena && !last.is_error {
                    if last.content.is_empty() {
                        last.content = format!("Error: {message}");
                    }
                    last.is_error = true;
                }
            }
        }
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }

    /// Remove the failed turn before replaying it so Retry does not duplicate
    /// the user message or leave an obsolete error bubble in the transcript.
    pub fn prepare_retry(&mut self, text: &str) -> bool {
        if self.error.is_none() {
            return false;
        }
        let failed_assistant = self
            .messages
            .back()
            .is_some_and(|message| message.role == MessageRole::Athena && message.is_error);
        if !failed_assistant {
            return false;
        }
        self.messages.pop_back();
        let matching_user = self
            .messages
            .back()
            .is_some_and(|message| message.role == MessageRole::User && message.content == text);
        if matching_user {
            self.messages.pop_back();
        }
        self.error = None;
        true
    }

    pub fn set_model(&mut self, model: impl Into<String>) {
        self.model = model.into();
    }

    pub fn set_provider(&mut self, provider: impl Into<String>) {
        self.provider = provider.into();
    }

    pub fn set_bypass_mode(&mut self, bypass: bool) {
        self.bypass_mode = bypass;
    }

    pub fn set_auto_launch(&mut self, auto: bool) {
        self.auto_launch = auto;
    }

    pub fn clear_messages(&mut self) {
        self.invalidate_active_request();
        self.messages.clear();
        self.error = None;
    }

    pub fn set_messages(&mut self, messages: Vec<AthenaMessage>) {
        self.invalidate_active_request();
        // Convert Vec into VecDeque via FromIterator.
        self.messages = messages.into_iter().collect();
        self.error = None;
    }

    pub fn set_session_id(&mut self, id: Option<String>) {
        self.session_id = id;
    }

    pub fn set_session_title(&mut self, title: impl Into<String>) {
        self.session_title = title.into();
    }

    /// Pin an agent to Athena's prompt context. Repeated drops of the same
    /// pane are intentionally idempotent so a drag jitter or accidental
    /// re-drop cannot duplicate the reference shown to the user or sent to
    /// the model.
    pub fn add_agent_context(
        &mut self,
        pane_id: impl Into<String>,
        agent_type: impl Into<String>,
        label: impl Into<String>,
    ) -> bool {
        let item = DraggableItem::Agent {
            pane_id: pane_id.into(),
            agent_type: agent_type.into(),
            label: label.into(),
        };
        let pane_id = match &item {
            DraggableItem::Agent { pane_id, .. } => pane_id,
            _ => return false,
        };
        if self.dropped_context.iter().any(|existing| {
            matches!(existing, DraggableItem::Agent { pane_id: existing_id, .. } if existing_id == pane_id)
        }) {
            return false;
        }
        self.dropped_context.push(item);
        true
    }

    /// Record the result of the backend `llm.api_key` probe. Called on
    /// panel mount and whenever Settings saves a new key.
    pub fn set_api_configured(&mut self, configured: Option<bool>) {
        self.api_configured = configured;
    }

    /// Record the model actually persisted in the store (`llm.model`).
    pub fn set_configured_model(&mut self, model: Option<String>) {
        self.configured_model = model;
    }

    /// Record a keyring probe error so the UI can warn the user.
    pub fn set_api_keyring_error(&mut self, error: Option<String>) {
        self.api_keyring_error = error;
    }

    /// Convert messages to a JSON string suitable for the backend session store.
    pub fn messages_as_json(&self) -> String {
        let msgs: Vec<serde_json::Value> = self
            .messages
            .iter()
            .map(|m| {
                serde_json::json!({
                    "id": &m.id,
                    "role": match m.role {
                        MessageRole::User => "user",
                        MessageRole::Athena => "athena",
                    },
                    "content": &m.content,
                    "timestamp": m.timestamp,
                    "isError": m.is_error,
                })
            })
            .collect();
        serde_json::to_string(&msgs).unwrap_or_default()
    }

    /// Load messages from a session JSON string (produced by the backend).
    pub fn load_messages_from_json(&mut self, json: &str) {
        let parsed: Vec<serde_json::Value> = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(_) => return,
        };
        let mut loaded: VecDeque<AthenaMessage> = VecDeque::new();
        for val in parsed {
            let role = val
                .get("role")
                .and_then(|v| v.as_str())
                .map(|s| match s {
                    "user" => MessageRole::User,
                    _ => MessageRole::Athena,
                })
                .unwrap_or(MessageRole::User);
            let content = val
                .get("content")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let id = val
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let timestamp = val
                .get("timestamp")
                .and_then(|v| v.as_i64())
                .unwrap_or_else(|| chrono::Utc::now().timestamp());
            let is_error = val
                .get("isError")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            loaded.push_back(AthenaMessage {
                id,
                role,
                content,
                timestamp,
                is_error,
                images: Vec::new(),
                blocks: Vec::new(),
            });
        }
        self.messages = loaded;
        self.error = None;
    }

    /// Handle athena:askUser event — add an AskUser block to the latest assistant message
    /// or create a new message with the AskUser block.
    pub fn handle_ask_user(
        &mut self,
        request_id: String,
        question: String,
        options: Vec<AskUserOption>,
    ) {
        let ask_block = ContentBlock::AskUser(AskUserBlock {
            request_id,
            question,
            options,
            answered: false,
            selected_answer: None,
        });

        // Try to append to the last assistant message
        let last_is_athena = self
            .messages
            .back()
            .is_some_and(|m| m.role == MessageRole::Athena);
        if last_is_athena {
            if let Some(msg) = self.messages.back_mut() {
                msg.blocks.push(ask_block);
                return;
            }
        }

        // Otherwise create a new assistant message with the AskUser block
        let msg = AthenaMessage {
            id: format!("ask-{}", chrono::Utc::now().timestamp_millis()),
            role: MessageRole::Athena,
            content: String::new(),
            timestamp: chrono::Utc::now().timestamp(),
            is_error: false,
            images: Vec::new(),
            blocks: vec![ask_block],
        };
        self.add_message(msg);
    }

    /// Mark an AskUser block as answered with the selected response.
    pub fn mark_ask_user_answered(&mut self, request_id: &str, answer: &str) {
        for msg in self.messages.iter_mut() {
            for block in msg.blocks.iter_mut() {
                if let ContentBlock::AskUser(ask) = block {
                    if ask.request_id == request_id {
                        ask.answered = true;
                        ask.selected_answer = Some(answer.to_string());
                    }
                }
            }
        }
    }

    /// Handle athena:planUpdate event — update or create a plan block.
    pub fn handle_plan_update(
        &mut self,
        plan_id: String,
        goal: String,
        steps: Vec<PlanStepBlock>,
        status: PlanStatus,
    ) {
        let plan_block = ContentBlock::Plan(PlanBlock {
            plan_id,
            goal,
            steps,
            status,
        });

        // Try to update existing plan in last assistant message
        let last_is_athena = self
            .messages
            .back()
            .is_some_and(|m| m.role == MessageRole::Athena);
        if last_is_athena {
            if let Some(msg) = self.messages.back_mut() {
                // Replace existing plan block or add new one
                let mut found = false;
                let target_plan_id = match &plan_block {
                    ContentBlock::Plan(p) => p.plan_id.clone(),
                    _ => String::new(),
                };
                for block in &mut msg.blocks {
                    if let Some(existing_id) = block.plan_id() {
                        if existing_id == target_plan_id {
                            *block = plan_block.clone();
                            found = true;
                            break;
                        }
                    }
                }
                if !found {
                    msg.blocks.push(plan_block);
                }
                return;
            }
        }

        // Create new message with the plan
        let msg = AthenaMessage {
            id: format!("plan-{}", chrono::Utc::now().timestamp_millis()),
            role: MessageRole::Athena,
            content: String::new(),
            timestamp: chrono::Utc::now().timestamp(),
            is_error: false,
            images: Vec::new(),
            blocks: vec![plan_block],
        };
        self.add_message(msg);
    }

    /// Handle athena:planEvaluated event — add evaluation block.
    pub fn handle_plan_evaluated(
        &mut self,
        plan_id: String,
        overall_status: String,
        step_evaluations: Vec<StepEvaluation>,
        next_action: String,
        reasoning: String,
    ) {
        let eval_block = ContentBlock::Evaluation(EvaluationBlock {
            plan_id,
            overall_status,
            step_evaluations,
            next_action,
            reasoning,
        });

        // Try to append to last assistant message
        let last_is_athena = self
            .messages
            .back()
            .is_some_and(|m| m.role == MessageRole::Athena);
        if last_is_athena {
            if let Some(msg) = self.messages.back_mut() {
                msg.blocks.push(eval_block);
                return;
            }
        }

        // Create new message
        let msg = AthenaMessage {
            id: format!("eval-{}", chrono::Utc::now().timestamp_millis()),
            role: MessageRole::Athena,
            content: String::new(),
            timestamp: chrono::Utc::now().timestamp(),
            is_error: false,
            images: Vec::new(),
            blocks: vec![eval_block],
        };
        self.add_message(msg);
    }
}

// ---------------------------------------------------------------------------
// Context helpers
// ---------------------------------------------------------------------------

/// Obtain the Athena signal from the Dioxus context.
pub fn use_athena_store() -> Signal<AthenaState> {
    use_context::<Signal<AthenaState>>()
}

/// Initialize the Athena store as a context provider.
pub fn provide_athena_store() {
    use_context_provider(|| Signal::new(AthenaState::new()));
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_msg(id: &str) -> AthenaMessage {
        AthenaMessage {
            id: id.to_string(),
            role: MessageRole::User,
            content: format!("msg-{id}"),
            timestamp: 0,
            is_error: false,
            images: Vec::new(),
            blocks: Vec::new(),
        }
    }

    #[test]
    fn add_message_evicts_oldest() {
        let mut s = AthenaState::default();
        for i in 0..(MAX_MESSAGES + 5) {
            s.add_message(make_msg(&i.to_string()));
        }
        assert_eq!(s.messages.len(), MAX_MESSAGES);
        // First five (0..5) should be evicted - 5 is the new front.
        assert_eq!(s.messages.front().unwrap().id, "5");
        assert_eq!(
            s.messages.back().unwrap().id,
            (MAX_MESSAGES + 4).to_string()
        );
    }

    #[test]
    fn add_message_under_cap_keeps_all() {
        let mut s = AthenaState::default();
        for i in 0..10 {
            s.add_message(make_msg(&i.to_string()));
        }
        assert_eq!(s.messages.len(), 10);
        assert_eq!(s.messages.front().unwrap().id, "0");
        assert_eq!(s.messages.back().unwrap().id, "9");
    }

    #[test]
    fn clear_messages_empties_deque() {
        let mut s = AthenaState::default();
        for i in 0..5 {
            s.add_message(make_msg(&i.to_string()));
        }
        s.clear_messages();
        assert!(s.messages.is_empty());
    }

    #[test]
    fn agent_context_is_added_once_per_exact_reference() {
        let mut s = AthenaState::default();
        assert!(s.add_agent_context("pane-1", "claude", "Builder"));
        assert!(!s.add_agent_context("pane-1", "claude", "Builder"));
        assert!(!s.add_agent_context("pane-1", "codex", "Renamed Builder"));
        assert_eq!(s.dropped_context.len(), 1);
    }

    #[test]
    fn agent_context_keeps_distinct_panes_separate() {
        let mut s = AthenaState::default();
        assert!(s.add_agent_context("pane-1", "claude", "Builder"));
        assert!(s.add_agent_context("pane-2", "codex", "Reviewer"));
        assert_eq!(s.dropped_context.len(), 2);
    }

    #[test]
    fn prepare_retry_removes_failed_turn_without_touching_earlier_history() {
        let mut s = AthenaState::default();
        s.add_message(AthenaMessage {
            id: "old-user".into(),
            role: MessageRole::User,
            content: "older question".into(),
            timestamp: 0,
            is_error: false,
            images: Vec::new(),
            blocks: Vec::new(),
        });
        s.add_message(AthenaMessage {
            id: "failed-user".into(),
            role: MessageRole::User,
            content: "try again".into(),
            timestamp: 0,
            is_error: false,
            images: Vec::new(),
            blocks: Vec::new(),
        });
        s.add_message(AthenaMessage {
            id: "failed-assistant".into(),
            role: MessageRole::Athena,
            content: "Error: timeout".into(),
            timestamp: 0,
            is_error: true,
            images: Vec::new(),
            blocks: Vec::new(),
        });
        s.error = Some("timeout".into());

        assert!(s.prepare_retry("try again"));
        assert_eq!(s.messages.len(), 1);
        assert_eq!(s.messages.front().unwrap().content, "older question");
        assert!(s.error.is_none());
    }
}
