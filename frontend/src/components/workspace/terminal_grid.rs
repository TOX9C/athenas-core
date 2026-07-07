use dioxus::prelude::*;
use wasm_bindgen::JsCast;

use crate::components::shared::icon::{
    IconCheck, IconClose, IconCopy, IconFullscreen, IconMinimize,
};
use crate::components::shared::illustration::{EmptyArt, EmptyState};
use crate::stores::terminal::{
    use_terminal_registry, use_terminal_store, TerminalCell, TerminalColor,
};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::{use_workspace_store, AgentType, Space};
use crate::tauri_bridge::{pty_agent_info, pty_kill, pty_write};
use crate::types::workspace::CustomAgent;
use crate::utils::agent_commands::{
    agent_process_name, claude_resume_variants, custom_agent_process_name, get_agent_color,
    get_agent_label, get_agent_resume_command,
};

#[cfg(feature = "xterm")]
use crate::components::workspace::xterm_mount::XtermMount;

#[cfg(feature = "xterm")]
fn render_shell_pane(
    pane_id: String,
    cwd: String,
    agent_type: AgentType,
    resume_id: Option<String>,
    custom_cmd: Option<String>,
) -> Element {
    rsx! { XtermMount { key: "xterm-{pane_id}", pane_id, cwd, agent_type, resume_id, custom_cmd } }
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
// DragInfo & DragKind
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct DragInfo {
    kind: DragKind,
    scope_index: Option<usize>,
    index: usize,
    start_x: f64,
    start_y: f64,
    initial_left: f64,
    initial_right: f64,
    dimension_pixels: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DragKind {
    Col,
    Row,
}

// ---------------------------------------------------------------------------
// WorkspaceGrid
// ---------------------------------------------------------------------------

#[component]
pub fn WorkspaceGrid(props: WorkspaceGridProps) -> Element {
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

    let actual_row_count = (pane_count + cols - 1) / cols;

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
                                            s.push_str(" border-right: 1px solid color-mix(in srgb, var(--border, #888) 58%, transparent);");
                                        }
                                        if has_bottom {
                                            s.push_str(" border-bottom: 1px solid color-mix(in srgb, var(--border, #888) 58%, transparent);");
                                        }
                                        s
                                    };

                                    // DnD target highlight: gold ring while this pane is
                                    // the hit-tested drop target of an in-flight pill drag.
                                    let is_dnd_target = pill_drag
                                        .read()
                                        .as_ref()
                                        .and_then(|d| d.target_pane_id.as_ref())
                                        .map(|t| t == &pane.id)
                                        .unwrap_or(false);
                                    let wrapper_class = if is_dnd_target {
                                        "pane-wrap is-dnd-target"
                                    } else {
                                        "pane-wrap"
                                    };

                                    rsx! {
                                        div {
                                            key: "pane-wrap-{space.id}-{pane.id}",
                                            class: "{wrapper_class}",
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
                                                is_shell: matches!(pane.agent_type, AgentType::Shell | AgentType::Custom),
                                                resume_id: pane.resume_id.clone(),
                                                resume_cmd: pane.resume_cmd.clone(),
                                                resume_dismissed: pane.resume_dismissed.clone(),
                                                custom_cmd: pane.custom_cmd.clone(),
                                                custom_agent_id: pane.custom_agent_id.clone(),
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
            crate::components::workspace::pill_drag::PillDragOverlay {
                drag: pill_drag,
                workspace: workspace,
                terminal_store: terminal_store,
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
    is_shell: bool,
    resume_id: Option<String>,
    resume_cmd: Option<String>,
    resume_dismissed: Option<bool>,
    custom_cmd: Option<String>,
    custom_agent_id: Option<String>,
    label: Option<String>,
    fullscreen_pane_id: Signal<Option<String>>,
    pill_drag: Signal<Option<crate::components::workspace::pill_drag::PillDrag>>,
}

#[component]
fn PaneItem(props: PaneItemProps) -> Element {
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

    // Editable title state
    let mut editing_title = use_signal(|| false);
    let mut temp_title = use_signal(|| String::new());

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

    let left_label = crate::utils::pane_label::resolve_pane_label(
        props.label.as_deref(),
        &title_state,
        &props.agent_type,
        fg_process.as_deref(),
        ui_state.read().smart_pane_titles,
        &agent_label,
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
                        let label_text = display_label.clone();
                        let color = crate::utils::agent_commands::get_agent_color(&drag_agent_type);
                        pill_drag.set(Some(crate::components::workspace::pill_drag::PillDrag {
                            source_pane_id: drag_pane_id.clone(),
                            source_space_id: drag_space_id.clone(),
                            source_label: label_text,
                            source_color: color.to_string(),
                            pointer_id: e.data.pointer_id(),
                            start_x: coords.x,
                            start_y: coords.y,
                            cur_x: coords.x,
                            cur_y: coords.y,
                            moved: false,
                            target_pane_id: None,
                        }));
                    },
                    style: "display: flex; align-items: center; gap: 8px; padding: 4px 12px; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: 999px; cursor: grab; flex-shrink: 0;",

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
                                {
                                    let mut ws = workspace.write();
                                    ws.remove_pane_from_space(&space_id_for_close, &pane_id_for_close);
                                }
                                {
                                    // `use_terminal_registry()` is a Dioxus hook
                                    // and may NOT be called inside an event handler
                                    // (runs outside render → "hook list already
                                    // borrowed" panic on click). Use the registry
                                    // captured synchronously at the top of PaneItem.
                                    let registry = &terminal_registry;
                                    let mut term = terminal_store.write();
                                    // Phase 4 (Item 3): per-pane data lives in the
                                    // registry; remove the pane's inner signal, drop
                                    // the id from the slimmed store's membership set,
                                    // reassign active if needed, and bump the
                                    // whole-store generation (cross-session change).
                                    registry.remove(&pane_id_for_close);
                                    term.known_pane_ids.remove(&pane_id_for_close);
                                    if term.active_session_id.as_deref()
                                        == Some(&pane_id_for_close)
                                    {
                                        term.active_session_id =
                                            term.known_pane_ids.iter().next().cloned();
                                    }
                                    term.generation = term.generation.wrapping_add(1);
                                }
                                // Clear fullscreen if this pane was full-screened
                                if is_fullscreen {
                                    fullscreen_pane_id.set(None);
                                }
                                spawn({
                                    let pane_id = pane_id_for_close.clone();
                                    async move {
                                        let _ = pty_kill(&pane_id).await;
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
                                    class: "btn-pill",
                                    style: "font-family: var(--font-ui); font-size: var(--text-2xs); font-weight: 600; color: var(--bg); background: var(--accent, var(--text)); border: none; border-radius: 999px; padding: 3px 12px; cursor: pointer;",
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

            // Shell body — flat, no padding, fills edge-to-edge below the pill header
            div {
                style: "flex: 1; min-width: 0; min-height: 0; padding: 0; background: var(--bg); overflow: hidden;",
                if props.is_shell {
                    { render_shell_pane(props.pane_id.clone(), props.cwd.clone(), props.agent_type.clone(), props.resume_id.clone(), props.custom_cmd.clone()) }
                } else {
                    TerminalPaneBody { pane_id: props.pane_id.clone() }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalPaneBody
// ---------------------------------------------------------------------------

#[component]
fn TerminalPaneBody(pane_id: String) -> Element {
    // Subscribe to THIS pane's inner signal for grid snapshots (Item 3). A
    // cell delta in pane A re-clones only pane A's grid here; pane B's memo
    // doesn't re-evaluate. `use_memo` caches the clone until the signal moves.
    //
    // `use_terminal_registry()` is a hook — captured once, synchronously, at
    // render top; `registry.session_signal(...)` is a plain method safe to
    // call inside the `use_memo` closure. Calling `use_session_signal(...)`
    // here would re-enter the hook list (it warps `use_context`) and panic
    // Dioxus at mount with "hook list already borrowed".
    let terminal_registry = use_terminal_registry();
    let Bud = use_memo(move || {
        terminal_registry
            .session_signal(&pane_id)
            .and_then(|s| s.try_read().ok().map(|r| r.grid.clone()))
            .unwrap_or_default()
    })();

    rsx! {
        div {
            style: "flex: 1; display: flex; flex-direction: column; min-height: 0; min-width: 0; background: var(--bg); overflow: hidden; padding-left: 3px; padding-bottom: 4px;",
            div {
                style: "font-family: 'JetBrains Mono', 'Fira Code', 'Cascadia Code', monospace; font-size: 11px; line-height: 1.4; color: var(--text); white-space: pre-wrap; overflow-wrap: break-word;",
                if Bud.is_empty() {
                    "Waiting for output..."
                } else {
                    for (row_idx, row) in Bud.iter().enumerate() {
                        div {
                            key: "row-{row_idx}",
                            style: "display: flex;",
                            for cell in row.iter() {
                                TerminalCellItem { cell: cell.clone() }
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TerminalCellItem
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct TerminalCellItemProps {
    cell: TerminalCell,
}

#[component]
fn TerminalCellItem(props: TerminalCellItemProps) -> Element {
    let cell = &props.cell;
    let fg = color_to_css(&cell.fg);
    let bg = color_to_css(&cell.bg);
    let bold = if cell.bold { "font-weight: bold;" } else { "" };
    let style = format!("color: {}; background-color: {}; {}", fg, bg, bold);

    rsx! {
        span {
            style: "{style}",
            "{cell.text}"
        }
    }
}

// ---------------------------------------------------------------------------
// ColDivider
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct ColDividerProps {
    space_id: String,
    row_index: usize,
    index: usize,
    col_widths: Signal<Vec<Vec<f64>>>,
    drag: Signal<Option<DragInfo>>,
}

#[component]
fn ColDivider(props: ColDividerProps) -> Element {
    let mut drag = props.drag;
    let col_widths = props.col_widths;
    let row_index = props.row_index;
    let index = props.index;

    let space_id_for_col_resize = props.space_id.clone();

    let onmousedown = move |e: MouseEvent| {
        let coords = e.data.client_coordinates();
        let (initial_left, initial_right) = {
            let widths = col_widths.read();
            let row = widths.get(row_index);
            let left = row.and_then(|r| r.get(index)).copied().unwrap_or(1.0);
            let right = row.and_then(|r| r.get(index + 1)).copied().unwrap_or(1.0);
            (left, right)
        };
        let dimension_pixels =
            workspace_grid_dimension(DragKind::Col, &space_id_for_col_resize).unwrap_or(0.0);
        drag.set(Some(DragInfo {
            kind: DragKind::Col,
            scope_index: Some(row_index),
            index: index,
            start_x: coords.x,
            start_y: coords.y,
            initial_left,
            initial_right,
            dimension_pixels,
        }));
    };

    let is_dragging = matches!(
        &*drag.read(),
        Some(d) if d.kind == DragKind::Col && d.scope_index == Some(row_index) && d.index == index
    );

    rsx! {
        div {
            style: "position: relative; width: 0; min-width: 0; flex-shrink: 0; overflow: visible; z-index: 2;",
            div {
                class: if is_dragging { "pane-divider-col is-dragging" } else { "pane-divider-col" },
                onmousedown: onmousedown,
                title: "Resize panes",
                style: "position: absolute; left: -4px; top: 0; bottom: 0; width: 8px; cursor: col-resize;",
            }
        }
    }
}

// ---------------------------------------------------------------------------
// RowDivider
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct RowDividerProps {
    space_id: String,
    index: usize,
    row_heights: Signal<Vec<f64>>,
    drag: Signal<Option<DragInfo>>,
}

#[component]
fn RowDivider(props: RowDividerProps) -> Element {
    let mut drag = props.drag;
    let row_heights = props.row_heights;
    let index = props.index;

    let space_id_for_row_resize = props.space_id.clone();

    let onmousedown = move |e: MouseEvent| {
        let coords = e.data.client_coordinates();
        let (initial_left, initial_right) = {
            let heights = row_heights.read();
            let left = heights.get(index).copied().unwrap_or(1.0);
            let right = heights.get(index + 1).copied().unwrap_or(1.0);
            (left, right)
        };
        let dimension_pixels =
            workspace_grid_dimension(DragKind::Row, &space_id_for_row_resize).unwrap_or(0.0);
        drag.set(Some(DragInfo {
            kind: DragKind::Row,
            scope_index: None,
            index: index,
            start_x: coords.x,
            start_y: coords.y,
            initial_left,
            initial_right,
            dimension_pixels,
        }));
    };

    let is_dragging = matches!(
        &*drag.read(),
        Some(d) if d.kind == DragKind::Row && d.index == index
    );

    rsx! {
        div {
            style: "position: relative; height: 0; min-height: 0; flex-shrink: 0; overflow: visible; z-index: 2;",
            div {
                class: if is_dragging { "pane-divider-row is-dragging" } else { "pane-divider-row" },
                onmousedown: onmousedown,
                title: "Resize panes",
                style: "position: absolute; top: -4px; left: 0; right: 0; height: 8px; cursor: row-resize;",
            }
        }
    }
}

// ---------------------------------------------------------------------------
// DragOverlay
// ---------------------------------------------------------------------------

#[derive(Props, Clone, PartialEq)]
struct DragOverlayProps {
    drag: Signal<Option<DragInfo>>,
    col_widths: Signal<Vec<Vec<f64>>>,
    row_heights: Signal<Vec<f64>>,
}

#[component]
fn DragOverlay(props: DragOverlayProps) -> Element {
    let mut drag = props.drag;
    let mut col_widths = props.col_widths;
    let mut row_heights = props.row_heights;

    let onmousemove = move |e: MouseEvent| {
        if let Some(drag_info) = *drag.read() {
            let coords = e.data.client_coordinates();
            let total = drag_info.initial_left + drag_info.initial_right;
            let dimension = drag_info.dimension_pixels;
            if dimension <= 0.0 {
                return;
            }

            match drag_info.kind {
                DragKind::Col => {
                    if let Some(row_idx) = drag_info.scope_index {
                        let delta = coords.x - drag_info.start_x;
                        let mut cw = col_widths.write();
                        if let Some(row) = cw.get_mut(row_idx) {
                            if let Some((left, right)) = resize_pair_from_drag(
                                drag_info.initial_left,
                                total,
                                delta,
                                dimension,
                            ) {
                                row[drag_info.index] = left;
                                row[drag_info.index + 1] = right;
                            }
                        }
                    }
                }
                DragKind::Row => {
                    let delta = coords.y - drag_info.start_y;
                    let mut rh = row_heights.write();
                    let len = rh.len();
                    if len > 1 && drag_info.index < len - 1 {
                        if let Some((left, right)) =
                            resize_pair_from_drag(drag_info.initial_left, total, delta, dimension)
                        {
                            rh[drag_info.index] = left;
                            rh[drag_info.index + 1] = right;
                        }
                    }
                }
            }
        }
    };

    let onmouseup = move |_e: MouseEvent| {
        drag.set(None);
    };

    let cursor = match *drag.read() {
        Some(DragInfo {
            kind: DragKind::Col,
            ..
        }) => "col-resize",
        Some(DragInfo {
            kind: DragKind::Row,
            ..
        }) => "row-resize",
        None => "default",
    };

    rsx! {
        div {
            style: "position: fixed; top: 0; left: 0; right: 0; bottom: 0; z-index: 9999; cursor: {cursor}; background: transparent;",
            onmousemove: onmousemove,
            onmouseup: onmouseup,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Resize a pair of columns/rows preserving total weight using the drag
/// start state and the current cursor delta relative to the grid dimension.
fn resize_pair_from_drag(
    initial_left: f64,
    total: f64,
    delta_pixels: f64,
    total_pixels: f64,
) -> Option<(f64, f64)> {
    if total_pixels <= 0.0 || total <= 0.0 {
        return None;
    }
    let delta_ratio = delta_pixels / total_pixels;
    let new_left = (initial_left + delta_ratio * total)
        .max(0.1)
        .min(total - 0.1);
    let new_right = total - new_left;
    Some((new_left, new_right))
}

fn workspace_grid_dimension(kind: DragKind, space_id: &str) -> Option<f64> {
    let window = web_sys::window()?;
    let document = window.document()?;
    let selector = format!(".workspace-grid-root[data-space-id=\"{}\"]", space_id);
    let element = document.query_selector(&selector).ok()??;
    let html_el = element.dyn_into::<web_sys::HtmlElement>().ok()?;
    let rect = html_el.get_bounding_client_rect();
    match kind {
        DragKind::Col => Some(rect.width()),
        DragKind::Row => Some(rect.height()),
    }
}

/// Calculate the number of horizontal row dividers between rendered rows.
#[cfg(test)]
fn row_divider_count(pane_count: usize, cols: usize, _rows: usize) -> usize {
    let actual_rows = if cols == 0 {
        0
    } else {
        (pane_count + cols - 1) / cols
    };
    actual_rows.saturating_sub(1)
}

/// Convert a TerminalColor to a CSS color string.
fn color_to_css(color: &TerminalColor) -> String {
    match color {
        TerminalColor::Default => "inherit".to_string(),
        TerminalColor::Black => "#000000".to_string(),
        TerminalColor::Red => "#ef4444".to_string(),
        TerminalColor::Green => "#22c55e".to_string(),
        TerminalColor::Yellow => "#eab308".to_string(),
        TerminalColor::Blue => "#3b82f6".to_string(),
        TerminalColor::Magenta => "#a855f7".to_string(),
        TerminalColor::Cyan => "#06b6d4".to_string(),
        TerminalColor::White => "#ffffff".to_string(),
        TerminalColor::BrightBlack => "#374151".to_string(),
        TerminalColor::BrightRed => "#f87171".to_string(),
        TerminalColor::BrightGreen => "#4ade80".to_string(),
        TerminalColor::BrightYellow => "#facc15".to_string(),
        TerminalColor::BrightBlue => "#60a5fa".to_string(),
        TerminalColor::BrightMagenta => "#c084fc".to_string(),
        TerminalColor::BrightCyan => "#22d3ee".to_string(),
        TerminalColor::BrightWhite => "#f9fafb".to_string(),
        TerminalColor::Indexed(idx) => ansi256_to_rgb(*idx),
        TerminalColor::Rgb(r, g, b) => format!("#{:02x}{:02x}{:02x}", r, g, b),
    }
}

/// Convert an ANSI 256 color index to an RGB hex string.
fn ansi256_to_rgb(idx: u8) -> String {
    // Standard 16 colors
    if idx < 16 {
        let colors = [
            "#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0",
            "#808080", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff",
        ];
        return colors[idx as usize].to_string();
    }
    // 216 color cube (16-231)
    if idx < 232 {
        let cube_idx = (idx as usize) - 16;
        let r = (cube_idx / 36) * 51;
        let g = ((cube_idx % 36) / 6) * 51;
        let b = (cube_idx % 6) * 51;
        return format!("#{:02x}{:02x}{:02x}", r, g, b);
    }
    // Grayscale (232-255)
    let gray = 8 + ((idx as usize) - 232) * 10;
    format!("#{:02x}{:02x}{:02x}", gray, gray, gray)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resize_pair_from_drag() {
        let result = resize_pair_from_drag(1.0, 2.0, 50.0, 200.0);
        assert!(result.is_some());
        let (left, right) = result.unwrap();
        assert!(left > 1.0);
        assert!(right < 1.0);
        assert!((left + right - 2.0).abs() < f64::EPSILON);

        let result = resize_pair_from_drag(1.0, 2.0, -1000.0, 200.0).unwrap();
        assert_eq!(result, (0.1, 1.9));
    }

    #[test]
    fn test_row_divider_count() {
        assert_eq!(row_divider_count(0, 2, 2), 0);
        assert_eq!(row_divider_count(1, 2, 2), 0);
        assert_eq!(row_divider_count(2, 2, 2), 0);
        assert_eq!(row_divider_count(3, 2, 2), 1);
        assert_eq!(row_divider_count(4, 2, 2), 1);
    }

    #[test]
    fn test_color_to_css_default() {
        assert_eq!(color_to_css(&TerminalColor::Default), "inherit");
    }

    #[test]
    fn test_ansi256_to_rgb() {
        assert_eq!(ansi256_to_rgb(0), "#000000");
        assert_eq!(ansi256_to_rgb(1), "#800000");
        assert_eq!(ansi256_to_rgb(16), "#000000");
        assert_eq!(ansi256_to_rgb(232), "#080808");
        assert_eq!(ansi256_to_rgb(255), "#eeeeee");
    }
}
