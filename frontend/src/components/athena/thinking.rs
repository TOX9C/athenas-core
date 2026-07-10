use crate::components::shared::illustration::OwlMark;
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

    // Animated dots via periodic re-render. The future is cancelled
    // on component unmount via use_drop to prevent leaks.
    let mut animation = use_future(move || async move {
        loop {
            gloo::timers::future::TimeoutFuture::new(500).await;
            dots.set((dots() + 1) % 4);
        }
    });

    // Cancel the animation loop when this component unmounts.
    use_drop(move || {
        animation.cancel();
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
            style: "display: flex; align-items: center; gap: 10px; padding: 8px 12px; background: transparent; border-radius: var(--radius-md); border: none;",

            // Brand owl — the thinking dot traces its orbital halo while Athena is streaming.
            span {
                style: "display: inline-flex; align-items: center; justify-content: center;",
                OwlMark { size: Some(14) }
            }

            div {
                style: "display: flex; flex-direction: column; gap: 3px;",

                span {
                    style: "font-size: var(--text-xs); font-weight: 500; letter-spacing: 0.02em; color: var(--textMuted);",
                    "{status_label}{dot_str}"
                }

                // Pulse bar — lapis-tinted track with an accent sweep.
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
