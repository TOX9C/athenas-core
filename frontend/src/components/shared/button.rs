use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ButtonSize {
    #[default]
    Md,
    Sm,
}

#[derive(Props, Clone, PartialEq)]
pub struct ButtonProps {
    #[props(default = ButtonVariant::Primary)]
    pub variant: ButtonVariant,
    #[props(default = ButtonSize::Md)]
    pub size: ButtonSize,
    #[props(default = false)]
    pub disabled: bool,
    pub on_click: EventHandler<MouseEvent>,
    pub children: Element,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let bg = match props.variant {
        ButtonVariant::Primary => "var(--accent)",
        ButtonVariant::Secondary => "var(--bgTertiary)",
        ButtonVariant::Ghost => "transparent",
    };
    let color = match props.variant {
        ButtonVariant::Primary => "#fff",
        ButtonVariant::Secondary => "var(--text)",
        ButtonVariant::Ghost => "var(--textMuted)",
    };
    let border = match props.variant {
        ButtonVariant::Primary => "none",
        ButtonVariant::Secondary => "1px solid var(--border)",
        ButtonVariant::Ghost => "1px solid transparent",
    };
    let padding = match props.size {
        ButtonSize::Sm => "4px 10px",
        ButtonSize::Md => "6px 16px",
    };
    let font_size = match props.size {
        ButtonSize::Sm => "10px",
        ButtonSize::Md => "12px",
    };
    let opacity = if props.disabled { "0.5" } else { "1" };

    rsx! {
        button {
            style: "background: {bg}; color: {color}; border: {border}; border-radius: 6px; padding: {padding}; font-size: {font_size}; font-weight: 500; cursor: pointer; opacity: {opacity}; transition: opacity 0.15s ease; display: inline-flex; align-items: center; gap: 4px;",
            disabled: props.disabled,
            onclick: move |e| props.on_click.call(e),
            {props.children}
        }
    }
}
