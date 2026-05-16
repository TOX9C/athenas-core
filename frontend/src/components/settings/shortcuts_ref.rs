use dioxus::prelude::*;

#[component]
pub fn ShortcutsRef() -> Element {
    let shortcuts = [
        ("\u{2318}T", "New terminal"),
        ("\u{2318}J", "Toggle sidebar"),
        ("\u{2318}K", "Command palette"),
        ("\u{2318}\u{21e7}P", "Command palette (alt)"),
        ("\u{2318}\u{21e7}S", "Settings"),
        ("\u{2318}1-9", "Switch panel"),
        ("Escape", "Close modal/palette"),
        ("\u{2318}\u{21e7}R", "Refresh"),
    ];

    rsx! {
        div {
            style: "display: flex; flex-direction: column; gap: 6px;",

            for (key, desc) in shortcuts.iter() {
                div {
                    key: "{key}",
                    style: "display: flex; align-items: center; justify-content: space-between; padding: 4px 0; border-bottom: 1px solid var(--border);",

                    span {
                        style: "font-size: 10px; color: var(--textDim);",
                        "{desc}"
                    }

                    kbd {
                        style: "padding: 2px 6px; border-radius: 4px; background: var(--bgTertiary); border: 1px solid var(--border); font-size: 10px; color: var(--text); font-family: inherit;",
                        "{key}"
                    }
                }
            }
        }
    }
}
