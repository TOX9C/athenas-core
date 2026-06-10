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
            style: "display: flex; flex-direction: column; gap: 22px;",

            for (category, shortcuts) in groups.iter() {
                div {
                    key: "{category}",
                    style: "display: flex; flex-direction: column; gap: 4px;",

                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; color: var(--text); margin-bottom: 4px; letter-spacing: 0.01em;",
                        "{category}"
                    }

                    for (key, desc) in shortcuts.iter() {
                        div {
                            key: "{key}",
                            style: "display: flex; align-items: center; justify-content: space-between; padding: 5px 0; border-bottom: 1px solid var(--border);",

                            span {
                                style: "font-size: var(--text-xs); color: var(--textMuted);",
                                "{desc}"
                            }

                            kbd {
                                style: "padding: 2px 6px; border-radius: var(--radius-sm); background: var(--bgTertiary); border: 1px solid var(--border); font-size: var(--text-2xs); color: var(--accent); font-family: var(--fontFamily); font-weight: 600;",
                                "{key}"
                            }
                        }
                    }
                }
            }
        }
    }
}
