use crate::stores::workspace::GridTemplate;
use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct GridTemplateSelectorProps {
    pub selected: GridTemplate,
    pub on_select: EventHandler<GridTemplate>,
}

#[component]
pub fn GridTemplateSelector(props: GridTemplateSelectorProps) -> Element {
    let templates = [
        GridTemplate::X1x1,
        GridTemplate::X1x2,
        GridTemplate::X2x2,
        GridTemplate::X2x3,
        GridTemplate::X3x3,
        GridTemplate::X3x4,
        GridTemplate::X4x4,
    ];

    rsx! {
        div {
            style: "display: flex; gap: 8px; margin-top: 4px; flex-wrap: wrap;",

            for tmpl in templates.iter() {
                {
                    let tmpl_val = *tmpl;
                    let is_selected = tmpl_val == props.selected;
                    let (cols, rows) = match tmpl_val {
                        GridTemplate::X1x1 => (1, 1),
                        GridTemplate::X1x2 => (2, 1),
                        GridTemplate::X2x2 => (2, 2),
                        GridTemplate::X2x3 => (3, 2),
                        GridTemplate::X3x3 => (3, 3),
                        GridTemplate::X3x4 => (4, 3),
                        GridTemplate::X4x4 => (4, 4),
                    };
                    let label = format!("{}x{}", cols, rows);
                    let border = if is_selected { "2px solid var(--accent)" } else { "1px solid var(--border)" };
                    let bg = if is_selected { "var(--accent)" } else { "var(--bgTertiary)" };
                    let color = if is_selected { "#fff" } else { "var(--textDim)" };
                    rsx! {
                        button {
                            key: "{label}",
                            style: "width: 48px; height: 48px; border-radius: 6px; border: {border}; background: {bg}; color: {color}; cursor: pointer; display: flex; flex-direction: column; align-items: center; justify-content: center; gap: 2px; font-size: 10px; font-weight: 600;",
                            onclick: move |_| props.on_select.call(tmpl_val),

                            // Mini grid preview
                            div {
                                style: "display: grid; grid-template-columns: repeat({cols}, 1fr); grid-template-rows: repeat({rows}, 1fr); gap: 1px; width: 24px; height: 24px;",
                                for _i in 0..(cols * rows) {
                                    div {
                                        key: "{_i}",
                                        style: "background: {color}; border-radius: 1px; opacity: 0.5;",
                                    }
                                }
                            }

                            "{label}"
                        }
                    }
                }
            }
        }
    }
}
