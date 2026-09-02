use crate::components::shared::icon::{IconChevronRight, IconSearch};
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
            style: "position: fixed; inset: 0; z-index: 60; display: flex; justify-content: center; padding-top: 12vh;",

            // Backdrop
            div {
                style: "position: absolute; inset: 0; background: color-mix(in srgb, var(--bg) 70%, transparent);",
                onclick: move |_| command_state.write().close(),
            }

            // Palette container
            div {
                class: "pane-astrolabe-mark",
                style: "position: relative; z-index: 1; width: 520px; max-height: 400px; display: flex; flex-direction: column; overflow: hidden; background: var(--bgSecondary); border: 1px solid var(--border); border-radius: var(--radius-lg); box-shadow: var(--shadow-lg);",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "Command palette",

                // Search input
                div {
                    style: "display: flex; align-items: center; gap: 8px; padding: 10px 14px; border-bottom: 1px solid var(--border);",

                    IconSearch { size: Some(15), color: Some("var(--textDim)".to_string()) }

                    input {
                        style: "flex: 1; background: transparent; border: none; outline: none; font-size: 14px; color: var(--text); font-family: var(--font-ui); caret-color: var(--accent);",
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
                        placeholder: "Type a command...",
                        spellcheck: false,
                        autocomplete: "off",
                        autofocus: true,
                    }

                    div {
                        style: "display: flex; align-items: center; gap: 4px;",

                        if !query.trim().is_empty() && flat_count > 0 {
                            span {
                                class: "badge",
                                "{flat_count}"
                            }
                        }

                        kbd {
                            style: "font-size: var(--text-2xs); padding: 2px 6px; border-radius: var(--radius-sm); background: var(--bgTertiary); border: 1px solid var(--border); color: var(--textDim); font-family: var(--fontFamily);",
                            "esc"
                        }
                    }
                }

                // Great-circle rule — gold seam between input header and results.
                div { class: "great-circle-rule" }

                // Command list
                div {
                    style: "flex: 1; overflow-y: auto;",

                    if flat_count == 0 {
                        div {
                            style: "display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 36px; color: var(--textDim);",
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
                                        style: "display: flex; align-items: center; gap: 6px; padding: 10px 14px 4px 14px; font-family: var(--font-display); font-size: var(--text-2xs); font-weight: 600; color: var(--accent); text-transform: uppercase; letter-spacing: 0.04em;",
                                        "{group_label}"
                                    }
                                });
                                for cmd in group.commands.iter() {
                                    let idx = running_idx;
                                    running_idx += 1;
                                    let is_selected = idx == selected_idx();
                                    let shortcut_str = cmd.shortcut.as_ref().map(|s| format_shortcut(s));
                                    // selection now carried by text + icon color-shift only (flat-quiet)
                                    let icon_color = if is_selected { "var(--accent)" } else { "var(--textDim)" };
                                    let cmd_text_color = if is_selected { "var(--accent)" } else { "var(--text)" };
                                    let cmd_id = cmd.id.clone();
                                    let cmd_label = cmd.label.clone();
                                    items.push(rsx! {
                                        button {
                                            key: "{cmd_id}",
                                            class: "lit-sweep",
                                            style: "display: flex; align-items: center; gap: 10px; padding: 7px 14px; width: 100%; text-align: left; border: none; background: transparent; cursor: pointer; font-size: var(--text-sm); color: {cmd_text_color};",
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
                                                style: "display: inline-flex; align-items: center; justify-content: center; width: 16px;",
                                                IconChevronRight { size: Some(13), color: Some(icon_color.to_string()) }
                                            }

                                            span {
                                                style: "flex: 1; font-size: var(--text-sm);",
                                                "{cmd_label}"
                                            }

                                            if let Some(ref sc) = shortcut_str {
                                                kbd {
                                                    style: "font-size: var(--text-2xs); padding: 2px 7px; border-radius: var(--radius-sm); background: var(--bgTertiary); border: 1px solid var(--border); color: var(--accent); font-family: var(--fontFamily); display: inline-flex; align-items: center; justify-content: center; min-width: 24px;",
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
                    style: "display: flex; align-items: center; gap: 14px; padding: 8px 14px; border-top: 1px solid var(--border); font-size: var(--text-2xs); color: var(--textDim);",

                    span {
                        kbd { style: "font-size: var(--text-2xs); padding: 2px 5px; border-radius: var(--radius-sm); background: var(--bgTertiary); border: 1px solid var(--border); font-family: var(--fontFamily);", "\u{2191}\u{2193}" }
                        " navigate"
                    }

                    span {
                        kbd { style: "font-size: var(--text-2xs); padding: 2px 5px; border-radius: var(--radius-sm); background: var(--bgTertiary); border: 1px solid var(--border); font-family: var(--fontFamily);", "\u{21b5}" }
                        " execute"
                    }

                    span {
                        kbd { style: "font-size: var(--text-2xs); padding: 2px 5px; border-radius: var(--radius-sm); background: var(--bgTertiary); border: 1px solid var(--border); font-family: var(--fontFamily);", "esc" }
                        " close"
                    }

                    span {
                        style: "margin-left: auto; opacity: 0.5;",
                        "{total_commands} commands"
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::command::{Command, CommandCategory};

    fn cmd(id: &str, label: &str) -> Command {
        Command {
            id: id.to_string(),
            label: label.to_string(),
            category: CommandCategory::Workspace,
            description: None,
            keywords: vec![],
            shortcut: None,
            handler_key: id.to_string(),
            when_key: None,
        }
    }

    /// H8 regression: a multi-byte (non-ASCII) query MUST NOT panic the
    /// subsequence matcher. Previously the matcher did
    /// `lower.chars().nth(qi).unwrap()` where `qi` was a byte index; for any
    /// multi-byte query `qi` exceeds the char count and `nth()` returns
    /// `None`, panicking and aborting the WASM renderer.
    #[test]
    fn fuzzy_match_handles_non_ascii_query_without_panicking() {
        let commands = vec![
            cmd("open", "Open File"),
            cmd("save", "Save Workspace"),
            cmd("term", "New Terminal"),
        ];
        // Accented, CJK, and emoji — all multi-byte in UTF-8. The old code
        // panicked on any of these.
        for q in ["fïlé", "终端", "😀", "café"] {
            // Must not panic. (Result may be empty — that's fine; the point
            // is that non-ASCII input doesn't abort the renderer.)
            let _ = filter_commands(&commands, &[], q, |_| false);
        }
    }

    /// Sanity: ASCII subsequence matching still works after the rewrite.
    #[test]
    fn fuzzy_match_ascii_subsequence_still_works() {
        let commands = vec![cmd("term", "New Terminal"), cmd("save", "Save")];
        // "trm" is a subsequence of "new terminal".
        let groups = filter_commands(&commands, &[], "trm", |_| false);
        assert!(groups
            .iter()
            .any(|g| g.commands.iter().any(|c| c.id == "term")));
    }
}
