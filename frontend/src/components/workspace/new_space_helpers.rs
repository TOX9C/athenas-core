//! State and serialization helpers for the new-space modal.

use crate::stores::workspace::AgentType;
use crate::types::swarm::AgentRole;
use crate::utils::agent_commands::get_agent_label;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AgentRowState {
    pub(super) agent_type: AgentType,
    pub(super) label: String,
    pub(super) custom_id: Option<String>,
    pub(super) custom_cmd: Option<String>,
    pub(super) count: usize,
}

/// Build the initial list of agent rows, merging built-ins with
/// any user-defined custom agents from the UI store.
pub(super) fn init_agent_rows(
    custom_agents: &[crate::types::workspace::CustomAgent],
) -> Vec<AgentRowState> {
    let mut rows = Vec::new();
    for at in [
        AgentType::Claude,
        AgentType::Codex,
        AgentType::Opencode,
        AgentType::Gemini,
        AgentType::Qwen,
        AgentType::Aider,
        AgentType::Cursor,
        AgentType::Freebuff,
        AgentType::Omp,
        AgentType::Shell,
    ]
    .iter()
    {
        rows.push(AgentRowState {
            agent_type: at.clone(),
            label: get_agent_label(at).to_string(),
            custom_id: None,
            custom_cmd: None,
            count: 0,
        });
    }
    for ca in custom_agents {
        rows.push(AgentRowState {
            agent_type: AgentType::Custom,
            label: ca.alias.clone(),
            custom_id: Some(ca.id.clone()),
            custom_cmd: Some(ca.command.clone()),
            count: 0,
        });
    }
    rows
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct AgentSlot {
    pub(super) role: AgentRole,
    pub(super) agent_type: AgentType,
    pub(super) custom_id: Option<String>,
    pub(super) custom_cmd: Option<String>,
    pub(super) label: Option<String>,
}

pub(super) fn role_color(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Coordinator => "#0ea5e9",
        AgentRole::Builder => "#22c55e",
        AgentRole::Scout => "#f59e0b",
        AgentRole::Reviewer => "#06b6d4",
    }
}

pub(super) fn agent_role_str(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Coordinator => "coordinator",
        AgentRole::Builder => "builder",
        AgentRole::Scout => "scout",
        AgentRole::Reviewer => "reviewer",
    }
}

pub(super) fn agent_type_str(at: &AgentType) -> &'static str {
    match at {
        AgentType::Claude => "claude",
        AgentType::Codex => "codex",
        AgentType::Opencode => "opencode",
        AgentType::Gemini => "gemini",
        AgentType::Qwen => "qwen",
        AgentType::Aider => "aider",
        AgentType::Cursor => "cursor",
        AgentType::Freebuff => "freebuff",
        AgentType::Omp => "omp",
        AgentType::Custom => "custom",
        AgentType::Shell => "shell",
    }
}

pub(super) fn parse_agent_type(s: &str) -> AgentType {
    match s {
        "claude" => AgentType::Claude,
        "codex" => AgentType::Codex,
        "opencode" => AgentType::Opencode,
        "gemini" => AgentType::Gemini,
        "qwen" => AgentType::Qwen,
        "aider" => AgentType::Aider,
        "cursor" => AgentType::Cursor,
        "freebuff" => AgentType::Freebuff,
        "omp" => AgentType::Omp,
        "custom" => AgentType::Custom,
        "shell" => AgentType::Shell,
        _ => AgentType::Shell,
    }
}

/// Encode an AgentSlot for the swarm <select> `value` attribute.
/// For built-in agents, returns e.g. "claude". For custom agents,
/// embeds the custom id so the option can be uniquely identified.
pub(super) fn slot_value(slot: &AgentSlot) -> String {
    if let Some(ref id) = slot.custom_id {
        format!("custom${}", id)
    } else {
        agent_type_str(&slot.agent_type).to_string()
    }
}

/// Decode a swarm <select> value and update an AgentSlot in place.
pub(super) fn apply_slot_value(
    slot: &mut AgentSlot,
    val: &str,
    custom_agents: &[crate::types::workspace::CustomAgent],
) {
    if let Some(id) = val.strip_prefix("custom$") {
        if let Some(ca) = custom_agents.iter().find(|c| c.id == id) {
            slot.agent_type = AgentType::Custom;
            slot.custom_id = Some(ca.id.clone());
            slot.custom_cmd = Some(ca.command.clone());
            slot.label = Some(ca.alias.clone());
            return;
        }
    }
    let at = parse_agent_type(val);
    slot.agent_type = at;
    slot.custom_id = None;
    slot.custom_cmd = None;
    slot.label = None;
}

pub(super) fn parse_agent_role(s: &str) -> AgentRole {
    match s {
        "coordinator" => AgentRole::Coordinator,
        "builder" => AgentRole::Builder,
        "scout" => AgentRole::Scout,
        "reviewer" => AgentRole::Reviewer,
        _ => AgentRole::Builder,
    }
}

pub(super) fn generate_id() -> String {
    let ts = crate::utils::time::now_ms();
    static COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let count = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("{:x}-{:x}", ts, count)
}
