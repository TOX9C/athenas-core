use crate::components::shared::icon::{
    IconAthena, IconChevronRight, IconEye, IconFile, IconGrid, IconKanban, IconList, IconMinus,
    IconPlus, IconRefresh, IconSearch, IconSettings, IconSpaces, IconSwarm, IconTerminal, IconTune,
};
use crate::stores::command::{
    dispatch_command, filter_commands, use_command_handlers, use_command_store, Command,
};
use dioxus::prelude::*;

/// Format a shortcut string with Unicode symbols.
fn format_shortcut(shortcut: &str) -> String {
    shortcut
        .replace("Mod", "\u{2318}")
        .replace("Cmd", "\u{2318}")
        .replace("Ctrl", "\u{2303}")
        .replace("Shift", "\u{21e7}")
        .replace("Alt", "\u{2325}")
        .replace("Enter", "\u{23ce}")
        .replace("Escape", "\u{238b}")
        .replace("Backspace", "\u{232b}")
        .replace("Tab", "\u{21e5}")
}

/// Resolve a command's icon key to a glyph. Falls back to a chevron so
/// commands without an icon still align in the icon column.
fn palette_icon(cmd: &Command, color: &str) -> Element {
    let size = Some(15);
    let color = Some(color.to_string());
    match cmd.icon.as_deref() {
        Some("athena") => rsx! { IconAthena { size, color } },
        Some("spaces") => rsx! { IconSpaces { size, color } },
        Some("refresh") => rsx! { IconRefresh { size, color } },
        Some("list") => rsx! { IconList { size, color } },
        Some("grid") => rsx! { IconGrid { size, color } },
        Some("terminal") => rsx! { IconTerminal { size, color } },
        Some("file") => rsx! { IconFile { size, color } },
        Some("kanban") => rsx! { IconKanban { size, color } },
        Some("swarm") => rsx! { IconSwarm { size, color } },
        Some("eye") => rsx! { IconEye { size, color } },
        Some("plus") => rsx! { IconPlus { size, color } },
        Some("minus") => rsx! { IconMinus { size, color } },
        Some("tune") => rsx! { IconTune { size, color } },
        Some("settings") => rsx! { IconSettings { size, color } },
        _ => rsx! { IconChevronRight { size, color } },
    }
}

const KBD_STYLE: &str = "font-size: 10px; padding: 1px 5px; border-radius: 4px; background: transparent; border: 1px solid var(--border); color: var(--textDim); font-family: var(--font-ui); line-height: 1.5;";

#[component]
pub fn CommandPalette() -> Element {
    let mut command_state = use_command_store();
    let mut selected_idx = use_signal(|| 0usize);
    let handlers = use_command_handlers();

    if !command_state.read().is_open {
        return rsx! {};
    }

    let query = command_state.read().query.clone();
    let commands = command_state.read().commands.clone();
    let recent_ids = command_state.read().recent_ids.clone();
    // The palette shows all registered commands (no when-key gating), so the
    // visibility predicate is always false — matching the historical
    // `when_key.is_none()` filter.
    let groups = filter_commands(&commands, &recent_ids, &query, |_| false);
    let flat_count: usize = groups.iter().map(|g| g.commands.len()).sum();
    let total_commands = commands.len();

    let empty_msg = if query.trim().is_empty() {
        format!("{} commands available", total_commands)
    } else {
        "No matching commands".to_string()
    };

    rsx! {
        div {
            style: "position: fixed; inset: 0; z-index: 60; display: flex; justify-content: center; padding-top: 10vh;",

            // Backdrop — same scrim animation the modal system uses.
            div {
                style: "position: absolute; inset: 0; background: color-mix(in srgb, var(--bg) 60%, transparent); backdrop-filter: blur(8px); -webkit-backdrop-filter: blur(8px); animation: scrim-in var(--dur) var(--ease) both;",
                onclick: move |_| command_state.write().close(),
            }

            // Palette container
            div {
                class: "pane-astrolabe-mark",
                style: "position: relative; z-index: 1; width: 560px; max-width: calc(100vw - 32px); max-height: 440px; display: flex; flex-direction: column; overflow: hidden; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-lg); box-shadow: var(--shadow-lg); animation: modal-rise var(--dur) var(--ease) both;",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "Command palette",

                // Search input
                div {
                    style: "display: flex; align-items: center; gap: 10px; padding: 13px 16px; border-bottom: 1px solid var(--border);",

                    IconSearch { size: Some(15), color: Some("var(--textDim)".to_string()) }

                    input {
                        style: "flex: 1; background: transparent; border: none; outline: none; font-size: 15px; color: var(--text); font-family: var(--font-ui); caret-color: var(--accent);",
                        role: "searchbox",
                        "aria-label": "Search commands",
                        value: "{query}",
                        oninput: move |e| {
                            command_state.write().set_query(e.value());
                            selected_idx.set(0);
                        },
                onkeydown: move |e: KeyboardEvent| {
                    let key = e.key();
                    match key {
                        Key::ArrowDown => {
                            selected_idx.set((selected_idx() + 1).min(flat_count.saturating_sub(1)));
                        }
                        Key::ArrowUp => {
                            selected_idx.set(selected_idx().saturating_sub(1));
                        }
                        Key::Enter => {
                            // Find the command at selected_idx in the flat list
                            let idx = selected_idx();
                            let mut running = 0usize;
                            let mut found_cmd: Option<Command> = None;
                            for group in &groups {
                                for cmd in &group.commands {
                                    if running == idx {
                                        found_cmd = Some(cmd.clone());
                                        break;
                                    }
                                    running += 1;
                                }
                                if found_cmd.is_some() { break; }
                            }
                            if let Some(cmd) = found_cmd {
                                command_state.write().record_execution(&cmd.id);
                                // Dispatch the command action through the handler
                                // registry keyed by `cmd.handler_key`.
                                dispatch_command(&handlers, &cmd.handler_key);
                            }
                            command_state.write().close();
                        }
                        Key::Escape => {
                            command_state.write().close();
                        }
                        _ => {}
                    }
                },
                        placeholder: "Type a command or search...",
                        spellcheck: false,
                        autocomplete: "off",
                        autofocus: true,
                    }

                    div {
                        style: "display: flex; align-items: center; gap: 8px; flex-shrink: 0;",

                        if !query.trim().is_empty() && flat_count > 0 {
                            span {
                                style: "font-size: 11px; color: var(--textDim); font-family: var(--font-ui);",
                                "{flat_count} results"
                            }
                        }

                        kbd {
                            style: "{KBD_STYLE}",
                            "esc"
                        }
                    }
                }

                // Command list
                div {
                    style: "flex: 1; overflow-y: auto; padding: 6px 8px;",

                    if flat_count == 0 {
                        div {
                            style: "display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 40px; color: var(--textDim);",
                            IconSearch { size: Some(28), color: Some("var(--textDim)".to_string()) }
                            span {
                                style: "font-size: var(--text-sm); color: var(--textMuted);",
                                "{empty_msg}"
                            }
                        }
                    } else {
                        {
                            let groups_clone = groups.clone();
                            let mut running_idx = 0usize;
                            let mut items = Vec::new();
                            for group in groups_clone {
                                let group_label = group.label.clone();
                                items.push(rsx! {
                                    div {
                                        key: "group-{group_label}",
                                        style: "padding: 12px 12px 6px 12px; font-family: var(--font-display); font-size: 10px; font-weight: 700; color: var(--textMuted); text-transform: uppercase; letter-spacing: 0.12em;",
                                        "{group_label}"
                                    }
                                });
                                for cmd in group.commands.iter() {
                                    let idx = running_idx;
                                    running_idx += 1;
                                    let is_selected = idx == selected_idx();
                                    let shortcut_str = cmd.shortcut.as_ref().map(|s| format_shortcut(s));
                                    let icon_color = if is_selected { "var(--accent)" } else { "var(--textDim)" };
                                    let cmd_text_color = "var(--text)";
                                    let row_bg = if is_selected { "color-mix(in srgb, var(--accent) 10%, transparent)" } else { "transparent" };
                                    let cmd_id = cmd.id.clone();
                                    let cmd_label = cmd.label.clone();
                                    let cmd_desc = cmd.description.clone();
                                    let icon_el = palette_icon(cmd, icon_color);
                                    items.push(rsx! {
                                        button {
                                            key: "{cmd_id}",
                                            style: "display: flex; align-items: center; gap: 12px; padding: 9px 12px; width: 100%; text-align: left; border: none; border-radius: 6px; background: {row_bg}; cursor: pointer; font-size: var(--text-sm); color: {cmd_text_color};",
                                            onmouseenter: move |_| selected_idx.set(idx),
                                            onclick: {
                                                let handlers = handlers.clone();
                                                let cmd = cmd.clone();
                                                move |_| {
                                                    command_state.write().record_execution(&cmd.id);
                                                    dispatch_command(&handlers, &cmd.handler_key);
                                                    command_state.write().close();
                                                }
                                            },

                                            span {
                                                style: "display: inline-flex; align-items: center; justify-content: center; width: 20px; flex-shrink: 0;",
                                                {icon_el}
                                            }

                                            span {
                                                style: "flex: 1; display: flex; flex-direction: column; gap: 1px; min-width: 0;",

                                                span {
                                                    style: "font-size: 13.5px; font-weight: 500; white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                    "{cmd_label}"
                                                }

                                                if let Some(desc) = cmd_desc {
                                                    span {
                                                        style: "font-size: 11px; color: var(--textDim); white-space: nowrap; overflow: hidden; text-overflow: ellipsis;",
                                                        "{desc}"
                                                    }
                                                }
                                            }

                                            if let Some(sc) = &shortcut_str {
                                                kbd {
                                                    style: "{KBD_STYLE}",
                                                    "{sc}"
                                                }
                                            }
                                        }
                                    });
                                }
                            }
                            rsx! { {items.into_iter()} }
                        }
                    }
                }

                // Footer
                div {
                    style: "display: flex; align-items: center; gap: 14px; padding: 8px 16px; border-top: 1px solid var(--border); font-size: 11px; color: var(--textDim);",

                    span {
                        style: "display: inline-flex; align-items: center; gap: 5px;",
                        kbd { style: "{KBD_STYLE}", "\u{2191}\u{2193}" }
                        " navigate"
                    }

                    span {
                        style: "display: inline-flex; align-items: center; gap: 5px;",
                        kbd { style: "{KBD_STYLE}", "\u{21b5}" }
                        " run"
                    }

                    span {
                        style: "margin-left: auto; opacity: 0.6;",
                        "{total_commands} commands"
                    }
                }
            }
        }
    }
}
