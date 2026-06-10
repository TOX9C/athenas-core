use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq, Default)]
pub enum ButtonVariant {
    #[default]
    Primary,
    Secondary,
    Ghost,
    Danger,
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
    #[props(default = false)]
    pub loading: bool,
    pub on_click: EventHandler<MouseEvent>,
    pub children: Element,
}

#[component]
pub fn Button(props: ButtonProps) -> Element {
    let variant_class = match props.variant {
        ButtonVariant::Primary => "btn-primary",
        ButtonVariant::Secondary => "btn-secondary",
        ButtonVariant::Ghost => "btn-ghost",
        ButtonVariant::Danger => "btn-danger",
    };
    let size_class = match props.size {
        ButtonSize::Sm => "btn-sm",
        ButtonSize::Md => "",
    };
    let is_disabled = props.disabled || props.loading;
    let dim = if is_disabled {
        "opacity: 0.5; pointer-events: none;"
    } else {
        ""
    };

    rsx! {
        button {
            class: "{variant_class} {size_class}",
            style: "{dim}",
            disabled: is_disabled,
            onclick: move |e| props.on_click.call(e),
            if props.loading {
                span {
                    style: "width: 12px; height: 12px; border: 2px solid currentColor; border-top-color: transparent; border-radius: 50%; animation: spin 0.7s linear infinite; display: inline-block;",
                }
            }
            {props.children}
        }
    }
}
