use dioxus::prelude::*;

/// A single context-menu entry.
#[derive(Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub danger: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            danger: false,
        }
    }
    pub fn danger(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            danger: true,
        }
    }
}

/// Right-click context menu. Wraps a trigger; on contextmenu it opens a themed
/// menu at the cursor and reports the chosen item index via `on_select`.
#[derive(Props, Clone, PartialEq)]
pub struct ContextMenuProps {
    pub items: Vec<MenuItem>,
    pub on_select: EventHandler<usize>,
    pub children: Element,
}

#[component]
pub fn ContextMenu(props: ContextMenuProps) -> Element {
    let mut open = use_signal(|| false);
    let mut pos = use_signal(|| (0i32, 0i32));
    let items = props.items.clone();

    rsx! {
        div {
            style: "display: contents;",
            oncontextmenu: move |e: MouseEvent| {
                e.prevent_default();
                let c = e.data.client_coordinates();
                pos.set((c.x as i32, c.y as i32));
                open.set(true);
            },
            {props.children}
        }

        if open() {
            // backdrop to dismiss
            div {
                style: "position: fixed; inset: 0; z-index: 9590;",
                onclick: move |_| open.set(false),
                oncontextmenu: move |e: MouseEvent| { e.prevent_default(); open.set(false); },
            }
            div {
                class: "context-menu",
                style: "left: {pos().0}px; top: {pos().1}px;",
                for (i, item) in items.iter().enumerate() {
                    button {
                        key: "{i}",
                        class: if item.danger { "context-menu-item is-danger" } else { "context-menu-item" },
                        onclick: move |_| {
                            open.set(false);
                            props.on_select.call(i);
                        },
                        "{item.label}"
                    }
                }
            }
        }
    }
}
