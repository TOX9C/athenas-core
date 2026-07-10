use dioxus::prelude::*;

#[component]
pub fn ShortcutsRef() -> Element {
    let groups: [(&str, &[(&str, &str)]); 3] = [
        (
            "Workspace",
            &[
                ("\u{2318}T", "New terminal"),
                ("\u{2318}J", "Toggle sidebar"),
                ("\u{2318}1-9", "Switch panel"),
            ],
        ),
        (
            "Command",
            &[
                ("\u{2318}K", "Command palette"),
                ("\u{2318}\u{21e7}P", "Command palette (alt)"),
                ("\u{2318}\u{21e7}S", "Settings"),
            ],
        ),
        (
            "General",
            &[
                ("Escape", "Close modal/palette"),
                ("\u{2318}\u{21e7}R", "Refresh"),
            ],
        ),
    ];

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 32px;",

            // Header
            div {
                style: "padding-bottom: 12px; margin-bottom: 8px;",
                div {
                    style: "display: flex; align-items: center; gap: 8px;",
                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                        "Keyboard Shortcuts"
                    }
                }
                div {
                    style: "font-size: var(--text-xs); color: var(--textDim); margin-top: 4px; line-height: 1.5; padding-left: 18px;",
                    "Quick reference for the most common keyboard shortcuts in Athena."
                }
                hr { class: "great-circle-rule", style: "margin-top: 8px;" }
            }

            for (category, shortcuts) in groups.iter() {
                div {
                    key: "{category}",
                    style: "display: flex; flex-direction: column; gap: 6px;",

                    div {
                        style: "display: flex; align-items: center; gap: 6px; margin-bottom: 8px;",
                        div {
                            style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; color: var(--accent); letter-spacing: 0.04em;",
                            "{category}"
                        }
                    }

                    div {
                        style: "display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden;",

                        for (key, desc) in shortcuts.iter() {
                            div {
                                key: "{key}",
                                class: "lit-sweep",
                                style: "display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; background: var(--bgSecondary); border-bottom: 1px solid var(--border);",

                                span {
                                    style: "font-size: var(--text-sm); color: var(--text); padding-right: 12px;",
                                    "{desc}"
                                }

                                kbd {
                                    style: "padding: 4px 10px; border-radius: var(--radius-md); background: var(--bgTertiary); border: 1px solid var(--border); font-size: var(--text-xs); color: var(--accent); font-family: var(--fontFamily); font-weight: 600; flex-shrink: 0;",
                                    "{key}"
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
