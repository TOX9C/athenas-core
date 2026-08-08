use wasm_bindgen::JsCast;

#[path = "definitions.rs"]
mod definitions;

pub use definitions::{
    get_theme, ThemeColors, ThemeDefinition, ThemeName, ThemeType, ALL_THEMES, AVAILABLE_FONTS,
};

/// Apply a palette to the document root as CSS custom properties, including the
/// atmosphere tokens (lamp glow + grain). Unlike the previous engine, this drives
/// the full token set so light/dark themes carry correct hover/border/atmosphere.
pub fn apply_theme_to_dom(theme_name: &str) {
    let c = get_theme(theme_name);
    set_data_theme(theme_name);

    let derived_ring = c
        .accent_subtle
        .replace("0.12", "0.55")
        .replace("0.13", "0.55")
        .replace("0.14", "0.55");
    let props: [(&str, &str); 23] = [
        ("--bg", &c.bg),
        ("--bgSecondary", &c.bg_secondary),
        ("--bgTertiary", &c.bg_tertiary),
        ("--bgHover", &c.bg_hover),
        ("--border", &c.border),
        ("--borderActive", &c.border_active),
        ("--text", &c.text),
        ("--textMuted", &c.text_muted),
        ("--textDim", &c.text_dim),
        ("--accent", &c.accent),
        ("--accentHover", &c.accent_hover),
        ("--accentSubtle", &c.accent_subtle),
        ("--accentTeal", &c.accent_teal),
        ("--ring", &derived_ring),
        ("--success", &c.success),
        ("--error", &c.error),
        ("--warning", &c.warning),
        ("--terminalBg", &c.terminal_bg),
        ("--terminalFg", &c.terminal_fg),
        ("--terminalCursor", &c.terminal_cursor),
        ("--terminalSelection", &c.terminal_selection),
        ("--themeGlowColor", &c.glow_color),
        ("--themeGlowOpacity", "1"),
    ];
    for (k, v) in props {
        set_css_property(k, v);
    }
    set_css_property("--themeNoiseOpacity", &format!("{}", c.noise_opacity));
}

/// Set a CSS custom property on the document root via typed DOM bindings.
///
/// This used to build a JS string and run it through `js_sys::Function::new_no_args`
/// (i.e. `new Function(code)`), which the JS engine treats as `eval`. The app's
/// Content-Security-Policy only allows `'self' 'wasm-unsafe-eval'`, so that threw
/// `EvalError: Refused to evaluate a string as JavaScript` on every theme apply —
/// including the mount-time apply, which aborted the Dioxus runtime with a
/// `RefCell already borrowed` panic and left the UI unable to re-render.
/// `CssStyleDeclaration::set_property` performs the identical DOM mutation but
/// never invokes the JS parser, so it needs no CSP relaxation.
fn set_css_property(property: &str, value: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(html_el) = document.document_element() else {
        return;
    };
    // `style` is defined on `HtmlElement`, not `Element`; `document_element`
    // returns the latter, so downcast before accessing the inline style.
    let Some(html) = html_el.dyn_ref::<web_sys::HtmlElement>() else {
        return;
    };
    let style = html.style();
    let _ = style.set_property(property, value);
}

/// Set `data-theme` on the document root. Same reasoning as `set_css_property`:
/// use `Element::set_attribute` instead of `eval`-ing a JS string.
fn set_data_theme(value: &str) {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(document) = window.document() else {
        return;
    };
    let Some(html) = document.document_element() else {
        return;
    };
    let _ = html.set_attribute("data-theme", value);
}

/// Apply the user-overridable mono font + base size. Display/UI families are fixed
/// in CSS; only the terminal/code mono face is user-configurable.
pub fn apply_font_to_dom(font_family: &str, font_size: u8) {
    set_css_property(
        "--fontFamily",
        &format!(
            "'{}', 'Monaspace Neon', ui-monospace, monospace",
            font_family
        ),
    );
    set_css_property("--fontSize", &format!("{}px", font_size));
}
