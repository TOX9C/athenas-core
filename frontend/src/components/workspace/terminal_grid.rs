use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::shared::icon::{
    IconCheck, IconClose, IconCopy, IconFullscreen, IconMinimize,
};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use crate::stores::athena::use_athena_store;
use crate::stores::panel_manager::{use_panel_manager_store, RightPanel};
use crate::stores::terminal::{use_terminal_registry, use_terminal_store};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{use_workspace_store, AgentType, Space};
use crate::tauri_bridge::{pty_agent_info, pty_kill, pty_write};
use crate::types::workspace::CustomAgent;
use crate::utils::agent_commands::{
    agent_process_name, claude_resume_variants, custom_agent_process_name, get_agent_color,
    get_agent_label, get_agent_resume_command,
};

#[path = "terminal_cells.rs"]
mod terminal_cells;
use terminal_cells::TerminalPaneBody;

#[path = "terminal_resize.rs"]
mod terminal_resize;
use terminal_resize::{ColDivider, DragInfo, DragOverlay, RowDivider};

#[cfg(feature = "xterm")]
use crate::components::workspace::xterm_mount::XtermMount;

#[cfg(feature = "xterm")]
fn render_shell_pane(
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    resume_id: Option<String>,
    custom_cmd: Option<String>,
    bypass_mode: Option<bool>,
) -> Element {
    rsx! { XtermMount { key: "xterm-{pane_id}", pane_id, cwd, agent_type, resume_id, custom_cmd, bypass_mode } }
}

#[cfg(not(feature = "xterm"))]
fn render_shell_pane(pane_id: String, _cwd: String) -> Element {
    rsx! { TerminalPaneBody { key: "terminal-body-{pane_id}", pane_id } }
}

// ---------------------------------------------------------------------------
// Props
// ---------------------------------------------------------------------------

/// Props for the workspace grid.
#[derive(Props, Clone, PartialEq)]
pub struct WorkspaceGridProps {
    pub active_space: Option<Space>,
    pub active_space_id: Option<String>,
}

// ---------------------------------------------------------------------------
// WorkspaceGrid
// ---------------------------------------------------------------------------

#[component]
pub fn WorkspaceGrid(props: WorkspaceGridProps) -> Element {
    crate::utils::perf_metrics::mark_render("WorkspaceGrid");
    let space = match props.active_space {
        Some(s) => s,
        None => return rsx! { div {} },
    };

    let pane_count = space.panes.len();
    let (cols, _rows) = match space.grid {
        crate::types::workspace::GridTemplate::X1x1 => (1usize, 1usize),
        crate::types::workspace::GridTemplate::X1x2 => (2, 1),
        crate::types::workspace::GridTemplate::X2x2 => (2, 2),
        crate::types::workspace::GridTemplate::X2x3 => (3, 2),
        crate::types::workspace::GridTemplate::X3x3 => (3, 3),
        crate::types::workspace::GridTemplate::X3x4 => (4, 3),
        crate::types::workspace::GridTemplate::X4x4 => (4, 4),
    };

    if pane_count == 0 {
        return rsx! {
            EmptyState {
                kind: EmptyArt::Workspace,
                title: "Empty workspace".to_string(),
                hint: Some("Add a shell or agent to begin.".to_string()),
            }
        };
    }

    let actual_row_count = pane_count.div_ceil(cols);

    // Per-row column flex-grow values. Each row keeps its own width state, so
    // resizing the top row does not force the bottom row to match it.
    let mut col_widths = use_signal(|| {
        (0..actual_row_count)
            .map(|row_idx| {
                let start = row_idx * cols;
                let row_len = (pane_count.saturating_sub(start)).min(cols).max(1);
                vec![1.0_f64; row_len]
            })
            .collect::<Vec<_>>()
    });

    // Row heights remain row-scoped across the whole grid.
    let mut row_heights = use_signal(|| vec![1.0_f64; actual_row_count.max(1)]);
    let drag = use_signal(|| None::<DragInfo>);
    let fullscreen_pane_id = use_signal(|| None::<String>);
    // Pane-pill drag-and-drop: live drag-session state. `None` when idle.
    // The fullscreen `PillDragOverlay` (mounted below) owns pointermove/up for
    // the duration of a drag, mirroring the resize `DragOverlay` pattern.
    let pill_drag = use_signal(|| None::<crate::components::workspace::pill_drag::PillDrag>);
    let terminal_store = use_terminal_store();
    let athena_store = use_athena_store();
    let panel_store = use_panel_manager_store();
    let ui_store = use_ui_store();
    let active_pane_id = terminal_store.read().active_session_id.clone();
    // Note: active pane selection is stored in TerminalStore (single source of truth).
    // The clicked pane gets a subtle gold focus ring (see `.pane-focus-ring`).

    // Subscribe to workspace changes so this effect re-runs when panes are
    // added or removed, ensuring col_widths shape stays in sync.
    let workspace = use_workspace_store();

    use_effect(move || {
        // Reactive read causes re-run on workspace mutations (add/remove panes).
        let _ = workspace.read().spaces.len();

        let target_width_shape: Vec<usize> = (0..actual_row_count)
            .map(|row_idx| {
                let start = row_idx * cols;
                (pane_count.saturating_sub(start)).min(cols).max(1)
            })
            .collect();
        let current_width_shape: Vec<usize> =
            col_widths.read().iter().map(|row| row.len()).collect();
        if current_width_shape != target_width_shape {
            col_widths.set(
                target_width_shape
                    .into_iter()
                    .map(|len| vec![1.0_f64; len])
                    .collect(),
            );
        }

        let target_row_count = actual_row_count.max(1);
        if row_heights.read().len() != target_row_count {
            row_heights.set(vec![1.0_f64; target_row_count]);
        }
    });

    let show_athena_fallback = pill_drag
        .read()
        .as_ref()
        .is_some_and(|drag| drag.source_is_agent)
        && (!ui_store.read().right_sidebar_open
            || panel_store.read().active_right_panel != RightPanel::Assistant);

    rsx! {
        div {
            class: "workspace-grid-root",
            "data-space-id": "{space.id}",
            style: "flex: 1; display: flex; flex-direction: column; gap: 0; padding: 0; overflow: hidden; background: var(--bg); min-height: 0; min-width: 0; position: relative;",

            for row_idx in 0..actual_row_count {
                {
                    let start_pane = row_idx * cols;
                    let end_pane = ((row_idx + 1) * cols).min(pane_count);
                    let row_panes = &space.panes[start_pane..end_pane];
                    let row_weight = row_heights.read().get(row_idx).copied().unwrap_or(1.0);

                    rsx! {
                        div {
                            key: "row-{space.id}-{row_idx}",
                            style: "display: flex; flex-direction: row; flex: {row_weight}; min-height: 0; min-width: 0; gap: 0;",

                            for (rel_idx, pane) in row_panes.iter().enumerate() {
                                {
                                    let flex_weight = {
                                        let cw = col_widths.read();
                                        if let Some(row) = cw.get(row_idx) {
                                            *row.get(rel_idx).unwrap_or(&1.0)
                                        } else {
                                            1.0
                                        }
                                    };
                                    let has_right = rel_idx + 1 < row_panes.len();
                                    let has_bottom = row_idx + 1 < actual_row_count;
                                    let is_active = active_pane_id.as_deref() == Some(pane.id.as_str());
                                    let is_fullscreenmode = fullscreen_pane_id.read().as_deref() == Some(pane.id.as_str());

                                    let wrapper_style = if is_fullscreenmode {
                                        "position: absolute; top: 0; left: 0; width: 100%; height: 100%; z-index: 40; background: var(--bg); box-sizing: border-box;".to_string()
                                    } else if fullscreen_pane_id.read().is_some() {
                                        "display: none;".to_string()
                                    } else {
                                        let mut s = format!(
                                            "position: relative; flex: {}; min-height: 0; min-width: 0; padding: 0; display: flex; flex-direction: column; box-sizing: border-box;",
                                            flex_weight
                                        );
                                        if has_right {
                                            s.push_str(" border-right: 1px solid var(--border);");
                                        }
                                        if has_bottom {
                                            s.push_str(" border-bottom: 1px solid var(--border);");
                                        }
                                        s
                                    };

                                    // DnD target highlight: gold ring while this pane is
                                    // the hit-tested drop target of an in-flight pill drag.
                                    let is_dnd_target = pill_drag
                                        .read()
                                        .as_ref()
                                        .and_then(|d| d.target.as_ref())
                                        .is_some_and(|target| {
                                            matches!(target, crate::components::workspace::pill_drag::PillDropTarget::Pane(id) if id == &pane.id)
                                        });
                                    let wrapper_class = if is_dnd_target {
                                        "pane-wrap is-dnd-target"
                                    } else {
                                        "pane-wrap"
                                    };

                                    rsx! {
                                        div {
                                            key: "pane-wrap-{space.id}-{pane.id}",
                                            class: "{wrapper_class} pane-astrolabe-mark",
                                            "data-pane-id": "{pane.id}",
                                            style: "{wrapper_style}",

                                            if is_active && !is_fullscreenmode {
                                                div { class: "pane-focus-ring" }
                                            }

                                            PaneItem {
                                                key: "pane-{space.id}-{pane.id}",
                                                space_id: space.id.clone(),
                                                pane_id: pane.id.clone(),
                                                cwd: space.dir.clone(),
                                                agent_type: pane.agent_type.clone(),
                                                // Every PTY-backed pane uses xterm.js. The legacy cell
                                                // grid has no keyboard input surface, so restricting this
                                                // to Shell/Custom makes interactive agents such as OMP
                                                // render output while silently dropping all keystrokes.
                                                use_xterm: true,
                                                resume_id: pane.resume_id.clone(),
                                                resume_cmd: pane.resume_cmd.clone(),
                                                resume_dismissed: pane.resume_dismissed,
                                                custom_cmd: pane.custom_cmd.clone(),
                                                custom_agent_id: pane.custom_agent_id.clone(),
                                                bypass_mode: pane.bypass_mode,
                                                label: pane.label.clone(),
                                                fullscreen_pane_id: fullscreen_pane_id,
                                                pill_drag: pill_drag,
                                            }
                                        }

                                        if rel_idx + 1 < row_panes.len() {
                                            ColDivider {
                                                key: "col-div-{row_idx}-{rel_idx}",
                                                space_id: space.id.clone(),
                                                row_index: row_idx,
                                                index: rel_idx,
                                                col_widths: col_widths,
                                                drag: drag,
                                            }
                                        }
                                    }
                                }
                            }
                        }

                        if row_idx + 1 < actual_row_count {
                            RowDivider {
                                key: "row-div-{row_idx}",
                                space_id: space.id.clone(),
                                index: row_idx,
                                row_heights: row_heights,
                                drag: drag,
                            }
                        }
                    }
                }
            }
        }

        if drag.cloned().is_some() {
            DragOverlay {
                drag: drag,
                col_widths: col_widths,
                row_heights: row_heights,
            }
        }

        // Pill drag-and-drop. The fullscreen overlay mounts only while a drag
        // is in flight (so it doesn't intercept normal pointer events when
        // idle); the ghost renders nothing when idle, so mounting it
        // unconditionally avoids a key-flip and keeps it outside the grid's
        // flex layout.
        if pill_drag.read().is_some() {
            if show_athena_fallback {
                div {
                    class: "athena-dnd-fallback",
                    "data-athena-drop": "true",
                    aria_label: "Drop agent here to reference it in Athena",
                    span { class: "athena-dnd-fallback-kicker", "ATHENA" }
                    span { "Drop to reference this agent" }
                }
            }
            crate::components::workspace::pill_drag::PillDragOverlay {
                drag: pill_drag,
                workspace: workspace,
                terminal_store: terminal_store,
                athena_store: athena_store,
                panel_store: panel_store,
                ui_store: ui_store,
            }
        }
        crate::components::workspace::pill_drag::PillDragGhost {
            drag: pill_drag,
        }
    }
}

// ---------------------------------------------------------------------------
// PaneItem
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct PaneItemProps {
    space_id: String,
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    /// Whether this PTY-backed pane should use xterm.js for rendering and input.
    /// All desktop PTY panes currently require this; the prop remains explicit
    /// so a future non-PTY pane cannot accidentally enter the xterm lifecycle.
    use_xterm: bool,
    resume_id: Option<String>,
    resume_cmd: Option<String>,
    resume_dismissed: Option<bool>,
    custom_cmd: Option<String>,
    custom_agent_id: Option<String>,
    bypass_mode: Option<bool>,
    label: Option<String>,
    fullscreen_pane_id: Signal<Option<String>>,
    pill_drag: Signal<Option<crate::components::workspace::pill_drag::PillDrag>>,
}

#[component]
fn PaneItem(props: PaneItemProps) -> Element {
    crate::utils::perf_metrics::mark_render("PaneItem");
    let mut workspace = use_workspace_store();
    let mut terminal_store = use_terminal_store();
    let ui_state = use_ui_store();
    // Per-session terminal registry — captured once, synchronously, at render
    // top. The lookup `registry.session_signal(&pane_id)` is a plain method
    // (not a hook), so it may run inside the `use_memo` closure below without
    // re-entering the hook list (which `use_session_signal` would, since it
    // calls `use_context` — a hook — and "hook inside hook" panics Dioxus at
    // mount with "hook list already borrowed"). Fix for the merge #6 panic.
    let terminal_registry = use_terminal_registry();
    let mut fullscreen_pane_id = props.fullscreen_pane_id;
    let mut pill_drag = props.pill_drag;

    let pane_id_for_close = props.pane_id.clone();
    let space_id_for_close = props.space_id.clone();
    let agent_label = get_agent_label(&props.agent_type);
    let _agent_color = get_agent_color(&props.agent_type);
    let _display_id: String = props.pane_id.chars().take(10).collect();
    let is_fullscreen = fullscreen_pane_id.read().as_deref() == Some(&props.pane_id);
    let pane_id_for_fullscreen = props.pane_id.clone();

    // Drag-source locals for the pill drag-and-drop swap (clone once so the
    // grab-surface `onpointerdown` closure can move them without re-borrowing
    // `props` each render).
    let drag_pane_id = props.pane_id.clone();
    let drag_space_id = props.space_id.clone();
    let drag_agent_type = props.agent_type.clone();
    let drag_pane_id_for_start = drag_pane_id.clone();
    let drag_agent_type_for_start = drag_agent_type.clone();

    // Editable title state
    let mut editing_title = use_signal(|| false);
    let mut temp_title = use_signal(String::new);

    // Read current foreground process and title state for THIS pane from its
    // per-session inner signal (Item 3 decomposition). Subscribing to the inner
    // signal means a foreground/title change in pane A doesn't re-evaluate
    // pane B's memo. Falls back to defaults if the pane isn't registered.
    //
    // Uses `terminal_registry.session_signal(...)` (a plain method) instead of
    // `use_session_signal(...)` (a hook) so this memo doesn't re-enter the hook
    // list mid-closure — calling a hook inside another hook's compute closure
    // panics Dioxus at mount with "hook list already borrowed".
    let pane_id_for_pill = props.pane_id.clone();
    // Clone the registry into the memo's closure so the render-top binding
    // `terminal_registry` is NOT moved into it (it's reused by the close-pane
    // `onclick` handler further down). `TerminalRegistry` is a cheap `Rc`-bump
    // clone, and `use_memo` only re-evaluates on signal change, so this is
    // not per-render churn.
    let registry_for_memo = terminal_registry.clone();
    let (fg_process, title_state) = use_memo(move || {
        registry_for_memo
            .session_signal(&pane_id_for_pill)
            .and_then(|s| {
                s.try_read()
                    .ok()
                    .map(|r| (r.foreground_process.clone(), r.title_state.clone()))
            })
            .unwrap_or_else(|| (None, crate::utils::pane_label::TitleState::default()))
    })();

    // ── Resume banner state ──────────────────────────────────────────────
    // If the pane has a captured resume command (from PTY output for Shell
    // panes, or from resume_id + agent type for agent panes), show a banner
    // so the user can choose to resume the session. The banner auto-hides
    // while the agent is running (only for detectable agent types).
    let display_resume_cmd = props.resume_cmd.as_ref().cloned().or_else(|| {
        props
            .resume_id
            .as_deref()
            .and_then(|id| get_agent_resume_command(&props.agent_type, id))
    });

    // Resolve the custom agent config for a Custom pane — used to decide
    // running-detection and whether this pane is a Claude alias.
    let custom_agent: Option<CustomAgent> = if props.agent_type == AgentType::Custom {
        if let Some(cid) = props.custom_agent_id.as_deref() {
            ui_state
                .read()
                .custom_agents
                .iter()
                .find(|a| a.id == cid)
                .cloned()
        } else {
            None
        }
    } else {
        None
    };

    // Running-detection process name. Built-in agent types map to their binary;
    // a Custom pane maps to "claude" only when the agent is marked `is_claude`
    // (same binary, different flags), so its panes get running-detection too.
    let known_process = match &props.agent_type {
        AgentType::Custom => custom_agent
            .as_ref()
            .and_then(|a| custom_agent_process_name(a.is_claude)),
        other => agent_process_name(other),
    };
    let has_detectable_agent = known_process.is_some();

    // Build the resume variant list. A Claude session — a Claude pane, a
    // Custom is-claude pane, or a Shell pane where the user ran `claude`
    // manually — offers plain `claude --resume <id>` plus each is-claude
    // alias's variant (its flags preserved, `--resume <id>` appended).
    // Non-Claude sessions keep the single captured/synthesized command.
    let is_claude_session = match &props.agent_type {
        AgentType::Claude => true,
        AgentType::Custom => custom_agent.as_ref().map(|a| a.is_claude).unwrap_or(false),
        AgentType::Shell => display_resume_cmd
            .as_deref()
            .map(|c| c.starts_with("claude"))
            .unwrap_or(false),
        _ => false,
    };
    let claude_aliases: Vec<CustomAgent> = ui_state
        .read()
        .custom_agents
        .iter()
        .filter(|a| a.is_claude)
        .cloned()
        .collect();
    let resume_variants: Vec<String> = if is_claude_session {
        if let Some(id) = props.resume_id.as_deref() {
            claude_resume_variants(id, &claude_aliases)
        } else {
            Vec::new()
        }
    } else {
        display_resume_cmd.iter().cloned().collect()
    };
    let mut selected_variant = use_signal(|| 0usize);
    let mut agent_running = use_signal(|| false);
    // Initialize the dismissed flag from the persisted pane state so a
    // banner the user dismissed survives an app restart. Re-seeding on each
    // mount is fine: a new session capture resets `resume_dismissed` to
    // `Some(false)` on the pane (see xterm_mount.rs), so this reads false and
    // the banner reappears for the new session.
    let persisted_dismissed = props.resume_dismissed.unwrap_or(false);
    let mut banner_dismissed = use_signal(|| persisted_dismissed);
    let mut copied = use_signal(|| false);

    // Clamp selected_variant if the variant list shrank (e.g. an alias was
    // removed from settings while the banner was open) so the index stays in
    // range and Resume always points at a valid command.
    {
        let variant_count = resume_variants.len();
        use_effect(move || {
            if *selected_variant.read() >= variant_count && variant_count > 0 {
                selected_variant.set(variant_count.saturating_sub(1));
            }
        });
    }

    // Running-detection only for agent panes (Claude, Codex, etc.) since
    // Shell panes started manually don't have a reliable running signal.
    {
        let poll_pane_id = props.pane_id.clone();
        let want_process = known_process.map(|s| s.to_string());
        let has_resume = !resume_variants.is_empty() || display_resume_cmd.is_some();
        use_future(move || {
            let poll_pane_id = poll_pane_id.clone();
            let want_process = want_process.clone();
            async move {
                let Some(want) = want_process else {
                    return;
                };
                if !has_resume {
                    return;
                }
                loop {
                    if let Ok(info) = pty_agent_info(&poll_pane_id).await {
                        let running = info.foreground_process == want;
                        if agent_running() != running {
                            agent_running.set(running);
                        }
                    }
                    // Shorter interval than the general status poll: once the
                    // agent exits we want the resume banner to reappear quickly
                    // (the scanner already captured the id the instant it was
                    // printed; this only gates the "not running" reveal).
                    gloo::timers::future::TimeoutFuture::new(2000).await;
                }
            }
        });
    }

    // Banner shown when: a resume command is available (either a single
    // captured/synthesized command, or at least one Claude variant), not
    // dismissed, and either there's no running-detection or the agent is not
    // currently detected running.
    let show_resume_banner = (!resume_variants.is_empty() || display_resume_cmd.is_some())
        && !banner_dismissed()
        && (!has_detectable_agent || !agent_running());

    // Diagnostic breadcrumb for resume regressions. It records only pane
    // metadata and lengths, never terminal output or the full session ID.
    {
        let pane_id = props.pane_id.clone();
        let agent = props.agent_type.to_string();
        let resume_id_len = props.resume_id.as_deref().map(str::len).unwrap_or(0);
        let has_resume_cmd = props.resume_cmd.is_some();
        let variant_count = resume_variants.len();
        let detectable_process = known_process.map(str::to_string);
        use_effect(move || {
            let running = agent_running();
            let dismissed = banner_dismissed();
            web_sys::console::log_1(
                &format!(
                    "[resume-debug] pane={} agent={} id_len={} cmd_present={} variants={} process={:?} running={} dismissed={} show={}",
                    pane_id,
                    agent,
                    resume_id_len,
                    has_resume_cmd,
                    variant_count,
                    detectable_process,
                    running,
                    dismissed,
                    show_resume_banner
                )
                .into(),
            );
        });
    }

    let left_label = crate::utils::pane_label::resolve_pane_label(
        props.label.as_deref(),
        &title_state,
        &props.agent_type,
        fg_process.as_deref(),
        ui_state.read().smart_pane_titles,
        agent_label,
        &props.pane_id,
    );

    // View-only truncation. The store keeps the full title; the pill shows
    // up to ~24 chars with an ellipsis, full text on hover.
    const LABEL_MAX_CHARS: usize = 24;
    let (display_label, tooltip) = if left_label.chars().count() <= LABEL_MAX_CHARS {
        (left_label.clone(), None)
    } else {
        let truncated: String = left_label.chars().take(LABEL_MAX_CHARS).collect();
        (format!("{}…", truncated), Some(left_label.clone()))
    };
    let title_text = tooltip.unwrap_or_else(|| display_label.clone());
    // Show the detected foreground process as a subtle badge when it's meaningful
    let right_badge = fg_process.filter(|p| p != "shell" && p != &left_label && !p.is_empty());
    let drag_source_label = left_label.clone();
    let drag_source_label_for_attr = drag_source_label.clone();

    // Per-pane agent-status dot — mirrors the space-level badges: gold while
    // the agent is working, amber + pulse when it finished / waits / errored.
    let agent_status = use_agent_status_store();
    let pane_status = agent_status
        .read()
        .statuses
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| s.status.clone());
    let pane_dot_class = match pane_status.as_ref() {
        Some(AgentRunStatus::Working) => "status-dot is-working",
        // Thinking renders as a pulsing dot — distinct from the solid
        // working dot so a freshly-woken agent reads as "warming up".
        Some(AgentRunStatus::Thinking) => "status-dot is-thinking",
        Some(AgentRunStatus::WaitingForInput)
        | Some(AgentRunStatus::Error)
        | Some(AgentRunStatus::Completed) => "status-dot is-attention",
        _ => "status-dot is-idle",
    };
    let pane_dot_title = match pane_status.as_ref() {
        Some(AgentRunStatus::Working) => "Agent working".to_string(),
        Some(AgentRunStatus::Thinking) => "Agent thinking".to_string(),
        Some(AgentRunStatus::WaitingForInput) => "Agent waiting for input".to_string(),
        Some(AgentRunStatus::Error) => "Agent errored".to_string(),
        Some(AgentRunStatus::Completed) => "Agent finished".to_string(),
        _ => "No agent activity".to_string(),
    };
    let pane_id_for_rename = props.pane_id.clone();
    let space_id_for_rename = props.space_id.clone();

    // Resolve the currently-selected resume command + whether the dropdown is
    // shown. Computed here (not inside rsx!) because rsx! is a macro that
    // doesn't accept arbitrary `let` bindings in element position.
    let active_cmd: Option<String> = if resume_variants.is_empty() {
        display_resume_cmd.clone()
    } else {
        let idx = *selected_variant.read();
        Some(
            resume_variants
                .get(idx)
                .cloned()
                .unwrap_or_else(|| resume_variants[0].clone()),
        )
    };
    let is_multi = resume_variants.len() > 1;

    rsx! {
        div {
            style: "flex: 1; width: 100%; height: 100%; min-width: 0; min-height: 0; display: flex; flex-direction: column; background: var(--bg); overflow: hidden; box-sizing: border-box;",
            onpointerdown: move |_| {
                terminal_store.write().set_active(props.pane_id.clone());
            },

            // Pill header — distinct, refined, sits inside the pane
            div {
                style: "flex-shrink: 0; padding: 6px 8px 0 8px;",

                div {
                    // Grab surface for the pill drag-and-drop swap. The entire
                    // pill is the handle (no six-dot grip). A pointerdown
                    // records the source data; the fullscreen `PillDragOverlay`
                    // (mounted in `WorkspaceGrid` while `pill_drag.is_some()`)
                    // owns pointermove/up and commits a `swap_pane_agents` on
                    // drop. Only the primary pointer/button starts a drag so
                    // right-click and middle-click stay unaffected.
                    onpointerdown: move |e: dioxus::prelude::PointerEvent| {
                        // Only the primary pointer + primary button (left mouse
                        // / first touch) starts a drag — right-click and
                        // middle-click stay unaffected.
                        let is_primary_button = e
                            .data
                            .trigger_button()
                            .map(|b| {
                                matches!(
                                    b,
                                    dioxus::html::input_data::MouseButton::Primary
                                )
                            })
                            .unwrap_or(true);
                        if !e.data.is_primary() || !is_primary_button {
                            return;
                        }
                        let coords = e.data.client_coordinates();
                        let color = crate::utils::agent_commands::get_agent_color(&drag_agent_type_for_start);
                        pill_drag.set(Some(crate::components::workspace::pill_drag::PillDrag {
                            source_pane_id: drag_pane_id_for_start.clone(),
                            source_space_id: drag_space_id.clone(),
                            source_label: drag_source_label.clone(),
                            source_color: color.to_string(),
                            pointer_id: e.data.pointer_id(),
                            start_x: coords.x,
                            start_y: coords.y,
                            cur_x: coords.x,
                            cur_y: coords.y,
                            moved: false,
                            target: None,
                            source_agent_type: drag_agent_type_for_start.to_string(),
                            source_is_agent: !matches!(&drag_agent_type_for_start, AgentType::Shell),
                        }));
                    },
                        "data-agent-pill": "true",
                        "data-agent-type": "{drag_agent_type}",
                        "data-agent-pane-id": "{drag_pane_id}",
                        "data-agent-label": "{drag_source_label_for_attr}",
                        style: "display: flex; align-items: center; gap: 8px; padding: 4px 10px; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-sm); cursor: grab; flex-shrink: 0;",

                    // Left: agent-status dot (working / attention / idle)
                    span {
                        class: pane_dot_class,
                        style: "flex-shrink: 0;",
                        title: pane_dot_title.clone(),
                        "aria-label": "{pane_dot_title}",
                    }

                    // Left: editable title
                    {
                        let editing = editing_title();
                        let left_text = if editing {
                            rsx! {
                                input {
                                    style: "font-family: var(--font-ui); font-size: var(--text-xs); font-weight: 600; color: var(--text); background: transparent; border: none; outline: none; max-width: 200px; padding: 0; margin: 0;",
                                    value: temp_title(),
                                    oninput: move |e: dioxus::prelude::FormEvent| {
                                        temp_title.set(e.value().clone());
                                    },
                                    onblur: move |_| {
                                        editing_title.set(false);
                                    },
                                    onkeydown: move |e: dioxus::prelude::KeyboardEvent| {
                                        if matches!(e.key(), dioxus::prelude::Key::Enter) {
                                            let new_label = temp_title();
                                            let pid = pane_id_for_rename.clone();
                                            let sid = space_id_for_rename.clone();
                                            {
                                                let mut ws = workspace.write();
                                                ws.update_space(&sid, |space| {
                                                    for pane in &mut space.panes {
                                                        if pane.id == pid {
                                                            pane.label = Some(new_label.clone());
                                                            break;
                                                        }
                                                    }
                                                });
                                            }
                                            editing_title.set(false);
                                        }
                                    },
                                }
                            }
                        } else {
                            rsx! {
                                span {
                                    style: "font-family: var(--font-ui); font-size: var(--text-xs); font-weight: 600; color: var(--text); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; max-width: 200px; cursor: text;",
                                    title: "{title_text}",
                                    ondoubleclick: move |_| {
                                        temp_title.set(left_label.clone());
                                        editing_title.set(true);
                                    },
                                    "{display_label}"
                                }
                            }
                        };
                        left_text
                    }
                    // Right: subtle process badge
                    if let Some(ref badge) = right_badge {
                        span {
                            style: "font-family: var(--font-ui); font-size: var(--text-2xs); font-weight: 500; color: var(--text-secondary); background: var(--bgTertiary); border: 1px solid var(--border); border-radius: 4px; padding: 1px 6px; margin-left: 6px;",
                            "{badge}"
                        }
                    }
                    div {
                        style: "display: flex; align-items: center; gap: 4px; margin-left: auto;",
                        button {
                            class: "icon-btn",
                            title: if is_fullscreen { "Exit Fullscreen" } else { "Fullscreen" },
                            // Stop the pill-drag grab surface from intercepting
                            // pointerdown on the icon (so clicking the icon
                            // never starts a swap drag).
                            onpointerdown: move |e: dioxus::prelude::PointerEvent| {
                                e.stop_propagation();
                            },
                            onclick: move |e| {
                                e.stop_propagation();
                                if is_fullscreen {
                                    fullscreen_pane_id.set(None);
                                } else {
                                    fullscreen_pane_id.set(Some(pane_id_for_fullscreen.clone()));
                                }
                            },
                            if is_fullscreen {
                                IconMinimize { size: Some(12), color: Some("currentColor".to_string()) }
                            } else {
                                IconFullscreen { size: Some(12), color: Some("currentColor".to_string()) }
                            }
                        }

                        button {
                            class: "icon-btn",
                            title: "Close pane",
                            // Stop the pill-drag grab surface from intercepting
                            // pointerdown on the close icon.
                            onpointerdown: move |e: dioxus::prelude::PointerEvent| {
                                e.stop_propagation();
                            },
                            onclick: move |e| {
                                e.stop_propagation();
                                // Keep the pane's registry signal alive until the
                                // XtermMount drop hook has disposed its resources.
                                // Removing it synchronously races that cleanup and
                                // can leave the remaining grid blank.
                                if terminal_registry.is_closing(&pane_id_for_close) {
                                    return;
                                }
                                terminal_registry.mark_closing(&pane_id_for_close);
                                let pane_id = pane_id_for_close.clone();
                                let space_id = space_id_for_close.clone();
                                let mut workspace = workspace;
                                let mut terminal_store = terminal_store;
                                let terminal_registry_for_close = terminal_registry.clone();
                                let mut fullscreen_pane_id = fullscreen_pane_id;
                                spawn(async move {
                                    if pty_kill(&pane_id).await.is_err() {
                                        terminal_registry_for_close.cancel_closing(&pane_id);
                                        return;
                                    }

                                    workspace
                                        .write()
                                        .remove_pane_from_space(&space_id, &pane_id);
                                    {
                                        let mut term = terminal_store.write();
                                        term.known_pane_ids.remove(&pane_id);
                                        if term.active_session_id.as_deref() == Some(&pane_id) {
                                            term.active_session_id =
                                                term.known_pane_ids.iter().next().cloned();
                                        }
                                        term.generation = term.generation.wrapping_add(1);
                                    }
                                    // Clear fullscreen if this was the full-screen pane.
                                    if is_fullscreen {
                                        fullscreen_pane_id.set(None);
                                    }
                                });
                            },
                            IconClose { size: Some(14), color: Some("currentColor".to_string()) }
                        }
                    }
                }

            }

            // Resume banner — appears when a previous agent session left a
            // resume id and the agent isn't currently running. For Claude
            // sessions this renders a dropdown of every resume variant (plain
            // `claude --resume <id>` plus each "Treat as Claude" alias); for
            // other agents it shows the single captured/synthesized command.
            if show_resume_banner {
                if let Some(cmd) = active_cmd.clone() {
                    div {
                        style: "flex-shrink: 0; padding: 6px 8px 0 8px;",
                        div {
                            style: "display: flex; align-items: center; gap: 8px; padding: 6px 10px 6px 12px; background: var(--bgSecondary); border: 1px solid var(--accent, var(--border)); border-radius: 10px;",
                            span {
                                style: "font-family: var(--font-ui); font-size: var(--text-2xs); font-weight: 600; color: var(--text-secondary); text-transform: uppercase; letter-spacing: 0.04em; flex-shrink: 0;",
                                "Resume"
                            }
                            // Command: a dropdown when there are multiple
                            // variants (Claude aliases), otherwise a static
                            // code element. Both bound to the active variant.
                            if is_multi {
                                select {
                                    style: "font-family: var(--font-mono, monospace); font-size: var(--text-xs); color: var(--text); background: var(--bgTertiary); border: 1px solid var(--border); border-radius: 6px; padding: 2px 6px; flex: 1; min-width: 0; max-width: 360px; overflow: hidden;",
                                    value: "{*selected_variant.read()}",
                                    onchange: {
                                        move |e: dioxus::prelude::FormEvent| {
                                            if let Ok(idx) = e.value().parse::<usize>() {
                                                if idx < resume_variants.len() {
                                                    selected_variant.set(idx);
                                                }
                                            }
                                        }
                                    },
                                    for (i, variant) in resume_variants.iter().enumerate() {
                                        option {
                                            value: "{i}",
                                            selected: i == *selected_variant.read(),
                                            title: "{variant}",
                                            "{variant}"
                                        }
                                    }
                                }
                            } else {
                                code {
                                    style: "font-family: var(--font-mono, monospace); font-size: var(--text-xs); color: var(--text); background: var(--bgTertiary); border: 1px solid var(--border); border-radius: 6px; padding: 2px 8px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; flex: 1; min-width: 0;",
                                    title: "{cmd}",
                                    "{cmd}"
                                }
                            }
                            div {
                                style: "display: flex; align-items: center; gap: 4px; flex-shrink: 0;",
                                // Resume — write the selected variant into this
                                // pane's PTY and run it.
                                button {
                                    class: "btn-primary btn-sm",
                                    style: "font-family: var(--font-ui); font-size: var(--text-2xs); font-weight: 600; padding: 3px 10px; cursor: pointer;",
                                    title: "Run this command in the pane",
                                    onclick: {
                                        let pane_id = props.pane_id.clone();
                                        let space_id = props.space_id.clone();
                                        let cmd = cmd.clone();
                                        move |e: dioxus::prelude::MouseEvent| {
                                            e.stop_propagation();
                                            banner_dismissed.set(true);
                                            // Persist dismissal synchronously in
                                            // the handler (the signal is already
                                            // captured at component top). Do NOT
                                            // call use_workspace_store() inside
                                            // the spawned task — it is a Dioxus
                                            // hook and panics outside of render.
                                            {
                                                let pid = pane_id.clone();
                                                let sid = space_id.clone();
                                                workspace.write().update_space(&sid, |space| {
                                                    for pane in &mut space.panes {
                                                        if pane.id == pid {
                                                            pane.resume_dismissed = Some(true);
                                                            break;
                                                        }
                                                    }
                                                });
                                            }
                                            let pane_id = pane_id.clone();
                                            let to_run = format!("{}\n", cmd);
                                            spawn(async move {
                                                if let Err(err) = pty_write(&pane_id, &to_run).await {
                                                    web_sys::console::error_1(
                                                        &format!("resume write failed: {:?}", err).into(),
                                                    );
                                                }
                                            });
                                        }
                                    },
                                    "Resume"
                                }
                                // Copy — copy the selected variant to the clipboard.
                                button {
                                    class: "icon-btn",
                                    title: "Copy command",
                                    onclick: {
                                        let cmd = cmd.clone();
                                        move |e: dioxus::prelude::MouseEvent| {
                                            e.stop_propagation();
                                            if let Some(window) = web_sys::window() {
                                                if let Ok(nav) = js_sys::Reflect::get(
                                                    &window,
                                                    &wasm_bindgen::JsValue::from_str("navigator"),
                                                ) {
                                                    if let Ok(cb) = js_sys::Reflect::get(
                                                        &nav,
                                                        &wasm_bindgen::JsValue::from_str("clipboard"),
                                                    ) {
                                                        if let Ok(write_text) = js_sys::Reflect::get(
                                                            &cb,
                                                            &wasm_bindgen::JsValue::from_str("writeText"),
                                                        ) {
                                                            if let Ok(fn_) =
                                                                write_text.dyn_into::<js_sys::Function>()
                                                            {
                                                                let _ = fn_.call1(
                                                                    &cb,
                                                                    &wasm_bindgen::JsValue::from_str(&cmd),
                                                                );
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            copied.set(true);
                                        }
                                    },
                                    if copied() {
                                        IconCheck { size: Some(13), color: Some("currentColor".to_string()) }
                                    } else {
                                        IconCopy { size: Some(13), color: Some("currentColor".to_string()) }
                                    }
                                }
                                // Dismiss — hide the banner and persist the
                                // dismissal so it stays gone across restarts.
                                // (A freshly captured, different resume id
                                // resets this back to Some(false).)
                                button {
                                    class: "icon-btn",
                                    title: "Dismiss",
                                    onclick: {
                                        let pane_id = props.pane_id.clone();
                                        let space_id = props.space_id.clone();
                                        move |e: dioxus::prelude::MouseEvent| {
                                            e.stop_propagation();
                                            banner_dismissed.set(true);
                                            let pid = pane_id.clone();
                                            let sid = space_id.clone();
                                            // Write the signal directly in the
                                            // handler (mirrors the rename
                                            // handler). Do NOT call
                                            // use_workspace_store() here — that
                                            // is a Dioxus hook and may only run
                                            // during component render, not in
                                            // an event callback.
                                            workspace.write().update_space(&sid, |space| {
                                                for pane in &mut space.panes {
                                                    if pane.id == pid {
                                                        pane.resume_dismissed = Some(true);
                                                        break;
                                                    }
                                                }
                                            });
                                        }
                                    },
                                    IconClose { size: Some(13), color: Some("currentColor".to_string()) }
                                }
                            }
                        }
                    }
                }
            }

            // Shell body — uniform 4px inset so terminal content never touches
            // the pane-wrap border (right/bottom) or the pill header (top). The
            // padding lives on THIS wrapper, not on .xterm-mount: FitAddon reads
            // .xterm-mount's computed width/height (which already reflect this
            // inset because .xterm-mount is width:100%/height:100% of the
            // padded content box) and subtracts only the child .xterm element's
            // own padding, so cols/rows stay correct with no clipping.
            div {
                style: "flex: 1; min-width: 0; min-height: 0; padding: 4px; background: var(--bg); overflow: hidden;",
                if props.use_xterm {
                    { render_shell_pane(
                        props.pane_id.clone(),
                        props.cwd.clone(),
                        props.agent_type.clone(),
                        props.resume_id.clone(),
                        props.custom_cmd.clone(),
                        props.bypass_mode,
                    ) }
                } else {
                    TerminalPaneBody { pane_id: props.pane_id.clone() }
                }
            }
        }
    }
}
