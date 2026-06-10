use dioxus::prelude::*;

/// A segmented control — replaces bare `<select>`s and ad-hoc toggle-button rows.
/// Reports the selected option index via `on_select`.
#[derive(Props, Clone, PartialEq)]
pub struct SegmentedProps {
    pub options: Vec<String>,
    pub selected: usize,
    pub on_select: EventHandler<usize>,
}

#[component]
pub fn Segmented(props: SegmentedProps) -> Element {
    rsx! {
        div { class: "segmented",
            for (i, opt) in props.options.iter().enumerate() {
                button {
                    key: "{i}",
                    class: if i == props.selected { "segmented-item is-active" } else { "segmented-item" },
                    onclick: move |_| props.on_select.call(i),
                    "{opt}"
                }
            }
        }
    }
}
