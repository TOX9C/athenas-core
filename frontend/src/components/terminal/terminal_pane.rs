use super::pane_header::PaneHeader;
use crate::stores::agent_output::use_agent_output_store;
use crate::stores::agent_status::{use_agent_status_store, AgentRunStatus};
use crate::stores::terminal::{use_terminal_store, PtyStatus};
use crate::stores::ui::use_ui_store;
use crate::stores::workspace::use_workspace_store;
use crate::tauri_bridge;
use crate::xterm_interop;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct TerminalPaneProps {
    pub pane_id: String,
    #[props(default = "Shell".to_string())]
    pub agent_type: String,
}

/// Internal state for a live xterm.js terminal instance.
#[derive(Clone, Default)]
struct XtermState {
    handle: String,
    initialized: bool,
}

#[component]
pub fn TerminalPane(props: TerminalPaneProps) -> Element {
    let mut xterm = use_signal(XtermState::default);
    let mut terminal_store = use_terminal_store();
    let agent_output = use_agent_output_store();
    let agent_status = use_agent_status_store();
    let workspace_store = use_workspace_store();
    let mut ui_store = use_ui_store();

    let is_fullscreen =
        ui_store.read().fullscreen_pane_id.as_deref() == Some(props.pane_id.as_str());

    let session_status = terminal_store
        .read()
        .sessions
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| s.status.clone());

    let run_status = agent_status
        .read()
        .statuses
        .iter()
        .find(|(id, _)| id == &props.pane_id)
        .map(|(_, s)| s.status.clone());

    let border_color = match &session_status {
        Some(PtyStatus::Running) => "#98c379",
        Some(PtyStatus::Ready) => "#61afef",
        Some(PtyStatus::Error) => "#e06c75",
        Some(PtyStatus::Exited) => "#5c6370",
        _ => match &run_status {
            Some(AgentRunStatus::Thinking) | Some(AgentRunStatus::Working) => "#61afef",
            Some(AgentRunStatus::WaitingForInput) => "#e5c07b",
            Some(AgentRunStatus::Error) => "#e06c75",
            Some(AgentRunStatus::Completed) | Some(AgentRunStatus::Idle) => "transparent",
            _ => "transparent",
        },
    };

    // Determine theme from UI store (if accessible) or default to dark.
    let theme = "dark";

    // Pane ID for event names -- clone early to satisfy 'static closures.
    let pane_id_for_write = props.pane_id.clone();
    let pane_id_for_listen = props.pane_id.clone();

    // -- Mount effect: create xterm terminal, spawn PTY, wire events --------
    // We use spawn + a small async delay to ensure the DOM element with
    // id "xterm-container-{pane_id}" has been rendered and committed before
    // we attempt to create the xterm instance inside it.  A plain use_effect
    // may fire before the DOM node is attached.
    let pane_id_for_spawn = props.pane_id.clone();
    let pane_id_for_rsx = props.pane_id.clone();
    use_effect(move || {
        if xterm().initialized {
            return;
        }

        // Ensure xterm bootstrap JS is loaded.
        xterm_interop::ensure_xterm_bootstrap();

        let pane_id_rid = pane_id_for_spawn.clone();
        let pane_id_wid = pane_id_for_write.clone();
        let pane_id_lid = pane_id_for_listen.clone();

        spawn(async move {
            // Wait one microtask tick so the DOM commit finishes.
            gloo::timers::future::TimeoutFuture::new(16).await;

            let window = match web_sys::window() {
                Some(w) => w,
                None => {
                    log::warn!("xterm init: no window object");
                    return;
                }
            };
            let doc = match window.document() {
                Some(d) => d,
                None => {
                    log::warn!("xterm init: no document object");
                    return;
                }
            };

            let container_id = format!("xterm-container-{}", pane_id_rid);
            let container = match doc.get_element_by_id(&container_id) {
                Some(el) => el,
                None => {
                    log::warn!(
                        "xterm container #{} not found in DOM after delay",
                        container_id
                    );
                    return;
                }
            };

            let handle = match xterm_interop::create_terminal(&container, theme) {
                Ok(h) if h != "-1" => h,
                Ok(_) => {
                    log::warn!("xterm.js not ready yet, will retry");
                    return;
                }
                Err(e) => {
                    log::warn!("xterm create failed: {:?}", e);
                    return;
                }
            };

            // Safety net: if xterm.js created the terminal in a detached DOM
            // node, find the rendered container and append it now.
            if let Some(parent) = doc.get_element_by_id(&format!("xterm-container-{}", pane_id_rid))
            {
                let _ = parent.append_child(&container);
            }

            xterm.set(XtermState {
                handle: handle.clone(),
                initialized: true,
            });

            // Spawn the PTY process.
            let cwd = {
                let state = workspace_store.read();
                state
                    .active_space_id
                    .as_ref()
                    .and_then(|id| {
                        state
                            .spaces
                            .iter()
                            .find(|s| &s.id == id)
                            .map(|s| s.dir.clone())
                    })
                    .unwrap_or_else(|| "/".to_string())
            };

            let shell = tauri_bridge::pty_default_shell()
                .await
                .unwrap_or_else(|_| "/bin/zsh".to_string());
            let _ = tauri_bridge::pty_spawn(&pane_id_rid, &cwd, &shell).await;

            // Mark session as running after spawn.
            terminal_store
                .write()
                .update_session_status(&pane_id_rid, PtyStatus::Running);

            // Attach custom key handler so modifier combos (Cmd+K, Cmd+J,
            // etc.) pass through to the Dioxus root instead of being
            // consumed by xterm.js.
            xterm_interop::attach_custom_key_event_handler(&handle);

            // Register onData callback -- user keystrokes -> PTY write.
            let id_for_ondata = pane_id_wid.clone();
            xterm_interop::on_terminal_data(&handle, move |data: String| {
                let id = id_for_ondata.clone();
                let data_owned = data.clone();
                spawn(async move {
                    let _ = tauri_bridge::pty_write(&id, &data_owned).await;
                });
            });

            // Listen for terminal:data:{pane_id} events -- PTY output -> xterm write.
            let event_name = format!("terminal:data:{}", pane_id_lid);
            let handle_for_listen = handle.clone();
            let _ = tauri_bridge::listen(&event_name, move |payload: String| {
                xterm_interop::write_terminal(&handle_for_listen, &payload);
            });

            // Listen for terminal:ready:{pane_id} events -- shell prompt detected.
            let event_ready = format!("terminal:ready:{}", pane_id_lid);
            let mut terminal_store_for_ready = use_terminal_store();
            let pane_id_for_ready = pane_id_lid.clone();
            let _ = tauri_bridge::listen(&event_ready, move |_payload: String| {
                terminal_store_for_ready
                    .write()
                    .update_session_status(&pane_id_for_ready, PtyStatus::Ready);
            });

            // Listen for terminal:exit:{pane_id} events -- PTY exited.
            let event_exit = format!("terminal:exit:{}", pane_id_lid);
            let mut terminal_store_for_exit = use_terminal_store();
            let pane_id_for_exit = pane_id_lid.clone();
            let _ = tauri_bridge::listen(&event_exit, move |payload: String| {
                let exit_code: Option<i32> = serde_json::from_str::<serde_json::Value>(&payload)
                    .ok()
                    .and_then(|v| v.get("exitCode").and_then(|c| c.as_i64()).map(|c| c as i32));
                if let Some(code) = exit_code {
                    terminal_store_for_exit.write().update_session(
                        &pane_id_for_exit,
                        crate::stores::terminal::PtySessionUpdate {
                            last_exit_code: Some(code),
                            status: Some(PtyStatus::Exited),
                            ..Default::default()
                        },
                    );
                } else {
                    terminal_store_for_exit
                        .write()
                        .update_session_status(&pane_id_for_exit, PtyStatus::Exited);
                }
            });

            // Fit the terminal to its container.
            xterm_interop::fit_terminal(&handle);
        });
    });

    // -- Resize effect: fit xterm when fullscreen toggles --------------------
    let handle_for_resize = xterm().handle.clone();
    let initialized = xterm().initialized;
    let pane_id_for_resize = pane_id_for_rsx.clone();
    use_effect(move || {
        if initialized {
            xterm_interop::fit_terminal(&handle_for_resize);
            let (cols, rows) = xterm_interop::get_terminal_size(&handle_for_resize);
            let id = pane_id_for_resize.clone();
            spawn(async move {
                let _ = tauri_bridge::pty_resize(&id, cols, rows).await;
            });
        }
    });

    // Close handler: kill PTY, dispose terminal, and remove pane from workspace.
    let pane_id_for_close = props.pane_id.clone();
    let workspace_for_close = use_workspace_store();
    let ui_for_close = use_ui_store();
    let xterm_handle_for_close = xterm().handle.clone();
    let on_close = move |_| {
        // Dispose the xterm.js terminal instance.
        xterm_interop::dispose_terminal(&xterm_handle_for_close);
        let id = pane_id_for_close.clone();
        let mut ws = workspace_for_close;
        let mut ui = ui_for_close;
        spawn(async move {
            let _ = tauri_bridge::pty_kill(&id).await;
            let state = ws.read();
            if let Some(active_id) = &state.active_space_id {
                let space_id = active_id.clone();
                drop(state);
                ws.write().remove_pane_from_space(&space_id, &id);
            }
            // Clear fullscreen if this pane was fullscreen.
            if ui.read().fullscreen_pane_id.as_deref() == Some(id.as_str()) {
                ui.write().fullscreen_pane_id = None;
            }
        });
    };

    rsx! {
        div {
            class: "terminal-pane",
            style: "display: flex; flex-direction: column; height: 100%; background: var(--bg); border-radius: 6px; overflow: hidden; outline: 1.5px solid {border_color}; transition: outline-color 0.2s; min-height: 0;",

            PaneHeader {
                pane_id: pane_id_for_rsx.clone(),
                agent_type: props.agent_type.clone(),
                is_fullscreen: is_fullscreen,
                on_fullscreen: {
                    let fs_id = pane_id_for_rsx.clone();
                    move |fs: bool| {
                        if fs {
                            ui_store.write().fullscreen_pane_id = Some(fs_id.clone());
                        } else {
                            ui_store.write().fullscreen_pane_id = None;
                        }
                    }
                },
                on_close: on_close,
            }

            // xterm.js terminal container
            div {
                id: "xterm-container-{pane_id_for_rsx}",
                style: "flex: 1; min-height: 0; overflow: hidden;",

                // Fallback: show plain-text output if xterm not initialized
                if !xterm().initialized {
                    div {
                        style: "padding: 8px; font-family: monospace; font-size: 12px; color: var(--textMuted); white-space: pre-wrap;",
                        {
                            let output_lines: Vec<String> = agent_output
                                .read()
                                .buffers
                                .iter()
                                .find(|(id, _)| id == &pane_id_for_rsx)
                                .map(|(_, lines)| lines.iter().map(|l| l.text.clone()).collect())
                                .unwrap_or_default();
                            if output_lines.is_empty() {
                                "$ _".to_string()
                            } else {
                                output_lines.join("\n")
                            }
                        }
                    }
                }
            }
        }
    }
}
