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

        // Dispatch the agent — default to "claude" as the agent type
        // since the plan step does not carry its own agent_type field.
        let default_agent_type = "claude";
        let agent_cmd = build_agent_command(default_agent_type, Some(&step.description));
        let pane_id = format!(
            "plan-{}-{}-{}",
            plan.id,
            step_id,
            &Uuid::new_v4().to_string()[..8]
        );
        self.event_sender
            .agent_spawned(&pane_id, default_agent_type, &agent_cmd);

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

        // Update step statuses
        if let Some(ref evals) = args.step_evaluations {
            for eval_item in evals {
                let step_id = eval_item
                    .get("step_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let status_str = eval_item
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("failure");

                let step_status = if status_str == "success" {
                    StepStatus::Completed
                } else {
                    StepStatus::Failed
                };

                let _ = self
                    .plan_manager
                    .update_step_status(step_id, step_status, None);
            }
        }

        // Update plan status
        let status_map: HashMap<&str, PlanStatus> = {
            let mut m = HashMap::new();
            m.insert("success", PlanStatus::Completed);
            m.insert("partial_success", PlanStatus::Completed);
            m.insert("failure", PlanStatus::Failed);
            m.insert("needs_replanning", PlanStatus::Failed);
            m
        };

        let plan_status = args
            .overall_status
            .as_deref()
            .and_then(|s| status_map.get(s))
            .copied()
            .unwrap_or(PlanStatus::Completed);

        let _ = self.plan_manager.update_plan_status(plan_status);

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
            args.overall_status.as_deref().unwrap_or("unknown"),
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
                args.overall_status.as_deref().unwrap_or("unknown"),
                instruction
            ),
            is_error: None,
        })
    }
}
