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
                style: "padding-bottom: 12px; border-bottom: 1px solid var(--border); margin-bottom: 8px;",
                div {
                    style: "font-family: var(--font-display); font-size: var(--text-lg); font-weight: 600; color: var(--text); letter-spacing: 0.01em;",
                    "Keyboard Shortcuts"
                }
                div {
                    style: "font-size: var(--text-xs); color: var(--textDim); margin-top: 4px; line-height: 1.5;",
                    "Quick reference for the most common keyboard shortcuts in Athena."
                }
            }

            for (category, shortcuts) in groups.iter() {
                div {
                    key: "{category}",
                    style: "display: flex; flex-direction: column; gap: 6px;",

                    div {
                        style: "font-family: var(--font-display); font-size: var(--text-md); font-weight: 600; color: var(--text); margin-bottom: 8px; letter-spacing: 0.01em;",
                        "{category}"
                    }

                    div {
                        style: "display: flex; flex-direction: column; border: 1px solid var(--border); border-radius: var(--radius-md); overflow: hidden;",

                        for (key, desc) in shortcuts.iter() {
                            div {
                                key: "{key}",
                                style: "display: flex; align-items: center; justify-content: space-between; padding: 12px 16px; background: var(--bgSecondary); border-bottom: 1px solid var(--border); transition: background 0.15s ease;",
                                // Hover effect needs JS or a CSS class — keep inline for now or use a class if defined.
                                // Striping for readability:
                                // Since we can't easily do nth-child in inline styles without complex JS,
                                // we just keep the solid background.

                                span {
                                    style: "font-size: var(--text-sm); color: var(--text); padding-right: 12px;",
                                    "{desc}"
                                }

                                kbd {
                                    style: "padding: 4px 10px; border-radius: var(--radius-md); background: var(--bgTertiary); border: 1px solid var(--border); font-size: var(--text-xs); color: var(--accent); font-family: var(--fontFamily); font-weight: 600; box-shadow: 0 1px 0 rgba(255,255,255,0.04); flex-shrink: 0;",
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
