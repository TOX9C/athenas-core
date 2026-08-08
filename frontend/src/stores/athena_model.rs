//! Pure Athena message and content contracts.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_stable() {
        assert_eq!(MessageRole::default(), MessageRole::User);
        assert_eq!(
            ContentBlock::default(),
            ContentBlock::Plan(PlanBlock::default())
        );
        assert_eq!(ImageMediaType::default(), ImageMediaType::Jpeg);
        assert_eq!(PlanStepStatus::default(), PlanStepStatus::Pending);
        assert_eq!(PlanStatus::default(), PlanStatus::Pending);
    }

    #[test]
    fn plan_id_only_applies_to_plan_blocks() {
        let plan = ContentBlock::Plan(PlanBlock {
            plan_id: "plan-42".to_string(),
            ..Default::default()
        });
        assert_eq!(plan.plan_id(), Some("plan-42"));
        assert_eq!(
            ContentBlock::AskUser(AskUserBlock::default()).plan_id(),
            None
        );
        assert_eq!(
            ContentBlock::Evaluation(EvaluationBlock::default()).plan_id(),
            None
        );
    }

    #[test]
    fn message_preserves_nested_content_contracts() {
        let message = AthenaMessage {
            id: "message-1".to_string(),
            role: MessageRole::Athena,
            content: "Working".to_string(),
            blocks: vec![ContentBlock::Plan(PlanBlock {
                plan_id: "plan-1".to_string(),
                goal: "Refactor".to_string(),
                ..Default::default()
            })],
            ..Default::default()
        };
        assert_eq!(message.role, MessageRole::Athena);
        assert_eq!(message.blocks[0].plan_id(), Some("plan-1"));
    }
}
