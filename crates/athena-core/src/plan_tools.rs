//! Execution-plan tool implementations for [`ToolExecutor`].

use super::{ToolCallResult, ToolExecutor, ToolExecutorError, ToolInput};
use crate::plan_manager::{PlanInput, PlanStatus, PlanStepInput, StepStatus};
use crate::tool_schema::build_agent_command;
use std::collections::HashMap;
use uuid::Uuid;

impl ToolExecutor {
    pub(super) fn create_execution_plan(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let goal = args
            .goal
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("goal".to_string()))?;
        let reasoning = args
            .reasoning
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("reasoning".to_string()))?;

        let steps: Vec<PlanStepInput> = args
            .steps
            .as_deref()
            .unwrap_or(&[])
            .iter()
            .map(|s| PlanStepInput {
                id: s
                    .get("id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                description: s
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                agent_type: s
                    .get("agent_type")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string()),
            })
            .collect();

        let plan = self.plan_manager.set_active_plan(PlanInput {
            goal: goal.to_string(),
            reasoning: reasoning.to_string(),
            steps,
        })?;

        self.event_sender.plan_update(&plan);

        if let Some(ref svc) = self.notification_service {
            let _ = svc.notify(
                crate::notification::NotificationType::Info,
                "Plan Created",
                format!("Execution plan created: {}", goal),
            );
        }

        let step_summary: String = plan
            .steps
            .iter()
            .map(|s| format!("  {}: {}", s.id, s.description))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolCallResult {
            text: format!(
                "Plan created ({}):\nGoal: {}\nSteps:\n{}",
                plan.id, plan.goal, step_summary
            ),
            is_error: None,
        })
    }

    pub(super) fn dispatch_plan_step(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let step_id = args
            .step_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("step_id".to_string()))?;

        let plan = match self.plan_manager.get_active_plan() {
            Some(p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "No active execution plan. Create one first with create_execution_plan."
                        .to_string(),
                    is_error: None,
                })
            }
        };

        let step = match plan.steps.iter().find(|s| s.id == step_id) {
            Some(s) => s,
            None => {
                return Ok(ToolCallResult {
                    text: format!("Step '{}' not found in active plan.", step_id),
                    is_error: None,
                })
            }
        };

        // Dispatch the agent. The step carries an optional agent_type so a
        // plan can target shell/codex/gemini agents; default to "claude".
        let agent_type = step
            .agent_type
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or("claude");
        let agent_cmd = build_agent_command(agent_type, Some(&step.description));
        let pane_id = format!(
            "plan-{}-{}-{}",
            plan.id,
            step_id,
            &Uuid::new_v4().to_string()[..8]
        );
        self.event_sender
            .agent_spawned(&pane_id, agent_type, &agent_cmd);

        self.plan_manager
            .update_step_status(step_id, StepStatus::InProgress, Some(&pane_id))?;

        if let Some(updated_plan) = self.plan_manager.get_active_plan() {
            self.event_sender.plan_update(&updated_plan);
        }

        if let Some(ref svc) = self.notification_service {
            let _ = svc.notify(
                crate::notification::NotificationType::Info,
                "Step Dispatched",
                format!("Dispatched step: {}", step_id),
            );
        }

        Ok(ToolCallResult {
            text: format!(
                "Dispatched step '{}' ({}) -> pane {}",
                step_id, step.description, pane_id
            ),
            is_error: None,
        })
    }

    pub(super) fn evaluate_results(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let plan = match self.plan_manager.get_active_plan() {
            Some(p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "No active execution plan to evaluate.".to_string(),
                    is_error: None,
                })
            }
        };

        let overall_status = args
            .overall_status
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("overall_status".to_string()))?;

        let status_map: HashMap<&str, PlanStatus> = {
            let mut m = HashMap::new();
            m.insert("success", PlanStatus::Completed);
            m.insert("partial_success", PlanStatus::Completed);
            m.insert("failure", PlanStatus::Failed);
            m.insert("needs_replanning", PlanStatus::Failed);
            m
        };

        let plan_status = status_map.get(overall_status).copied().ok_or_else(|| {
            ToolExecutorError::InvalidParam(
                "overall_status must be one of: success, partial_success, failure, needs_replanning"
                    .to_string(),
            )
        })?;

        // Single pass: validate and collect typed results before mutating
        // the plan. A malformed entry errors out before any step changes,
        // and the apply loop below consumes only pre-validated values.
        let evals = args.step_evaluations.as_deref().unwrap_or(&[]);
        let mut validated: Vec<(&str, StepStatus)> = Vec::with_capacity(evals.len());
        for (index, eval_item) in evals.iter().enumerate() {
            let step_id = eval_item
                .get("step_id")
                .and_then(|v| v.as_str())
                .filter(|id| !id.is_empty())
                .ok_or_else(|| {
                    ToolExecutorError::InvalidParam(format!(
                        "step_evaluations[{index}].step_id must be a non-empty string"
                    ))
                })?;
            let step_status = match eval_item.get("status").and_then(|v| v.as_str()) {
                Some("success") => StepStatus::Completed,
                Some("failure") => StepStatus::Failed,
                _ => {
                    return Err(ToolExecutorError::InvalidParam(format!(
                        "step_evaluations[{index}].status must be success or failure"
                    )));
                }
            };
            if !plan.steps.iter().any(|step| step.id == step_id) {
                return Err(crate::plan_manager::PlanManagerError::StepNotFound(
                    step_id.to_string(),
                )
                .into());
            }
            validated.push((step_id, step_status));
        }

        for (step_id, step_status) in validated {
            self.plan_manager
                .update_step_status(step_id, step_status, None)?;
        }

        self.plan_manager.update_plan_status(plan_status)?;

        let updated_plan = match self.plan_manager.get_active_plan() {
            Some(p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "No active execution plan to evaluate.".to_string(),
                    is_error: None,
                })
            }
        };

        self.event_sender.plan_update(&updated_plan);

        let evals = args.step_evaluations.as_deref().unwrap_or(&[]);
        self.event_sender.plan_evaluated(
            &plan.id,
            overall_status,
            evals,
            args.next_action.as_deref().unwrap_or("done"),
            args.reasoning.as_deref().unwrap_or(""),
        );

        let action_instructions: HashMap<&str, &str> = {
            let mut m = HashMap::new();
            m.insert("done", "Plan complete. Report results to the user.");
            m.insert(
                "replan",
                "Create a new execution plan addressing the failures.",
            );
            m.insert("retry_steps", "Re-dispatch the failed steps.");
            m.insert(
                "escalate_to_user",
                "Ask the user for guidance on how to proceed.",
            );
            m
        };

        let next = args.next_action.as_deref().unwrap_or("done");
        let instruction = action_instructions.get(next).copied().unwrap_or(next);

        Ok(ToolCallResult {
            text: format!(
                "Evaluation recorded. Overall: {}. Next: {}",
                overall_status, instruction
            ),
            is_error: None,
        })
    }
}
