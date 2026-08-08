//! Workspace-aware app-state snapshot construction for the orchestrator.

use super::AthenaOrchestrator;
use std::time::{Duration, Instant};

impl AthenaOrchestrator {
    pub(super) fn active_space_scope(&self) -> Option<(String, std::collections::HashSet<String>)> {
        let store = self.kv_store.as_ref()?;
        let json = store.get::<String>("workspaces").ok()??;
        let val: serde_json::Value = serde_json::from_str(&json).ok()?;
        let active_id = val.get("active_space_id")?.as_str()?;
        let spaces = val.get("spaces")?.as_array()?;
        let space = spaces
            .iter()
            .find(|s| s.get("id").and_then(|v| v.as_str()) == Some(active_id))?;
        let name = space
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();
        let pane_ids = space
            .get("panes")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|p| p.get("id").and_then(|v| v.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        Some((name, pane_ids))
    }

    pub(super) fn build_app_state_snapshot(&self) -> String {
        const SNAPSHOT_TTL: Duration = Duration::from_secs(1);
        if let Some((cached, ts)) = self.snapshot_cache.lock().as_ref() {
            if ts.elapsed() < SNAPSHOT_TTL {
                return cached.clone();
            }
        }

        let mut lines: Vec<String> = Vec::new();

        // --- Header ---
        lines.push("[Current State]".to_string());

        // --- Workspace ---
        let scope = self.active_space_scope();
        let workspace_name = scope
            .as_ref()
            .map(|(name, _)| name.clone())
            .or_else(|| self.workspace_name.lock().clone())
            .unwrap_or_else(|| "Unknown".to_string());
        lines.push(format!("Workspace: {}", workspace_name));

        // --- Agents ---
        let in_scope = |id: &str| scope.as_ref().is_none_or(|(_, ids)| ids.contains(id));
        let mut agent_lines: Vec<String> = Vec::new();
        let mut has_agents = false;
        if let Some(ref ob) = self.output_buffer {
            for pane in ob.get_agent_list().iter().filter(|p| in_scope(&p.pane_id)) {
                has_agents = true;
                let mins =
                    (chrono::Utc::now().timestamp_millis() - pane.last_activity_at as i64) / 60000;
                agent_lines.push(format!(
                    "  {}: {} | {} lines | idle {}m",
                    pane.pane_id, pane.agent_type, pane.line_count, mins
                ));
            }
        }
        if let Some(ref ac) = self.agent_comms {
            for s in ac
                .get_agent_sessions()
                .iter()
                .filter(|s| in_scope(&s.agent_id))
            {
                has_agents = true;
                let status_str = format!("{:?}", s.status);
                let status = if status_str == "\"\"" || status_str == "Empty" {
                    "idle"
                } else {
                    &status_str
                };
                agent_lines.push(format!("  {}: {} | status={}", s.id, s.agent_id, status));
            }
        }
        if !has_agents {
            agent_lines.push("  (none running)".to_string());
        }
        // Agent budget: 200 tokens
        let mut agent_text = agent_lines.join("\n");
        if Self::estimate_tokens(&agent_text) > 200 {
            let mut truncated = agent_lines[..agent_lines.len().min(10)].join("\n");
            truncated.push_str("\n  [truncated]");
            agent_text = truncated;
        }
        lines.push("Agents:".to_string());
        lines.push(agent_text);

        // --- Execution Plan ---
        let mut plan_lines: Vec<String> = Vec::new();
        plan_lines.push("Execution Plan:".to_string());
        if let Some(ref pm) = self.plan_manager {
            if let Some(plan) = pm.get_active_plan() {
                let step_info: Vec<String> = plan
                    .steps
                    .iter()
                    .take(5) // limit to 5 steps
                    .map(|s| format!("  {}: — {:?}", s.id, s.status))
                    .collect();
                plan_lines.push(format!("  {}: {:?}", plan.id, plan.status));
                plan_lines.extend(step_info);
            } else {
                plan_lines.push("  (none active)".to_string());
            }
        } else {
            plan_lines.push("  (none active)".to_string());
        }
        let plan_text = plan_lines.join("\n");
        lines.push(plan_text);

        // --- Kanban ---
        let kanban_text = "Kanban: use kanban_list_tasks to query".to_string();
        lines.push(kanban_text);

        let snapshot = lines.join("\n");
        *self.snapshot_cache.lock() = Some((snapshot.clone(), Instant::now()));
        snapshot
    }
}
