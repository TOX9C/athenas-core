use dioxus::prelude::*;
use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Image attachment within an Athena message.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ImageAttachment {
    pub id: String,
    pub base64: String,
    pub media_type: ImageMediaType,
    pub name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ImageMediaType {
    #[default]
    Jpeg,
    Png,
    Gif,
    Webp,
}

/// Status of a plan step inside a plan block.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PlanStepStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// A single step within a PlanBlock.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanStepBlock {
    pub id: String,
    pub title: String,
    pub description: String,
    pub agent_type: String,
    pub status: PlanStepStatus,
    pub assigned_pane_id: Option<String>,
    pub result_summary: Option<String>,
}

/// Status of a top-level plan block.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum PlanStatus {
    #[default]
    Pending,
    InProgress,
    Completed,
    Failed,
}

/// A plan content block within an Athena message.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PlanBlock {
    pub plan_id: String,
    pub goal: String,
    pub steps: Vec<PlanStepBlock>,
    pub status: PlanStatus,
}

/// An ask-user content block within an Athena message.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AskUserOption {
    pub label: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AskUserBlock {
    pub request_id: String,
    pub question: String,
    pub options: Vec<AskUserOption>,
    pub answered: bool,
    pub selected_answer: Option<String>,
}

/// An evaluation content block within an Athena message.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct StepEvaluation {
    pub step_id: String,
    pub status: String,
    pub summary: String,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EvaluationBlock {
    pub plan_id: String,
    pub overall_status: String,
    pub step_evaluations: Vec<StepEvaluation>,
    pub next_action: String,
    pub reasoning: String,
}

/// Discriminated content block attached to a message.
#[derive(Debug, Clone, PartialEq)]
pub enum ContentBlock {
    Plan(PlanBlock),
    AskUser(AskUserBlock),
    Evaluation(EvaluationBlock),
}

impl Default for ContentBlock {
    fn default() -> Self {
        ContentBlock::Plan(PlanBlock::default())
    }
}

impl ContentBlock {
    pub fn plan_id(&self) -> Option<&str> {
        match self {
            ContentBlock::Plan(p) => Some(&p.plan_id),
            _ => None,
        }
    }
}

/// An item that has been dragged/dropped onto the Athena panel for context.
#[derive(Debug, Clone, PartialEq)]
pub enum DraggableItem {
    Agent {
        pane_id: String,
        agent_type: String,
        label: String,
    },
    KanbanTask {
        task_id: String,
        title: String,
        status: String,
    },
    File {
        path: String,
        name: String,
    },
}

/// Role of a message sender.
#[derive(Debug, Clone, PartialEq, Default)]
pub enum MessageRole {
    #[default]
    User,
    Athena,
}

/// A single chat message in the Athena conversation.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AthenaMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: i64,
    pub is_error: bool,
    pub images: Vec<ImageAttachment>,
    pub blocks: Vec<ContentBlock>,
}

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
        self.streaming_status = status;
    }

    pub fn set_error(&mut self, error: Option<String>) {
        self.error = error;
    }

    pub fn clear_error(&mut self) {
        self.error = None;
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
        self.messages.clear();
        self.error = None;
    }

    pub fn set_messages(&mut self, messages: Vec<AthenaMessage>) {
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

    // -- Event handlers for Tauri push events -------------------------------

    /// Handle athena:status event — update thinking/working/idle state.
    pub fn handle_status_event(&mut self, status: &str, detail: Option<String>) {
        match status {
            "thinking" | "working" => {
                self.is_streaming = true;
                self.streaming_status = detail.or_else(|| Some(status.to_string()));
            }
            "idle" | "completed" => {
                self.is_streaming = false;
                self.streaming_status = None;
            }
            "error" => {
                self.is_streaming = false;
                self.streaming_status = None;
                self.error = detail;
            }
            _ => {}
        }
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
            .map_or(false, |m| m.role == MessageRole::Athena);
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
            .map_or(false, |m| m.role == MessageRole::Athena);
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
            .map_or(false, |m| m.role == MessageRole::Athena);
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
}
