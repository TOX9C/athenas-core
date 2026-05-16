use dioxus::prelude::*;

#[derive(Props, Clone, PartialEq)]
pub struct ThinkingProps {
    #[props(default = None)]
    pub status: Option<String>,
}

#[component]
pub fn AthenaThinkingIndicator(props: ThinkingProps) -> Element {
    let status_label = props.status.as_deref().unwrap_or("Thinking");
    let mut dots = use_signal(|| 0u8);

    // Simple animated dots via periodic re-render using gloo timers
    use_future(move || async move {
        loop {
            gloo::timers::future::TimeoutFuture::new(500).await;
            dots.set((dots() + 1) % 4);
        }
    });

    let dot_str = match dots() {
        0 => ".",
        1 => "..",
        2 => "...",
        _ => "",
    };

    rsx! {
        div {
            class: "thinking-indicator",
            style: "display: flex; align-items: center; gap: 8px; padding: 8px 12px; background: var(--bgSecondary); border-radius: 8px; border: 1px solid var(--border);",

            // AI indicator with pulse animation
            div {
                style: "width: 20px; height: 20px; border-radius: 6px; background: #38bdf822; display: flex; align-items: center; justify-content: center; animation: pulse 1.5s ease-in-out infinite;",
                span {
                    style: "font-size: 11px; font-weight: 700; color: #38bdf8;",
                    "A"
                }
            }

            div {
                style: "display: flex; flex-direction: column; gap: 2px;",

                span {
                    style: "font-size: 11px; font-weight: 500; color: var(--accent);",
                    "{status_label}{dot_str}"
                }

                // Pulse bar
                div {
                    style: "width: 120px; height: 3px; border-radius: 2px; background: var(--bgTertiary); overflow: hidden;",
                    div {
                        style: "width: 40%; height: 100%; border-radius: 2px; background: var(--accent); animation: pulse 1.5s ease-in-out infinite;",
                    }
                }
            }
        }
    }
}
