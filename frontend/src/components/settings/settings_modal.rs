use super::shortcuts_ref::ShortcutsRef;
use super::theme_picker::ThemePicker;
use crate::components::shared::icon::{
    IconAgents, IconAthena, IconInfo, IconKeyboard, IconSettings, IconSmartphone, IconTune,
};
use crate::components::shared::modal::Modal;
use dioxus::prelude::*;
use wasm_bindgen::JsCast;

#[path = "agents_settings.rs"]
mod agents_settings;
use agents_settings::AgentsSettings;

#[path = "settings_controls.rs"]
mod settings_controls;
use settings_controls::{FontDropdown, LabeledField, SizeStepper};
pub(crate) use settings_controls::{GroupLabel, Toggle};

#[path = "settings_sections.rs"]
mod settings_sections;
use settings_sections::{AboutSettings, AthenaSettings, GeneralSettings, MobileMirrorSettings};

/// Icon for a settings section, shown in the floating index (replaces the
/// former unicode glyph set).
fn section_glyph(idx: usize) -> Element {
    match idx {
        0 => rsx! { IconSettings { size: Some(13), color: Some("currentColor".to_string()) } },
        1 => rsx! { IconAthena { size: Some(13), color: Some("currentColor".to_string()) } },
        2 => rsx! { IconAgents { size: Some(13), color: Some("currentColor".to_string()) } },
        3 => rsx! { IconTune { size: Some(13), color: Some("currentColor".to_string()) } },
        4 => rsx! { IconKeyboard { size: Some(13), color: Some("currentColor".to_string()) } },
        5 => rsx! { IconInfo { size: Some(13), color: Some("currentColor".to_string()) } },
        _ => rsx! { IconSmartphone { size: Some(13), color: Some("currentColor".to_string()) } },
    }
}

/* =============================================================
SettingsContent – the codex of settings (seven sections + floating index)
============================================================= */

#[derive(Props, Clone, PartialEq)]
pub struct SettingsContentProps {
    /// In a Modal the dialog chrome already renders its own "Settings"
    /// header — the interior masthead would duplicate it. The standalone
    /// panel variant keeps it.
    #[props(default = true)]
    pub show_masthead: bool,
}

#[component]
pub fn SettingsContent(props: SettingsContentProps) -> Element {
    // 0..=5 — the topmost visible section index. Updated by the scroll
    // listener (Task 7). Initial value 0 (General) so the index shows
    // item one as active before the first scroll event.
    //
    // The signal handle is `let mut` because clones of it (e.g. `let mut onidx =
    // active_idx.clone();` inside the index-button onclick) need to call `.set()`.
    // The binding itself is not mutated here — see the `// mut` note below.
    #[allow(unused_mut)]
    let mut active_idx = use_signal(|| 0u8);

    // ── Scroll listener ───────────────────────────────────────
    // Installs a one-shot 'scroll' listener on #codex-tome-scroll. For each
    // section root (s-i..s-vi) the listener reads the DOM rect, picks the
    // section whose top is closest-above the scroller's own top, and writes
    // that index into `active_idx`. The Codex-index reads `active_idx` and
    // renders the matching numeral.
    //
    // The listener is torn down on unmount via `use_drop` (Spec §3.3 /
    // project-redesign-mount-panic-hooks — without explicit removal the
    // listener would keep firing and writing into a stale signal after
    // the modal closes).
    //
    // Storage pattern: A component-scoped `Rc<RefCell<Option<...>>>` slot
    // holds the (scroller_element, closure) pair. The `use_effect` installs
    // the listener and stashes the pair; the `use_drop` reads it back out
    // and removes the JS-side listener on unmount, freeing the closure.
    //
    // Implementation constraints (Dioxus 0.7 hooks-at-mount):
    //   - `use_signal`/`use_effect`/`use_hook`/`use_drop` only inside component body.
    //   - The scroll closure captures a `Signal<u8>` clone of `active_idx`.
    //   - No hooks are called inside the scroll closure.
    let scroll_listener: ::std::rc::Rc<
        ::std::cell::RefCell<
            Option<(
                web_sys::Element,
                wasm_bindgen::closure::Closure<dyn FnMut()>,
            )>,
        >,
    > = use_hook(|| ::std::rc::Rc::new(::std::cell::RefCell::new(None)));

    use_effect({
        let scroll_listener = ::std::rc::Rc::clone(&scroll_listener);
        move || {
            let mut active_idx = active_idx;
            let Some(window) = web_sys::window() else {
                return;
            };
            let Some(document) = window.document() else {
                return;
            };
            let Some(scroller) = document.get_element_by_id("codex-tome-scroll") else {
                return;
            };
            let Ok(scroller_el) = scroller.dyn_into::<web_sys::Element>() else {
                return;
            };

            // Defensive: if a prior listener exists (rebind path), remove it.
            if let Some((old_scroller, old_cl)) = scroll_listener.borrow_mut().take() {
                let _ = old_scroller
                    .remove_event_listener_with_callback("scroll", old_cl.as_ref().unchecked_ref());
            }

            let cl = wasm_bindgen::closure::Closure::wrap(Box::new(move || {
                let Some(window) = web_sys::window() else {
                    return;
                };
                let Some(document) = window.document() else {
                    return;
                };
                let Some(scroller) = document.get_element_by_id("codex-tome-scroll") else {
                    return;
                };
                let Ok(scroller_el) = scroller.dyn_into::<web_sys::Element>() else {
                    return;
                };
                let rect = scroller_el.get_bounding_client_rect();
                let scroller_top = rect.top();

                // For each section root, pick the one whose top is closest-
                // above/on the scroller's own top. If the scroller has not
                // scrolled (everything still above the fold), the first
                // section wins.
                let mut best_idx: u8 = 0;
                let mut best_top: f64 = f64::NEG_INFINITY;
                let ids = ["s-i", "s-ii", "s-iii", "s-iv", "s-v", "s-vi", "s-vii"];
                for (i, id) in ids.iter().enumerate() {
                    let Some(el) = document.get_element_by_id(id) else {
                        continue;
                    };
                    let Ok(el) = el.dyn_into::<web_sys::Element>() else {
                        continue;
                    };
                    let rect = el.get_bounding_client_rect();
                    let top = rect.top();
                    if top <= scroller_top + 1.0 && top > best_top {
                        best_top = top;
                        best_idx = i as u8;
                    }
                }
                active_idx.set(best_idx);
            }) as Box<dyn FnMut()>);

            let _ =
                scroller_el.add_event_listener_with_callback("scroll", cl.as_ref().unchecked_ref());

            // Stash for the unmount-cleanup hook to consume later.
            *scroll_listener.borrow_mut() = Some((scroller_el, cl));
        }
    });

    // Cleanup on unmount: take the scroller+closure pair out of the slot
    // and remove the JS-side listener. Once the closure (and its slot entry)
    // drops here, the wasm_bindgen JS callback handle is freed.
    use_drop({
        let scroll_listener = ::std::rc::Rc::clone(&scroll_listener);
        move || {
            if let Some((scroller_el, cl)) = scroll_listener.borrow_mut().take() {
                let _ = scroller_el
                    .remove_event_listener_with_callback("scroll", cl.as_ref().unchecked_ref());
            }
        }
    });

    let section_i = rsx! {
        CodexSection {
            numeral: "I",
            title: "General",
            intro: Some("Configure your environment — type, size, pane titles."),
            id: "s-i",
            GeneralSettings {}
        }
    };
    let section_ii = rsx! {
        CodexSection {
            numeral: "II",
            title: "Athena",
            intro: Some("Configure your LLM provider. Works with any OpenAI-compatible API or Anthropic."),
            id: "s-ii",
            AthenaSettings {}
        }
    };
    let section_iii = rsx! {
        CodexSection {
            numeral: "III",
            title: "Agents",
            intro: Some("Manage custom agents with aliases and commands that launch them."),
            id: "s-iii",
            AgentsSettings {}
        }
    };
    let section_iv = rsx! {
        CodexSection {
            numeral: "IV",
            title: "Themes",
            intro: Some("Choose a color scheme for your environment."),
            id: "s-iv",
            ThemePicker {}
        }
    };
    let section_v = rsx! {
        CodexSection {
            numeral: "V",
            title: "Shortcuts",
            intro: Some("Quick reference for the most common keyboard shortcuts."),
            id: "s-v",
            ShortcutsRef {}
        }
    };
    let section_vi = rsx! {
        CodexSection {
            numeral: "VI",
            title: "About",
            intro: Some(""),
            id: "s-vi",
            AboutSettings {}
        }
    };
    let section_vii = rsx! {
        CodexSection {
            numeral: "VII",
            title: "Mobile Mirror",
            intro: Some("Mirror this desktop to your phone over the local network."),
            id: "s-vii",
            MobileMirrorSettings {}
        }
    };

    let sections = [
        section_i,
        section_ii,
        section_iii,
        section_iv,
        section_v,
        section_vi,
        section_vii,
    ];
    let numerals: [&'static str; 7] = ["I", "II", "III", "IV", "V", "VI", "VII"];

    rsx! {
        div {
            class: "pane-astrolabe-mark",
            style: "display: flex; flex-direction: column; height: 100%; overflow: hidden; background: var(--bgSecondary); color: var(--text);",

            /* ── Interior masthead (decorative; modal close button is owned by Modal) ── */
            if props.show_masthead {
            div {
                style: "display: flex; align-items: center; gap: 12px; padding: 18px 24px; border-bottom: 1px solid var(--border); background: var(--bgSecondary); flex-shrink: 0;",
                span {
                    class: "seal-mark",
                    style: "opacity: 0.95;",
                    IconAthena { size: Some(22), color: Some("var(--accent)".to_string()) }
                }
                span {
                    style: "font-family: var(--font-display); font-size: 20px; font-weight: 600; color: var(--accent); letter-spacing: 0.03em;",
                    "Settings"
                }
                span {
                    style: "margin-left: auto; color: var(--textDim); font-size: 10px; letter-spacing: 0.16em; text-transform: uppercase;",
                    "Workspace preferences"
                }
            }
            }

            /* ── Body: index on the left, scroll tome on the right ── */
            div {
                style: "display: flex; flex: 1; min-height: 0;",

                /* Floating left index (sticky) */
                div { class: "codex-index",
                    for (idx, _section) in sections.iter().enumerate() {
                        {
                            let idx_u8 = idx as u8;
                            let active = active_idx() == idx_u8;
                            let cls = if active { "codex-index-item is-active" } else { "codex-index-item" };
                            // `mut` is required: Signal::set takes &mut.
                            let mut onidx = active_idx;
                            let section_id = match idx_u8 {
                                0 => "s-i",
                                1 => "s-ii",
                                2 => "s-iii",
                                3 => "s-iv",
                                4 => "s-v",
                                5 => "s-vi",
                                _ => "s-vii",
                            };
                            let section_id_for_click = section_id.to_string();
                            rsx! {
                                button {
                                    key: "{idx}",
                                    class: "{cls}",
                                    r#type: "button",
                                    aria_label: "Jump to section {numerals[idx]}",
                                    onclick: move |_| {
                                        onidx.set(idx_u8);
                                        if let Some(window) = web_sys::window() {
                                            if let Some(doc) = window.document() {
                                                if let Some(el) = doc.get_element_by_id(&section_id_for_click) {
                                                    el.scroll_into_view();
                                                }
                                            }
                                        }
                                    },
                                    span { "{numerals[idx]}" }
                                    span { class: "glyph", {section_glyph(idx)} }
                                }
                            }
                        }
                    }
                }

                /* Scroll tome */
                div {
                    id: "codex-tome-scroll",
                    class: "codex-tome",
                    for (idx, sec) in sections.iter().enumerate() {
                        // Each element is already a CodexSection-wrapped <section> with id s-i..s-vi.
                        // The binding is named `sec` (not `section`) because `section` is
                        // a reserved rsx! element-name and would shadow the value-binder.
                        {
                            let _ = idx;
                            sec
                        }
                    }
                }
            }
        }
    }
}

/* =============================================================
SettingsModal – wraps SettingsContent in a modal overlay
============================================================= */

#[derive(Props, Clone, PartialEq)]
pub struct SettingsModalProps {
    pub on_close: EventHandler<()>,
}

#[component]
pub fn SettingsModal(props: SettingsModalProps) -> Element {
    rsx! {
        Modal {
            title: "Settings",
            on_close: move |_| props.on_close.call(()),
            width: 860,
            SettingsContent { show_masthead: false }
        }
    }
}

/* =============================================================
Settings — shared presentation primitives
============================================================= */

#[derive(Props, Clone, PartialEq)]
struct CodexSectionProps {
    /// Roman numeral shown in the section header, e.g. "I", "II".
    numeral: &'static str,
    /// Title text shown next to the numeral in --text + --font-display.
    title: &'static str,
    /// Optional intro line under the rule (--textMuted, --text-base).
    intro: Option<&'static str>,
    /// DOM id used by the floating index to scroll/jump-active. e.g. "s-i".
    id: &'static str,
    children: Element,
}

#[component]
fn CodexSection(props: CodexSectionProps) -> Element {
    rsx! {
        section {
            class: "codex-section",
            id: "{props.id}",
            div {
                class: "codex-section-head",
                span { class: "codex-section-num", "{props.numeral}." }
                span { class: "codex-section-title", "{props.title}" }
            }
            if let Some(intro) = props.intro {
                div { class: "codex-section-intro", "{intro}" }
            } else {
                div { class: "codex-section-intro" } /* keeps spacing rhythm */
            }
            hr { class: "codex-rule" }
            {props.children}
        }
    }
}
