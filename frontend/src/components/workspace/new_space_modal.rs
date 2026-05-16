use super::grid_template::GridTemplateSelector;
use crate::components::shared::modal::Modal;
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{
    grid_for_pane_count, use_workspace_store, AgentType, GridTemplate, PaneConfig, Space,
};
use crate::types::swarm::{AgentRole, SwarmAgent, SwarmAgentStatus};
use crate::utils::agent_commands::{get_agent_color, get_agent_label};
use dioxus::prelude::*;

const TAB_COLORS: &[&str] = &[
    "#0ea5e9", "#22c55e", "#f59e0b", "#ef4444", "#06b6d4", "#f97316", "#64748b",
];

const AGENT_TYPES: &[AgentType] = &[
    AgentType::Claude,
    AgentType::Codex,
    AgentType::Opencode,
    AgentType::Gemini,
    AgentType::Shell,
];

#[derive(Debug, Clone, PartialEq)]
struct AgentSlot {
    role: AgentRole,
    agent_type: AgentType,
}

fn role_color(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Coordinator => "#0ea5e9",
        AgentRole::Builder => "#22c55e",
        AgentRole::Scout => "#f59e0b",
        AgentRole::Reviewer => "#06b6d4",
    }
}

fn agent_role_str(role: &AgentRole) -> &'static str {
    match role {
        AgentRole::Coordinator => "coordinator",
        AgentRole::Builder => "builder",
        AgentRole::Scout => "scout",
        AgentRole::Reviewer => "reviewer",
    }
}

fn agent_type_str(at: &AgentType) -> &'static str {
    match at {
        AgentType::Claude => "claude",
        AgentType::Codex => "codex",
        AgentType::Opencode => "opencode",
        AgentType::Gemini => "gemini",
        AgentType::Custom => "custom",
        AgentType::Shell => "shell",
    }
}

fn parse_agent_type(s: &str) -> AgentType {
    match s {
        "claude" => AgentType::Claude,
        "codex" => AgentType::Codex,
        "opencode" => AgentType::Opencode,
        "gemini" => AgentType::Gemini,
        "custom" => AgentType::Custom,
        "shell" => AgentType::Shell,
        _ => AgentType::Shell,
    }
}

fn parse_agent_role(s: &str) -> AgentRole {
    match s {
        "coordinator" => AgentRole::Coordinator,
        "builder" => AgentRole::Builder,
        "scout" => AgentRole::Scout,
        "reviewer" => AgentRole::Reviewer,
        _ => AgentRole::Builder,
    }
}

fn generate_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("{:x}", ts)
}

#[derive(Props, Clone, PartialEq)]
pub struct NewSpaceModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn NewSpaceModal(props: NewSpaceModalProps) -> Element {
    let mut step = use_signal(|| 0u8);
    let mut mode = use_signal(|| "terminal".to_string());
    let mut space_name = use_signal(String::new);
    let mut space_dir = use_signal(String::new);
    let mut space_goal = use_signal(String::new);
    let mut selected_grid = use_signal(|| GridTemplate::X2x2);

    // Terminal mode: per-agent-type counts
    let mut pane_agents: Signal<Vec<(AgentType, usize)>> =
        use_signal(|| AGENT_TYPES.iter().map(|at| (at.clone(), 0)).collect());

    // Swarm mode: agent slots
    let mut slots: Signal<Vec<AgentSlot>> = use_signal(|| {
        vec![
            AgentSlot {
                role: AgentRole::Coordinator,
                agent_type: AgentType::Claude,
            },
            AgentSlot {
                role: AgentRole::Builder,
                agent_type: AgentType::Claude,
            },
            AgentSlot {
                role: AgentRole::Builder,
                agent_type: AgentType::Claude,
            },
        ]
    });

    let mut workspace_state = use_workspace_store();
    let mut ui_state = use_ui_store();

    let total_panes: usize = pane_agents.read().iter().map(|(_, c)| *c).sum();
    let coordinator_count = slots
        .read()
        .iter()
        .filter(|s| s.role == AgentRole::Coordinator)
        .count();
    let builder_count = slots
        .read()
        .iter()
        .filter(|s| s.role == AgentRole::Builder)
        .count();

    let is_swarm = mode() == "swarm";
    let can_launch_swarm = coordinator_count == 1 && builder_count >= 1;

    let total_steps = 2u8;

    // Footer navigation (rendered in Modal footer slot, outside scrollable body)
    let footer_el = if step() > 0 {
        rsx! {
            div {
                style: "display: flex; align-items: center; justify-content: space-between;",

                span {
                    style: "font-size: 11px; color: var(--textDim);",
                    "Step {step()} of {total_steps}"
                }

                div {
                    style: "display: flex; gap: 8px;",

                    button {
                        style: "padding: 6px 14px; border-radius: 6px; border: none; background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 11px;",
                        onclick: move |_| {
                            if step() == 1 { step.set(0); } else { step.set(step() - 1); }
                        },
                        "Back"
                    }

                    if step() == 1 {
                        {
                            let next_btn_style = format!(
                                "padding: 6px 16px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px; font-weight: 600; {}",
                                if space_dir.read().trim().is_empty() || (is_swarm && space_goal.read().trim().is_empty()) { "opacity: 0.4; pointer-events: none;" } else { "" }
                            );
                            rsx! {
                                button {
                                    style: "{next_btn_style}",
                                    onclick: move |_| step.set(2),
                                    "Next >"
                                }
                            }
                        }
                    }

                    if step() == 2 && mode() == "terminal" {
                        {
                            let launch_btn_style = format!(
                                "padding: 6px 16px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px; font-weight: 600; {}",
                                if total_panes == 0 { "opacity: 0.4; pointer-events: none;" } else { "" }
                            );
                            rsx! {
                                button {
                                    style: "{launch_btn_style}",
                                    onclick: move |_| {
                                        let dir = space_dir.read().trim().to_string();
                                        if dir.is_empty() { return; }

                                        let now_ts = chrono::Utc::now().timestamp_millis();
                                        let space_count = workspace_state.read().spaces.len();
                                        let name = if space_name.read().is_empty() {
                                            format!("Space {}", space_count + 1)
                                        } else {
                                            space_name.read().clone()
                                        };

                                        let mut panes = Vec::new();
                                        for (at, count) in pane_agents.read().iter() {
                                            for _ in 0..*count {
                                                panes.push(PaneConfig {
                                                    id: generate_id(),
                                                    agent_type: at.clone(),
                                                    custom_cmd: None,
                                                    custom_agent_id: None,
                                                    label: None,
                                                    bypass_mode: None,
                                                    project_name: None,
                                                    model_name: None,
                                                });
                                            }
                                        }

                                        let space = Space {
                                            id: generate_id(),
                                            name,
                                            dir,
                                            grid: selected_grid(),
                                            panes,
                                            color: TAB_COLORS[space_count % TAB_COLORS.len()].to_string(),
                                            created_at: now_ts,
                                            last_opened_at: now_ts,
                                        };

                                        workspace_state.write().add_space(space);
                                        props.on_close.call(());
                                    },
                                    "Launch Space"
                                }
                            }
                        }
                    }

                    if step() == 2 && mode() == "swarm" {
                        {
                            let swarm_btn_style = format!(
                                "padding: 6px 16px; border-radius: 6px; border: none; background: var(--accent); color: #0b0e13; cursor: pointer; font-size: 11px; font-weight: 600; {}",
                                if !can_launch_swarm { "opacity: 0.4; pointer-events: none;" } else { "" }
                            );
                            rsx! {
                                button {
                                    style: "{swarm_btn_style}",
                                    onclick: move |_| {
                                        let dir = space_dir.read().trim().to_string();
                                        let goal = space_goal.read().trim().to_string();
                                        if dir.is_empty() || goal.is_empty() { return; }

                                        let now_ts = chrono::Utc::now().timestamp_millis();
                                        let space_count = workspace_state.read().spaces.len();
                                        let name = if space_name.read().is_empty() {
                                            format!("Mission {}", space_count + 1)
                                        } else {
                                            space_name.read().clone()
                                        };

                                        let mut pane_configs = Vec::new();
                                        let mut _swarm_agents = Vec::new();
                                        for slot in slots.read().iter() {
                                            let pane_id = format!("swarm-{}", generate_id());
                                            let agent_id = generate_id();
                                            pane_configs.push(PaneConfig {
                                                id: pane_id.clone(),
                                                agent_type: slot.agent_type.clone(),
                                                custom_cmd: None,
                                                custom_agent_id: None,
                                                label: None,
                                                bypass_mode: None,
                                                project_name: None,
                                                model_name: None,
                                            });
                                            _swarm_agents.push(SwarmAgent {
                                                id: agent_id,
                                                role: slot.role.clone(),
                                                agent_type: slot.agent_type.clone(),
                                                pane_id,
                                                status: SwarmAgentStatus::Idle,
                                                current_task: None,
                                                last_action: "Spawned".to_string(),
                                                last_action_at: now_ts,
                                            });
                                        }

                                        let space = Space {
                                            id: generate_id(),
                                            name,
                                            dir,
                                            grid: GridTemplate::X2x2,
                                            panes: pane_configs,
                                            color: TAB_COLORS[space_count % TAB_COLORS.len()].to_string(),
                                            created_at: now_ts,
                                            last_opened_at: now_ts,
                                        };

                                        workspace_state.write().add_space(space);
                                        ui_state.write().panel = crate::stores::ui::Panel::Swarm;
                                        props.on_close.call(());
                                    },
                                    "Launch Swarm"
                                }
                            }
                        }
                    }
                }
            }
        }
    } else {
        rsx! {}
    };

    rsx! {
        Modal {
            title: if step() == 0 { "New Workspace" } else if mode() == "terminal" { "Terminal Workspace" } else { "Swarm Mission" },
            on_close: move |_| props.on_close.call(()),
            width: 560,
            footer: Some(footer_el),

            // Step dots
            if step() > 0 {
                div {
                    style: "display: flex; align-items: center; gap: 6px; margin-bottom: 16px;",

                    for i in 1..=total_steps {
                        {
                            let dot_bg = if step() >= i { "var(--accent)" } else { "var(--bgTertiary)" };
                            rsx! {
                                div {
                                    key: "{i}",
                                    style: "width: 6px; height: 6px; border-radius: 50%; background: {dot_bg}; transition: background 0.15s;",
                                }
                            }
                        }
                    }
                }
            }

            // Step 0: Mode selection
            if step() == 0 {
                div {
                    style: "display: flex; flex-direction: column; gap: 10px;",

                    p {
                        style: "font-size: 12px; color: var(--textDim); margin: 0 0 2px 0;",
                        "Choose workspace type"
                    }

                    // Terminal Workspace card
                    button {
                        style: if mode() == "terminal" {
                            "display: flex; align-items: center; gap: 16px; padding: 14px 16px; border-radius: 8px; border: 1px solid var(--accent); background: var(--bg); color: var(--text); cursor: pointer; text-align: left; width: 100%; transition: border-color 0.15s;"
                        } else {
                            "display: flex; align-items: center; gap: 16px; padding: 14px 16px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--text); cursor: pointer; text-align: left; width: 100%; transition: border-color 0.15s;"
                        },
                        onclick: move |_| {
                            mode.set("terminal".to_string());
                            step.set(1);
                        },

                        div {
                            style: "width: 40px; height: 40px; border-radius: 8px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; background: var(--bgTertiary);",
                            span { style: "font-size: 18px; color: var(--accent);", ">" }
                        }

                        div {
                            div { style: "font-size: 13px; font-weight: 600; color: var(--text);", "Terminal Workspace" }
                            span { style: "font-size: 11px; color: var(--textDim); display: block; margin-top: 2px;", "Launch multiple terminal panes with AI agents in a grid layout" }
                        }
                    }

                    // Swarm Mission card
                    button {
                        style: if mode() == "swarm" {
                            "display: flex; align-items: center; gap: 16px; padding: 14px 16px; border-radius: 8px; border: 1px solid var(--accent); background: var(--bg); color: var(--text); cursor: pointer; text-align: left; width: 100%; transition: border-color 0.15s;"
                        } else {
                            "display: flex; align-items: center; gap: 16px; padding: 14px 16px; border-radius: 8px; border: 1px solid var(--border); background: var(--bg); color: var(--text); cursor: pointer; text-align: left; width: 100%; transition: border-color 0.15s;"
                        },
                        onclick: move |_| {
                            mode.set("swarm".to_string());
                            step.set(1);
                        },

                        div {
                            style: "width: 40px; height: 40px; border-radius: 8px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; background: var(--bgTertiary);",
                            span { style: "font-size: 14px; font-weight: 700; color: var(--accent);", "SW" }
                        }

                        div {
                            div { style: "font-size: 13px; font-weight: 600; color: var(--text);", "Swarm Mission" }
                            span { style: "font-size: 11px; color: var(--textDim); display: block; margin-top: 2px;", "Orchestrate a team of AI agents on a shared goal" }
                        }
                    }
                }
            }

            // Step 1: Directory + goal
            if step() == 1 {
                div {
                    style: "display: flex; flex-direction: column; gap: 14px;",

                    // Space name
                    label {
                        style: "font-size: 11px; color: var(--textMuted); font-weight: 500;",
                        "Space Name"
                        input {
                            style: "width: 100%; padding: 8px 12px; margin-top: 4px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                            value: "{space_name}",
                            oninput: move |e| space_name.set(e.value()),
                            placeholder: "my-project"
                        }
                    }

                    // Working directory
                    label {
                        style: "font-size: 11px; color: var(--textMuted); font-weight: 500;",
                        "Working Directory"
                        div {
                            style: "display: flex; gap: 6px; margin-top: 4px;",
                            input {
                                style: "flex: 1; padding: 8px 12px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; outline: none; box-sizing: border-box;",
                                value: "{space_dir}",
                                oninput: move |e| space_dir.set(e.value()),
                                placeholder: "/path/to/project"
                            }
                            button {
                                style: "padding: 8px 12px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 11px; flex-shrink: 0; display: flex; align-items: center; gap: 4px;",
                                onclick: move |_| {
                                    let mut dir_signal = space_dir;
                                    spawn(async move {
                                        match crate::tauri_bridge::fs_show_open_dialog(
                                            Some("Select Working Directory"),
                                            true,
                                            false,
                                        )
                                        .await
                                        {
                                            Ok(result) if !result.is_empty() => {
                                                let cleaned = result.trim().trim_matches('"').to_string();
                                                if !cleaned.is_empty() && cleaned != "null" {
                                                    dir_signal.set(cleaned);
                                                }
                                            }
                                            Ok(_) => {}
                                            Err(e) => {
                                                log::warn!("Browse dialog error: {:?}", e);
                                            }
                                        }
                                    });
                                },
                                "Browse"
                            }
                        }
                    }

                    // Goal (swarm mode only)
                    if mode() == "swarm" {
                        label {
                            style: "font-size: 11px; color: var(--textMuted); font-weight: 500;",
                            "Goal"
                            textarea {
                                style: "width: 100%; padding: 8px 12px; margin-top: 4px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg); color: var(--text); font-size: 12px; outline: none; resize: vertical; min-height: 60px; box-sizing: border-box;",
                                value: "{space_goal}",
                                oninput: move |e| space_goal.set(e.value()),
                                placeholder: "Describe what the swarm should accomplish..."
                            }
                        }
                    }
                }
            }

            // Step 2: Grid + agents (terminal mode)
            if step() == 2 && mode() == "terminal" {
                div {
                    style: "display: flex; flex-direction: column; gap: 14px;",

                    // Grid layout selector
                    label {
                        style: "font-size: 11px; color: var(--textMuted); font-weight: 500;",
                        "Preset Layout"
                        GridTemplateSelector {
                            selected: selected_grid(),
                            on_select: move |g| selected_grid.set(g)
                        }
                    }

                    // Agent configuration
                    div {
                        style: "border-top: 1px solid var(--border); padding-top: 12px; margin-top: 4px;",

                        div {
                            style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px;",
                            label {
                                style: "font-size: 11px; color: var(--textMuted); font-weight: 500;",
                                "Agents ({total_panes}/16)"
                            }
                        }

                        for (idx, (at, count)) in pane_agents.read().iter().enumerate() {
                            {
                                let at_clone = at.clone();
                                let at_clone2 = at.clone();
                                let types_at = at_clone.clone();
                                let label = get_agent_label(&types_at);
                                let color = get_agent_color(&types_at);
                                let count_val = *count;
                                let has_any = count_val > 0;
                                let row_bg = if has_any { "var(--bgTertiary)" } else { "var(--bg)" };
                                let dot_bg = if has_any { color } else { "var(--textDim)" };
                                let text_color = if has_any { "var(--text)" } else { "var(--textMuted)" };
                                let minus_btn_style = format!(
                                    "width: 20px; height: 20px; display: flex; align-items: center; justify-content: center; border-radius: 50%; border: 1px solid var(--border); background: var(--bg); color: var(--text); cursor: pointer; font-size: 12px; line-height: 1; {}",
                                    if count_val == 0 { "opacity: 0.3; pointer-events: none;" } else { "" }
                                );
                                let plus_btn_style = format!(
                                    "width: 20px; height: 20px; display: flex; align-items: center; justify-content: center; border-radius: 50%; border: 1px solid var(--border); background: var(--bg); color: var(--text); cursor: pointer; font-size: 14px; line-height: 1; {}",
                                    if total_panes >= 16 { "opacity: 0.3; pointer-events: none;" } else { "" }
                                );

                                rsx! {
                                    div {
                                        key: "{idx}",
                                        style: "display: flex; align-items: center; justify-content: space-between; padding: 6px 8px; border-radius: 6px; border: 1px solid var(--border); background: {row_bg}; margin-bottom: 4px;",

                                        div {
                                            style: "display: flex; align-items: center; gap: 8px;",
                                            div {
                                                style: "width: 8px; height: 8px; border-radius: 50%; background: {dot_bg}; flex-shrink: 0;",
                                            }
                                            span {
                                                style: "font-size: 12px; font-weight: 500; color: {text_color};",
                                                "{label}"
                                            }
                                        }

                                        div {
                                            style: "display: flex; align-items: center; gap: 8px;",

                                            button {
                                                style: "{minus_btn_style}",
                                                onclick: move |_| {
                                                    let mut agents = pane_agents.write();
                                                    if let Some(pos) = agents.iter().position(|(a, _)| *a == at_clone) {
                                                        if agents[pos].1 > 0 {
                                                            agents[pos].1 -= 1;
                                                        }
                                                    }
                                                    drop(agents);
                                                    let total: usize = pane_agents.read().iter().map(|(_, c)| *c).sum();
                                                    selected_grid.set(grid_for_pane_count(total));
                                                },
                                                "\u{2013}"
                                            }

                                            span {
                                                style: "font-size: 12px; font-weight: 500; color: var(--text); width: 12px; text-align: center; font-variant-numeric: tabular-nums;",
                                                "{count_val}"
                                            }

                                            button {
                                                style: "{plus_btn_style}",
                                                onclick: move |_| {
                                                    let mut agents = pane_agents.write();
                                                    if let Some(pos) = agents.iter().position(|(a, _)| *a == at_clone2) {
                                                        if agents.iter().map(|(_, c)| *c).sum::<usize>() < 16 {
                                                            agents[pos].1 += 1;
                                                        }
                                                    }
                                                    drop(agents);
                                                    let total: usize = pane_agents.read().iter().map(|(_, c)| *c).sum();
                                                    selected_grid.set(grid_for_pane_count(total));
                                                },
                                                "+"
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Summary
                    div {
                        style: "padding: 10px 12px; border-radius: 8px; background: var(--bgTertiary); border: 1px solid var(--border); margin-top: 4px;",
                        div { style: "font-size: 10px; color: var(--textMuted); margin-bottom: 4px;", "Summary" }
                        div {
                            style: "font-size: 12px; color: var(--text);",
                            {space_name.read().clone()}
                        }
                        div {
                            style: "font-size: 10px; color: var(--textDim);",
                            {space_dir.read().clone()}
                        }
                    }
                }
            }

            // Step 2: Swarm team config
            if step() == 2 && mode() == "swarm" {
                {
                    let slots_count = slots.read().len();
                    let add_btn_style = format!(
                        "padding: 4px 10px; border-radius: 6px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--textMuted); cursor: pointer; font-size: 10px; {}",
                        if slots_count >= 10 { "opacity: 0.3; pointer-events: none;" } else { "" }
                    );
                    let team_label = format!("Team ({} agents)", slots_count);
                    rsx! {
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px;",

                            div {
                                style: "display: flex; align-items: center; justify-content: space-between;",
                                label {
                                    style: "font-size: 11px; color: var(--textMuted); font-weight: 500;",
                                    "{team_label}"
                                }
                                button {
                                    style: "{add_btn_style}",
                                    onclick: move |_| {
                                        if slots.read().len() < 10 {
                                            slots.write().push(AgentSlot { role: AgentRole::Builder, agent_type: AgentType::Claude });
                                        }
                                    },
                                    "+ Add"
                                }
                            }

                            for (idx, slot) in slots.read().iter().enumerate() {
                                {
                                    let slot_role_val = agent_role_str(&slot.role);
                                    let slot_agent_val = agent_type_str(&slot.agent_type);
                                    let dot_c = role_color(&slot.role);
                                    let cur_slots_len = slots.read().len();
                                    let remove_btn_style = format!(
                                        "padding: 2px 6px; border-radius: 4px; border: none; background: transparent; color: var(--textDim); cursor: pointer; font-size: 14px; {}",
                                        if cur_slots_len <= 2 { "opacity: 0.2; pointer-events: none;" } else { "" }
                                    );
                                    rsx! {
                                        div {
                                            key: "{idx}",
                                            style: "display: flex; align-items: center; gap: 8px; padding: 6px 8px; border-radius: 6px; border: 1px solid var(--border); background: var(--bg);",

                                            div {
                                                style: "width: 8px; height: 8px; border-radius: 50%; background: {dot_c}; flex-shrink: 0;",
                                            }

                                            select {
                                                style: "padding: 3px 6px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--text); font-size: 11px; outline: none;",
                                                value: "{slot_role_val}",
                                                onchange: move |e| {
                                                    let role = parse_agent_role(&e.value());
                                                    if let Some(s) = slots.write().get_mut(idx) {
                                                        s.role = role;
                                                    }
                                                },
                                                option { value: "coordinator", "Coordinator" }
                                                option { value: "builder", "Builder" }
                                                option { value: "scout", "Scout" }
                                                option { value: "reviewer", "Reviewer" }
                                            }

                                            select {
                                                style: "padding: 3px 6px; border-radius: 4px; border: 1px solid var(--border); background: var(--bgTertiary); color: var(--text); font-size: 11px; outline: none;",
                                                value: "{slot_agent_val}",
                                                onchange: move |e| {
                                                    let at = parse_agent_type(&e.value());
                                                    if let Some(s) = slots.write().get_mut(idx) {
                                                        s.agent_type = at;
                                                    }
                                                },
                                                option { value: "claude", "Claude Code" }
                                                option { value: "codex", "Codex" }
                                                option { value: "opencode", "OpenCode" }
                                                option { value: "gemini", "Gemini CLI" }
                                                option { value: "shell", "Shell" }
                                            }

                                            div { style: "flex: 1;" }

                                            button {
                                                style: "{remove_btn_style}",
                                                onclick: move |_| {
                                                    if slots.read().len() > 2 {
                                                        slots.write().remove(idx);
                                                    }
                                                },
                                                "\u{2013}"
                                            }
                                        }
                                    }
                                }
                            }

                            // Validation messages
                            if coordinator_count != 1 {
                                p { style: "font-size: 10px; color: var(--error); margin: 2px 0 0 0;", "Exactly 1 Coordinator required" }
                            }
                            if builder_count < 1 {
                                p { style: "font-size: 10px; color: var(--error); margin: 2px 0 0 0;", "At least 1 Builder required" }
                            }
                        }
                    }
                }
            }
        }
    }
}
