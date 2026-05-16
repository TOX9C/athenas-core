use dioxus::prelude::*;

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

/// A user-defined custom agent shortcut.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CustomAgent {
    pub id: String,
    pub name: String,
    pub command: String,
}

// ---------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------

/// Maximum number of messages kept in memory.
const MAX_MESSAGES: usize = 100;

/// Global Athena chat state.
#[derive(Clone, PartialEq, Default)]
pub struct AthenaState {
    pub messages: Vec<AthenaMessage>,
    pub is_open: bool,
    pub is_loading: bool,
    pub is_streaming: bool,
    pub streaming_status: Option<String>,
    pub error: Option<String>,
    pub model: String,
    pub provider: String,
    pub bypass_mode: bool,
    pub auto_launch: bool,
    pub custom_agents: Vec<CustomAgent>,
}

impl AthenaState {
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            is_open: false,
            is_loading: false,
            is_streaming: false,
            streaming_status: None,
            error: None,
            model: "claude".to_string(),
            provider: "anthropic".to_string(),
            bypass_mode: true,
            auto_launch: true,
            custom_agents: Vec::new(),
        }
    }

    // -- Mutators (in-place, compatible with Signal::write()) ---------------

    pub fn add_message(&mut self, msg: AthenaMessage) {
        self.messages.push(msg);
        if self.messages.len() > MAX_MESSAGES {
            let excess = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(0..excess);
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

    pub fn add_custom_agent(&mut self, agent: CustomAgent) {
        self.custom_agents.push(agent);
    }

    pub fn set_custom_agents(&mut self, agents: Vec<CustomAgent>) {
        self.custom_agents = agents;
    }

    pub fn remove_custom_agent(&mut self, id: &str) {
        self.custom_agents.retain(|a| a.id != id);
    }

    pub fn clear_messages(&mut self) {
        self.messages.clear();
        self.error = None;
    }

    pub fn set_messages(&mut self, messages: Vec<AthenaMessage>) {
        self.messages = messages;
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
    pub fn handle_ask_user(&mut self, request_id: String, question: String, options: Vec<AskUserOption>) {
        let ask_block = ContentBlock::AskUser(AskUserBlock {
            request_id,
            question,
            options,
            answered: false,
            selected_answer: None,
        });

        // Try to append to the last assistant message
        let last_is_athena = self.messages.last().map_or(false, |m| m.role == MessageRole::Athena);
        if last_is_athena {
            if let Some(msg) = self.messages.last_mut() {
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
        let last_is_athena = self.messages.last().map_or(false, |m| m.role == MessageRole::Athena);
        if last_is_athena {
            if let Some(msg) = self.messages.last_mut() {
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
        let last_is_athena = self.messages.last().map_or(false, |m| m.role == MessageRole::Athena);
        if last_is_athena {
            if let Some(msg) = self.messages.last_mut() {
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
