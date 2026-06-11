use std::sync::{Arc, RwLock};
use thiserror::Error;

/// Status of an execution plan.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// Status of an individual plan step.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Cancelled,
}

/// A single step in a plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStep {
    pub id: String,
    pub description: String,
    pub status: StepStatus,
    pub assigned_pane_id: Option<String>,
}

/// An execution plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExecutionPlan {
    pub id: String,
    pub goal: String,
    pub reasoning: String,
    pub steps: Vec<PlanStep>,
    pub status: PlanStatus,
    pub created_at: u64,
}

/// Input for creating a new plan.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanInput {
    pub goal: String,
    pub reasoning: String,
    pub steps: Vec<PlanStepInput>,
}

/// Input for a single plan step (without status).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PlanStepInput {
    pub id: String,
    pub description: String,
}

/// Errors for the plan manager.
#[derive(Debug, Error)]
pub enum PlanManagerError {
    #[error("No active plan")]
    NoActivePlan,
    #[error("Step not found: {0}")]
    StepNotFound(String),
    #[error("Lock poisoned")]
    LockPoisoned,
}

/// Thread-safe plan manager.
pub struct PlanManager {
    active_plan: Arc<RwLock<Option<ExecutionPlan>>>,
    event_emitter:
        Arc<parking_lot::Mutex<Option<Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>>>>,
}

impl std::fmt::Debug for PlanManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PlanManager")
            .field("active_plan", &"<RwLock<Option>>")
            .field("event_emitter", &"<Option>")
            .finish()
    }
}

impl Clone for PlanManager {
    fn clone(&self) -> Self {
        Self {
            active_plan: Arc::clone(&self.active_plan),
            event_emitter: Arc::clone(&self.event_emitter),
        }
    }
}

impl Default for PlanManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PlanManager {
    pub fn new() -> Self {
        Self {
            active_plan: Arc::new(RwLock::new(None)),
            event_emitter: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Set an event emitter callback for forwarding events to the frontend.
    pub fn set_event_emitter<F>(&self, emitter: F)
    where
        F: Fn(&str, &serde_json::Value) + Send + Sync + 'static,
    {
        let mut guard = self.event_emitter.lock();
        *guard = Some(Box::new(emitter));
    }

    fn emit_event(&self, channel: &str, data: &serde_json::Value) {
        let guard = self.event_emitter.lock();
        if let Some(ref emitter) = *guard {
            emitter(channel, data);
            return;
        }
        log::debug!("[plan-manager] {} -> {}", channel, data);
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn generate_id() -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        format!("plan-{:08x}", n)
    }

    /// Set the active plan. Overwrites any existing plan.
    pub fn set_active_plan(&self, input: PlanInput) -> Result<ExecutionPlan, PlanManagerError> {
        let mut lock = self
            .active_plan
            .write()
            .map_err(|_| PlanManagerError::LockPoisoned)?;
        let plan = ExecutionPlan {
            id: Self::generate_id(),
            goal: input.goal,
            reasoning: input.reasoning,
            steps: input
                .steps
                .into_iter()
                .map(|s| PlanStep {
                    id: s.id,
                    description: s.description,
                    status: StepStatus::Pending,
                    assigned_pane_id: None,
                })
                .collect(),
            status: PlanStatus::Pending,
            created_at: Self::now(),
        };
        *lock = Some(plan.clone());
        let plan_clone = plan.clone();
        drop(lock);

        self.emit_event(
            "athena:planUpdate",
            &serde_json::to_value(&plan_clone).unwrap_or_default(),
        );

        Ok(plan)
    }

    /// Get the current active plan.
    pub fn get_active_plan(&self) -> Option<ExecutionPlan> {
        let lock = match self.active_plan.read() {
            Ok(guard) => guard,
            Err(_) => {
                log::error!("PlanManager: lock poisoned while reading active plan");
                return None;
            }
        };
        lock.clone()
    }

    /// Update the status of a specific step.
    pub fn update_step_status(
        &self,
        step_id: &str,
        status: StepStatus,
        pane_id: Option<&str>,
    ) -> Result<bool, PlanManagerError> {
        let mut lock = self
            .active_plan
            .write()
            .map_err(|_| PlanManagerError::LockPoisoned)?;
        let plan = lock.as_mut().ok_or(PlanManagerError::NoActivePlan)?;
        let step = plan
            .steps
            .iter_mut()
            .find(|s| s.id == step_id)
            .ok_or_else(|| PlanManagerError::StepNotFound(step_id.to_string()))?;

        step.status = status;
        if let Some(pid) = pane_id {
            step.assigned_pane_id = Some(pid.to_string());
        }

        // Auto-update plan status if any step is in-progress and plan is still pending
        let has_in_progress = plan
            .steps
            .iter()
            .any(|s| s.status == StepStatus::InProgress);
        if has_in_progress && plan.status == PlanStatus::Pending {
            plan.status = PlanStatus::InProgress;
        }

        let plan_clone = plan.clone();
        drop(lock);

        self.emit_event(
            "athena:planUpdate",
            &serde_json::to_value(&plan_clone).unwrap_or_default(),
        );

        Ok(true)
    }

    /// Update the overall plan status.
    pub fn update_plan_status(&self, status: PlanStatus) -> Result<bool, PlanManagerError> {
        let mut lock = self
            .active_plan
            .write()
            .map_err(|_| PlanManagerError::LockPoisoned)?;
        let plan = lock.as_mut().ok_or(PlanManagerError::NoActivePlan)?;
        plan.status = status;
        let plan_clone = plan.clone();
        drop(lock);

        self.emit_event(
            "athena:planUpdate",
            &serde_json::to_value(&plan_clone).unwrap_or_default(),
        );

        Ok(true)
    }

    /// Clear the active plan.
    pub fn clear_active_plan(&self) -> Result<(), PlanManagerError> {
        let mut lock = self
            .active_plan
            .write()
            .map_err(|_| PlanManagerError::LockPoisoned)?;
        *lock = None;
        drop(lock);

        self.emit_event("athena:planUpdate", &serde_json::json!(null));

        Ok(())
    }
}
