use crate::components::shared::icon::{IconMinus, IconPlus, IconSeal, IconSwarm, IconTerminal};
use crate::components::shared::modal::Modal;
use crate::stores::swarm::{use_swarm_store, SwarmAgent, SwarmAgentStatus, SwarmData};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{
    grid_for_pane_count, use_workspace_store, AgentType, PaneConfig, Space,
};
use crate::types::swarm::AgentRole;
use crate::utils::agent_commands::get_agent_color;
use dioxus::prelude::*;

#[path = "new_space_helpers.rs"]
mod new_space_helpers;
use new_space_helpers::{
    agent_role_str, apply_slot_value, generate_id, init_agent_rows, parse_agent_role, role_color,
    slot_value, AgentRowState, AgentSlot,
};

const TAB_COLORS: &[&str] = &[
    "#0ea5e9", "#22c55e", "#f59e0b", "#ef4444", "#06b6d4", "#f97316", "#64748b",
];

/// Represents a selectable agent row in the New Workspace modal.
/// For built-in agents, `custom_id` is `None`. For custom agents,
/// the `label`, `custom_id`, and `custom_cmd` fields carry the values
/// set by the user in Settings > Agents.
#[derive(Props, Clone, PartialEq)]
pub struct NewSpaceModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn NewSpaceModal(props: NewSpaceModalProps) -> Element {
    let mut step = use_signal(|| 0u8);
    let mut mode = use_signal(|| "".to_string());
    let mut space_name = use_signal(String::new);
    let mut space_dir = use_signal(String::new);
    let mut space_goal = use_signal(String::new);
    let launch_error = use_signal(|| None::<String>);

    // Move store access above signals that need it for init
    let mut workspace_state = use_workspace_store();
    let swarm_state = use_swarm_store();
    let mut ui_state = use_ui_store();

    // Terminal mode: per-row counts (built-in + custom agents)
    let _init_snapshot = ui_state.read().custom_agents.clone();
    let pane_agents: Signal<Vec<AgentRowState>> = use_signal(|| init_agent_rows(&_init_snapshot));

    // If launched from SwarmModal, pre-set swarm mode and carry over the goal.
    use_effect(move || {
        let pending = ui_state.read().pending_swarm_goal.clone();
        if let Some(goal) = pending {
            mode.set("swarm".to_string());
            space_goal.set(goal);
            step.set(1);
            ui_state.write().pending_swarm_goal = None;
        }
    });

    // Swarm mode: agent slots
    let mut slots: Signal<Vec<AgentSlot>> = use_signal(|| {
        vec![
            AgentSlot {
                role: AgentRole::Coordinator,
                agent_type: AgentType::Claude,
                custom_id: None,
                custom_cmd: None,
                label: None,
            },
            AgentSlot {
                role: AgentRole::Builder,
                agent_type: AgentType::Claude,
                custom_id: None,
                custom_cmd: None,
                label: None,
            },
            AgentSlot {
                role: AgentRole::Builder,
                agent_type: AgentType::Claude,
                custom_id: None,
                custom_cmd: None,
                label: None,
            },
        ]
    });

    // E2E helper: allow skipping validation by setting window.__athenaE2E = true
    let is_e2e: bool = web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &"__athenaE2E".into()).ok())
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let total_panes: usize = pane_agents.read().iter().map(|r| r.count).sum();
    // Snapshot the per-row agent state into an owned Vec BEFORE the rsx! tree.
    // Holding `pane_agents.read()` alive across IconMinus/IconPlus button children
    // re-borrows Dioxus's hook list during render → open panic
    // "The hook list is already borrowed" (BorrowMutError). Same idiom as
    // `total_panes` above: clone here so the RefCell borrow drops before rsx!.
    let pane_rows_snapshot: Vec<AgentRowState> = pane_agents.read().iter().cloned().collect();

    let coordinator_count = slots
        .read()
        .iter()
        .filter(|s| s.role == AgentRole::Coordinator)
        .count();
    // Snapshot the swarm slots (same reason as pane_rows_snapshot —
    // IconMinus/select children re-borrow the hook list if `slots.read()` is
    // held alive across the for body).
    let slots_snapshot: Vec<AgentSlot> = slots.read().iter().cloned().collect();
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
                style: "display: flex; align-items: center; justify-content: flex-end; gap: 8px; width: 100%;",

                button {
                    class: "btn-ghost",
                    onclick: move |_| {
                        // saturating_sub avoids a u8 underflow panic in debug
                        // builds if the render guard ever lets this fire at
                        // step 0 (the explicit step==1 branch is currently
                        // redundant with the saturating form, but kept
                        // defensive against future changes to the footer
                        // condition).
                        step.set(step().saturating_sub(1));
                    },
                        "Back"
                    }

                    if step() == 1 {
                        {
        let next_disabled = !is_e2e && (space_dir.read().trim().is_empty() || (is_swarm && space_goal.read().trim().is_empty()));
        rsx! {
            button {
                class: "btn-primary",
                disabled: next_disabled,
                onclick: move |_| {
                    let disabled = space_dir.read().trim().is_empty() || (mode() == "swarm" && space_goal.read().trim().is_empty());
                    if disabled && !is_e2e { return; }
                    step.set(2);
                },
                "Next >"
            }
        }
                            }
                        }

                    if step() == 2 && mode() == "terminal" {
                        {
        let launch_disabled = total_panes == 0;
                            rsx! {
                                button {
                                    class: "btn-primary",
                                    disabled: launch_disabled,
                                    onclick: move |_| {
                                        web_sys::console::log_1(&"[NewSpaceModal] Launch Space clicked (sync)".into());
                                        let dir = space_dir.read().trim().to_string();
                                        if dir.is_empty() && !is_e2e { return; }
                                        let dir = if dir.is_empty() { "/tmp".to_string() } else { dir };

                                        let now_ts = js_sys::Date::now() as i64;
                                        let space_count = workspace_state.read().spaces.len();
                                        let name = if space_name.read().is_empty() {
                                            format!("Space {}", space_count + 1)
                                        } else {
                                            space_name.read().clone()
                                        };

                                        let mut panes = Vec::new();
                                        for row in pane_agents.read().iter() {
                                            for _ in 0..row.count {
                                                panes.push(PaneConfig {
                                                    id: generate_id(),
                                                    agent_type: row.agent_type.clone(),
                                                    custom_cmd: row.custom_cmd.clone(),
                                                    custom_agent_id: row.custom_id.clone(),
                                                    // Default to None so the pane pill's
                                                    // fallback chain can resolve the live
                                                    // label (scraped title for agents,
                                                    // random name for idle shells). The
                                                    // modal row still shows "Shell"/"Codex"
                                                    // via AgentRowState.label at creation.
                                                    label: None,
                                                    bypass_mode: None,
                                                    project_name: None,
                                                    model_name: None,
                                                    resume_id: None,
                                                    resume_cmd: None,
                                                    resume_dismissed: None,
                                                });
                                            }
                                        }

                                        let grid = grid_for_pane_count(panes.len());
                                        // Capture the dir for trust authorization before it is
                                        // moved into the Space struct below.
                                        let trust_dir = dir.clone();
                                        let space = Space {
                                            id: generate_id(),
                                            name,
                                            dir,
                                            grid,
                                            panes,
                                            color: TAB_COLORS[space_count % TAB_COLORS.len()].to_string(),
                                            created_at: now_ts,
                                            last_opened_at: now_ts,
                                        };

                                        // Wrap in spawn to avoid synchronous signal writes
                                        // in onclick. Dioxus 0.7 has known wasm panics when
                                        // large closures write multiple signals.
                                        workspace_state.write().add_space(space);
                                        // Authorize the Space's directory so PTY spawns and
                                        // agent file tools aren't rejected by the sandbox.
                                        // Best-effort: a failure only means the backend will
                                        // reject the dir later with a clear "outside the
                                        // workspace" message, it doesn't break Space creation.
                                        spawn(async move {
                                            if let Err(e) =
                                                crate::tauri_bridge::workspace_add_trusted_root(
                                                    &trust_dir,
                                                )
                                                .await
                                            {
                                                web_sys::console::warn_1(
                                                    &format!(
                                                        "[NewSpaceModal] trust-on-launch failed for '{}': {:?}",
                                                        trust_dir, e
                                                    )
                                                    .into(),
                                                );
                                            }
                                        });
                                        props.on_close.call(());
                                    },
                                    "Launch Space"
                                }
                            }
                        }
                    }

                    if step() == 2 && mode() == "swarm" {
                        {
        let swarm_disabled = !can_launch_swarm;
                            rsx! {
                                button {
                                    class: "btn-primary",
                                    disabled: swarm_disabled,
                                    onclick: move |_| {
                                        let dir = space_dir.read().trim().to_string();
                                        let goal = space_goal.read().trim().to_string();
                                        if dir.is_empty() || goal.is_empty() { return; }

                                        let now_ts = js_sys::Date::now() as i64;
                                        let space_count = workspace_state.read().spaces.len();
                                        let name = if space_name.read().is_empty() {
                                            format!("Mission {}", space_count + 1)
                                        } else {
                                            space_name.read().clone()
                                        };

                                        let mut pane_configs = Vec::new();
                                        let mut swarm_agents = Vec::new();
                                        for slot in slots.read().iter() {
                                            let pane_id = format!("swarm-{}", generate_id());
                                            let agent_id = generate_id();
                                            pane_configs.push(PaneConfig {
                                                id: pane_id.clone(),
                                                agent_type: slot.agent_type.clone(),
                                                custom_cmd: slot.custom_cmd.clone(),
                                                custom_agent_id: slot.custom_id.clone(),
                                                label: slot.label.clone(),
                                                bypass_mode: None,
                                                project_name: None,
                                                model_name: None,
                                                resume_id: None,
                                                resume_cmd: None,
                                                resume_dismissed: None,
                                            });
                                            swarm_agents.push(SwarmAgent {
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

                                        let grid = grid_for_pane_count(pane_configs.len());
                                        // Capture the dir for trust authorization and backend
                                        // mission creation before it is moved into Space.
                                        let trust_dir = dir.clone();
                                        let swarm_dir = dir.clone();
                                        let space = Space {
                                            id: generate_id(),
                                            name,
                                            dir,
                                            grid,
                                            panes: pane_configs,
                                            color: TAB_COLORS[space_count % TAB_COLORS.len()].to_string(),
                                            created_at: now_ts,
                                            last_opened_at: now_ts,
                                        };

                                        let swarm_id = generate_id();
                                        let swarm_data = SwarmData {
                                            id: swarm_id,
                                            workspace_dir: swarm_dir.clone(),
                                            goal: goal.clone(),
                                            agents: swarm_agents,
                                            tasks: Vec::new(),
                                            messages: Vec::new(),
                                            status: crate::stores::swarm::SwarmOverallStatus::Active,
                                            started_at: now_ts,
                                            revision: 0,
                                        };
                                        let swarm_json = match serde_json::to_string(&swarm_data) {
                                            Ok(json) => json,
                                            Err(e) => {
                                                web_sys::console::error_1(&format!("[NewSpaceModal] swarm serialization failed: {e}").into());
                                                return;
                                            }
                                        };

                                        // Keep the new space out of the global stores until
                                        // authorization and mission persistence both succeed.
                                        // Otherwise a failed IPC call leaves a ghost mission that
                                        // the board cannot recover from disk.
                                        let mut launch_error = launch_error;
                                        let mut workspace_state = workspace_state;
                                        let mut swarm_state = swarm_state;
                                        let mut ui_state = ui_state;
                                        spawn(async move {
                                            if let Err(error) =
                                                crate::tauri_bridge::workspace_add_trusted_root(
                                                    &trust_dir,
                                                )
                                                .await
                                            {
                                                let message = format!(
                                                    "Could not authorize the workspace: {:?}",
                                                    error
                                                );
                                                web_sys::console::error_1(&message.clone().into());
                                                launch_error.set(Some(message));
                                                return;
                                            }
                                            match crate::tauri_bridge::swarm_create(&swarm_dir, &swarm_json).await {
                                                Ok(_) => {
                                                    workspace_state.write().add_space(space);
                                                    swarm_state.write().replace_swarm(swarm_data);
                                                    ui_state.write().panel = crate::stores::ui::Panel::Swarm;
                                                    props.on_close.call(());
                                                }
                                                Err(error) => {
                                                    let message = format!(
                                                        "Could not create the swarm mission: {:?}",
                                                        error
                                                    );
                                                    web_sys::console::error_1(&message.clone().into());
                                                    launch_error.set(Some(message));
                                                }
                                            }
                                        });
                                    },
                                    "Launch Swarm"
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
            compact: true,
            footer: Some(footer_el),

            // Step indicator dots — brand seal pendant at the leading edge.
            if step() > 0 {
                div {
                    style: "display: flex; align-items: center; gap: 8px; margin-bottom: 12px; padding-bottom: 12px; border-bottom: 1px solid var(--border);",

                    span {
                        class: "seal-mark",
                        IconSeal { size: Some(14), color: Some("var(--accent)".to_string()) }
                    }

                    for i in 1..=total_steps {
                        {
                            let dot_bg = if step() >= i { "var(--accent)" } else { "var(--bgTertiary)" };
                            rsx! {
                                div {
                                    key: "{i}",
                                    style: "width: 8px; height: 8px; border-radius: var(--radius-pill); background: {dot_bg}; transition: background var(--dur-fast) var(--ease);",
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
                        style: "font-size: var(--text-sm); color: var(--textMuted); margin: 0 0 4px 0;",
                        "Choose workspace type"
                    }

                    // Terminal Workspace card
                    button {
                        class: "card is-interactive",
                        style: if mode() == "terminal" {
                            "display: flex; align-items: center; gap: 16px; text-align: left; width: 100%; border-color: var(--accent); background: var(--accentSubtle); cursor: pointer;"
                        } else {
                            "display: flex; align-items: center; gap: 16px; text-align: left; width: 100%; cursor: pointer;"
                        },
                        onclick: move |_| {
                            mode.set("terminal".to_string());
                            step.set(1);
                        },

                        div {
                            style: "width: 40px; height: 40px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; border-radius: var(--radius-md); background: var(--bgTertiary);",
                            IconTerminal { size: Some(20), color: Some("var(--accent)".to_string()) }
                        }

                        div {
                            div { style: "font-size: var(--text-sm); font-weight: 600; color: var(--text);", "Terminal Workspace" }
                            span { style: "font-size: var(--text-xs); color: var(--textDim); display: block; margin-top: 2px;", "Launch multiple terminal panes with AI agents in a grid layout" }
                        }
                    }

                    // Swarm Mission card
                    button {
                        class: "card is-interactive",
                        style: if mode() == "swarm" {
                            "display: flex; align-items: center; gap: 16px; text-align: left; width: 100%; border-color: var(--accent); background: var(--accentSubtle); cursor: pointer;"
                        } else {
                            "display: flex; align-items: center; gap: 16px; text-align: left; width: 100%; cursor: pointer;"
                        },
                        onclick: move |_| {
                            mode.set("swarm".to_string());
                            step.set(1);
                        },

                        div {
                            style: "width: 40px; height: 40px; display: flex; align-items: center; justify-content: center; flex-shrink: 0; border-radius: var(--radius-md); background: var(--bgTertiary);",
                            IconSwarm { size: Some(20), color: Some("var(--accent)".to_string()) }
                        }

                        div {
                            div { style: "font-size: var(--text-sm); font-weight: 600; color: var(--text);", "Swarm Mission" }
                            span { style: "font-size: var(--text-xs); color: var(--textDim); display: block; margin-top: 2px;", "Orchestrate a team of AI agents on a shared goal" }
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
                        style: "font-size: var(--text-xs); color: var(--textMuted); font-weight: 500;",
                        "Space Name"
                        input {
                            class: "field",
                            style: "width: 100%; margin-top: 4px; box-sizing: border-box;",
                            value: "{space_name}",
                            oninput: move |e| space_name.set(e.value()),
                            placeholder: "my-project"
                        }
                    }

                    // Working directory
                    div {
                        style: "display: flex; flex-direction: column; gap: 4px;",
                        label {
                            style: "font-size: var(--text-xs); color: var(--textMuted); font-weight: 500;",
                            "Working Directory"
                        }
                        div {
                            style: "display: flex; gap: 6px; margin-top: 4px;",
                input {
                    class: "field",
                    style: "flex: 1; box-sizing: border-box;",
                    value: "{space_dir}",
                    oninput: move |e| space_dir.set(e.value()),
                    onchange: move |e| space_dir.set(e.value()),
                    placeholder: "/path/to/project"
                }
                            button {
                                class: "btn-secondary",
                                style: "flex-shrink: 0;",
                                onclick: move |e| {
                                    e.stop_propagation();
                                    web_sys::console::log_1(&"[Browse] clicked".into());
                                    let mut space_dir = space_dir;
                                    spawn(async move {
                                        web_sys::console::log_1(&"[Browse] invoking dialog...".into());
                                        match crate::tauri_bridge::fs_show_open_dialog(Some("Select Workspace Directory"), true, false).await {
                                            Ok(path) => {
                                                web_sys::console::log_1(&format!("[Browse] path: {:?}", path).into());
                                                if !path.is_empty() {
                                                    space_dir.set(path);
                                                }
                                            }
                                            Err(e) => {
                                                web_sys::console::error_1(&format!("[Browse] error: {:?}", e).into());
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
                            style: "font-size: var(--text-xs); color: var(--textMuted); font-weight: 500;",
                            "Goal"
                            textarea {
                                class: "field",
                                style: "width: 100%; margin-top: 4px; resize: vertical; min-height: 60px; box-sizing: border-box;",
                                value: "{space_goal}",
                                oninput: move |e| space_goal.set(e.value()),
                                placeholder: "Describe what the swarm should accomplish..."
                            }
                        }
                    }
                }
            }

            // Step 2: Agents (terminal mode)
            if step() == 2 && mode() == "terminal" {
                div {
                    style: "display: flex; flex-direction: column; gap: 14px;",

                    // Agent configuration
                    div {
                        style: "padding-top: 4px;",

                        div {
                            style: "display: flex; align-items: center; justify-content: space-between; margin-bottom: 8px;",
                            label {
                                style: "font-size: var(--text-xs); color: var(--textMuted); font-weight: 500;",
                                "Agents ({total_panes}/16)"
                            }
                        }

                        for (idx, row) in pane_rows_snapshot.iter().enumerate() {
                            {
                                let count_val = row.count;
                                let has_any = count_val > 0;
                                let color = get_agent_color(&row.agent_type);
                                let row_bg = if has_any {
                                    "var(--bgSecondary)"
                                } else {
                                    "transparent"
                                };
                                let row_border = "1px solid var(--border)";
                                let dot_bg = if has_any { color } else { "var(--textDim)" };
                                let text_color = if has_any { "var(--text)" } else { "var(--textMuted)" };
                                let minus_disabled_class = if count_val == 0 { "btn-disabled" } else { "" };
                                let plus_disabled_class = if total_panes >= 16 { "btn-disabled" } else { "" };
                                let plus_testid = format!("add-{}", row.label.to_lowercase().replace(' ', "-"));

                                rsx! {
                                    div {
                                        key: "{idx}",
                                        style: "display: flex; align-items: center; justify-content: space-between; padding: 8px 10px; border: 1px solid {row_border}; border-radius: var(--radius-md); background: {row_bg}; margin-bottom: 4px;",

                                        div {
                                            style: "display: flex; align-items: center; gap: 8px;",
                                            div {
                                                style: "width: 8px; height: 8px; border-radius: 50%; background: {dot_bg}; flex-shrink: 0;",
                                            }
                                            span {
                                                style: "font-size: var(--text-sm); font-weight: 500; color: {text_color};",
                                                "{row.label}"
                                            }
                                        }

                                        div {
                                            style: "display: flex; align-items: center; gap: 8px;",

                                            button {
                                                class: "icon-btn {minus_disabled_class}",
                                                "aria-label": "Remove agent",
                                                onclick: move |_: dioxus::events::MouseEvent| {
                                                    web_sys::console::log_1(&"[NewSpaceModal] - clicked".into());
                                                    let mut paned = pane_agents;
                                                    let mut agents: Vec<AgentRowState> = paned.read().iter().cloned().collect();
                                                    if let Some(r) = agents.get_mut(idx) {
                                                        if r.count > 0 {
                                                            web_sys::console::log_1(&"[NewSpaceModal] decrementing".into());
                                                            r.count -= 1;
                                                        } else {
                                                            web_sys::console::log_1(&"[NewSpaceModal] count already 0, cannot decrement".into());
                                                        }
                                                        paned.set(agents);
                                                        let total: usize = paned.read().iter().map(|ag| ag.count).sum();
                                                        web_sys::console::log_1(&format!("[NewSpaceModal] total panes after decrement: {}", total).into());
                                                    }
                                                    web_sys::console::log_1(&"[NewSpaceModal] - done".into());
                                                },
                                                IconMinus { size: Some(14), color: Some("currentColor".to_string()) }
                                            }

                                            span {
                                                style: "font-size: var(--text-sm); font-weight: 500; color: var(--text); width: 12px; text-align: center; font-variant-numeric: tabular-nums;",
                                                "{count_val}"
                                            }

                                            button {
                                                class: "icon-btn {plus_disabled_class}",
                                                id: "{plus_testid}",
                                                "aria-label": "Add agent",
                                                onclick: move |_: dioxus::events::MouseEvent| {
                                                    web_sys::console::log_1(&"[NewSpaceModal] + clicked".into());
                                                    let mut paned = pane_agents;
                                                    if let Some(win) = web_sys::window() {
                                                        let _ = js_sys::Reflect::set(&win, &"__athenaClickFired".into(), &true.into());
                                                    }
                                                    web_sys::console::log_1(&"[NewSpaceModal] about to write pane_agents".into());
                                                    let mut agents: Vec<AgentRowState> = paned.read().iter().cloned().collect();
                                                    let total: usize = agents.iter().map(|ag| ag.count).sum();
                                                    if let Some(r) = agents.get_mut(idx) {
                                                        web_sys::console::log_1(&"[NewSpaceModal] incrementing".into());
                                                        if total < 16 {
                                                            r.count += 1;
                                                        }
                                                        paned.set(agents);
                                                    }
                                                    let total: usize = paned.read().iter().map(|ag| ag.count).sum();
                                                    web_sys::console::log_1(&format!("[NewSpaceModal] total panes: {}", total).into());
                                                    web_sys::console::log_1(&"[NewSpaceModal] + done".into());
                                                    if let Some(win) = web_sys::window() {
                                                        let _ = js_sys::Reflect::set(&win, &"__athenaClickDone".into(), &true.into());
                                                    }
                                                },
                                                IconPlus { size: Some(14), color: Some("currentColor".to_string()) }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        }

                    // Summary
                    div {
                        style: "padding: 10px 12px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary); margin-top: 4px;",
                        div { style: "font-size: var(--text-2xs); color: var(--accent); margin-bottom: 4px; text-transform: uppercase; letter-spacing: 0.05em;", "Summary" }
                        div {
                            style: "font-size: var(--text-sm); color: var(--text);",
                            {space_name.read().clone()}
                        }
                        div {
                            style: "font-size: var(--text-2xs); color: var(--textDim);",
                            {space_dir.read().clone()}
                        }
                    }
                }
            }

            // Step 2: Swarm team config
            if step() == 2 && mode() == "swarm" {
                {
                    let slots_count = slots.read().len();
                    let add_disabled_class = if slots_count >= 10 { "btn-disabled" } else { "" };
                    let team_label = format!("Team ({} agents)", slots_count);
                    rsx! {
                        div {
                            style: "display: flex; flex-direction: column; gap: 10px;",

                            div {
                                style: "display: flex; align-items: center; justify-content: space-between;",
                                label {
                                    style: "font-size: var(--text-xs); color: var(--textMuted); font-weight: 500;",
                                    "{team_label}"
                                }
                                button {
                                    class: "btn-secondary btn-sm {add_disabled_class}",
                                    onclick: move |_| {
                                        if slots.read().len() < 10 {
                                            slots.write().push(AgentSlot { role: AgentRole::Builder, agent_type: AgentType::Claude, custom_id: None, custom_cmd: None, label: None });
                                        }
                                    },
                                    "+ Add"
                                }
                            }

                            for (idx, slot) in slots_snapshot.iter().enumerate() {
                                {
                                    let slot_role_val = agent_role_str(&slot.role);
                                    let slot_agent_val = slot_value(slot);
                                    let dot_c = role_color(&slot.role);
                                    let cur_slots_len = slots.read().len();
                                    let remove_disabled_class = if cur_slots_len <= 2 { "btn-disabled" } else { "" };
                                    rsx! {
                                        div {
                                            key: "{idx}",
                                            style: "display: flex; align-items: center; gap: 8px; padding: 8px 10px; border: 1px solid var(--border); border-radius: var(--radius-md); background: var(--bgSecondary);",

                                            div {
                                                style: "width: 8px; height: 8px; border-radius: var(--radius-pill); background: {dot_c}; flex-shrink: 0;",
                                            }

                                            select {
                                                style: "padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgTertiary); color: var(--text); font-size: var(--text-xs); outline: none;",
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
                                                style: "padding: 5px 8px; border: 1px solid var(--border); border-radius: var(--radius-sm); background: var(--bgTertiary); color: var(--text); font-size: var(--text-xs); outline: none;",
                                                value: "{slot_agent_val}",
                                                onchange: move |e| {
                                                    let ca_list = {
                                                        let ui = ui_state.read();
                                                        ui.custom_agents.clone()
                                                    };
                                                    if let Some(s) = slots.write().get_mut(idx) {
                                                        apply_slot_value(s, &e.value(), &ca_list);
                                                    }
                                                },
                                                option { value: "claude", "Claude Code" }
                                                option { value: "codex", "Codex" }
                                                option { value: "opencode", "OpenCode" }
                                                option { value: "gemini", "Gemini CLI" }
                                                option { value: "shell", "Shell" }
                                                for ca in ui_state.read().custom_agents.clone() {
                                                    option { value: "custom${ca.id}", "{ca.alias}" }
                                                }
                                            }

                                            div { style: "flex: 1;" }

                                            button {
                                                class: "icon-btn {remove_disabled_class}",
                                                "aria-label": "Remove agent",
                                                onclick: move |_| {
                                                    if slots.read().len() > 2 {
                                                        slots.write().remove(idx);
                                                    }
                                                },
                                                IconMinus { size: Some(14), color: Some("currentColor".to_string()) }
                                            }
                                        }
                                    }
                                }
                            }

                            if let Some(error) = launch_error.read().as_ref() {
                                p { style: "font-size: var(--text-xs); color: var(--error); margin: 2px 0 0 0;", "{error}" }
                            }

                            // Validation messages
                            if coordinator_count != 1 {
                                p { style: "font-size: var(--text-xs); color: var(--error); margin: 2px 0 0 0;", "Exactly 1 Coordinator required" }
                            }
                            if builder_count < 1 {
                                p { style: "font-size: var(--text-xs); color: var(--error); margin: 2px 0 0 0;", "At least 1 Builder required" }
                            }
                        }
                    }
                }
            }
        }
    }
}
