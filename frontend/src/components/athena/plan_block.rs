use crate::components::shared::icon::{
    IconCheck, IconChevronDown, IconChevronRight, IconClose, IconKanban,
};
use crate::stores::athena::{use_athena_store, PlanBlock, PlanStepStatus};
use crate::stores::task::use_task_store;
use crate::tauri_bridge;
use dioxus::prelude::*;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

#[derive(Props, Clone, PartialEq)]
pub struct PlanBlockViewProps {
    pub plan: PlanBlock,
}

#[component]
pub fn PlanBlockView(props: PlanBlockViewProps) -> Element {
    let mut collapsed = use_signal(|| false);
    let plan = &props.plan;

    // Kanban ↔ plan deep link: when the user clicks "View in plan" on a kanban
    // card, the store carries the target step id; scroll to and pulse that step.
    let athena_state = use_athena_store();
    let task_store = use_task_store();
    let highlight_step = athena_state.read().plan_highlight_step.clone();
    let plan_has_step = highlight_step
        .as_ref()
        .is_some_and(|h| plan.steps.iter().any(|s| &s.id == h));

    // Scroll the highlighted step into view once the overlay mounts, then
    // clear the highlight so a later click on the same step re-triggers.
    use_effect(move || {
        if !plan_has_step {
            return;
        }
        let Some(step_id) = highlight_step.clone() else {
            return;
        };
        let mut athena_state = athena_state;
        spawn(async move {
            // Let the panel overlay mount / settle before scrolling.
            gloo::timers::future::TimeoutFuture::new(120).await;
            if let (Some(window), Some(doc)) = (
                web_sys::window(),
                web_sys::window().and_then(|w| w.document()),
            ) {
                if let Some(el) = doc.get_element_by_id(&format!("plan-step-{step_id}")) {
                    if let Ok(f) = js_sys::Reflect::get(&el, &JsValue::from_str("scrollIntoView"))
                        .and_then(|f| f.dyn_into::<js_sys::Function>())
                    {
                        let opts = js_sys::Object::new();
                        let _ = js_sys::Reflect::set(
                            &opts,
                            &JsValue::from_str("behavior"),
                            &JsValue::from_str("smooth"),
                        );
                        let _ = js_sys::Reflect::set(
                            &opts,
                            &JsValue::from_str("block"),
                            &JsValue::from_str("center"),
                        );
                        let _ = f.call1(&el, &opts);
                    }
                    let _ = window;
                }
            }
            // Clear the highlight after the pulse animation completes so the
            // next deep link to the same step re-triggers.
            gloo::timers::future::TimeoutFuture::new(2800).await;
            athena_state.write().set_plan_highlight(None);
        });
    });

    // Steps that have been sent to the kanban board this session.
    let sent_steps = use_signal(HashSet::<String>::new);

    let completed_count = plan
        .steps
        .iter()
        .filter(|s| s.status == PlanStepStatus::Completed)
        .count();
    let total_count = plan.steps.len();
    let progress_pct = if total_count > 0 {
        (completed_count as f64 / total_count as f64 * 100.0) as u32
    } else {
        0
    };

    let status_color = match plan.status {
        crate::stores::athena::PlanStatus::Pending => "var(--textDim)",
        crate::stores::athena::PlanStatus::InProgress => "var(--accent)",
        crate::stores::athena::PlanStatus::Completed => "var(--success)",
        crate::stores::athena::PlanStatus::Failed => "var(--error)",
    };

    let is_collapsed = collapsed();

    rsx! {
        div {
            style: "margin-top: 8px; padding: 12px; border-radius: var(--radius-md); border: none; background: transparent;",

            // Header
            div {
                style: "display: flex; align-items: center; gap: 8px; cursor: pointer;",
                onclick: move |_| collapsed.set(!collapsed()),

                span {
                    style: "display: inline-flex; align-items: center; color: var(--textMuted);",
                    if is_collapsed {
                        IconChevronRight { size: Some(14), color: Some("currentColor".to_string()) }
                    } else {
                        IconChevronDown { size: Some(14), color: Some("currentColor".to_string()) }
                    }
                }

                span {
                    style: "font-family: var(--font-display); font-size: 14px; font-weight: 600; letter-spacing: 0.04em; color: var(--accent); flex: 1;",
                    "Plan: {plan.goal}"
                }

                span {
                    class: "pill",
                    style: "background: color-mix(in srgb, {status_color} 13%, transparent); color: {status_color};",
                    "{completed_count}/{total_count}"
                }
            }

            // Progress bar — lapis-tinted track.
            div {
                style: "margin-top: 6px; width: 100%; height: 3px; border-radius: 2px; background: var(--bgTertiary); overflow: hidden;",
                div {
                    style: "width: {progress_pct}%; height: 100%; border-radius: 2px; background: {status_color}; transition: width 0.3s ease;",
                }
            }

            // Steps
            if !collapsed() {
                div {
                    style: "margin-top: 8px; display: flex; flex-direction: column; gap: 4px;",
                    for step in plan.steps.iter() {
                        {
                            let step_status = step.status.clone();
                            let show_agent_type = !step.agent_type.is_empty();
                            let step_id = step.id.clone();
                            let step_title = step.title.clone();
                            let step_desc = step.description.clone();
                            let is_highlighted = athena_state
                                .read()
                                .plan_highlight_step
                                .as_deref()
                                == Some(step.id.as_str());
                            let is_sent = sent_steps.read().contains(&step.id);
                            rsx! {
                                div {
                                    key: "{step.id}",
                                    id: "plan-step-{step.id}",
                                    style: if is_highlighted {
                                        "display: flex; align-items: center; gap: 8px; padding: 4px 6px; border-radius: var(--radius-sm); background: var(--accentSubtle); box-shadow: inset 0 0 0 1px var(--accent); animation: athena-plan-highlight 2.4s ease-in-out;"
                                    } else {
                                        "display: flex; align-items: center; gap: 8px; padding: 4px 0;"
                                    },

                                    span {
                                        style: "flex-shrink: 0; width: 14px; height: 14px; display: inline-flex; align-items: center; justify-content: center;",
                                        match step_status {
                                            PlanStepStatus::Completed => rsx! {
                                                IconCheck { size: Some(13), color: Some("var(--success)".to_string()) }
                                            },
                                            PlanStepStatus::Failed => rsx! {
                                                IconClose { size: Some(13), color: Some("var(--error)".to_string()) }
                                            },
                                            PlanStepStatus::InProgress => rsx! {
                                                span { style: "width: 8px; height: 8px; border-radius: 50%; background: var(--accent);" }
                                            },
                                            PlanStepStatus::Pending => rsx! {
                                                span { style: "width: 8px; height: 8px; border-radius: 50%; border: 1.5px solid var(--textDim); background: transparent;" }
                                            },
                                        }
                                    }
                                    span {
                                        style: "font-size: 12px; color: var(--text); flex: 1;",
                                        "{step.title}"
                                    }
                                    if show_agent_type {
                                        span {
                                            class: "badge",
                                            style: "color: var(--textMuted);",
                                            "{step.agent_type}"
                                        }
                                    }
                                    if is_sent {
                                        span {
                                            class: "badge",
                                            style: "color: var(--success);",
                                            "In Kanban"
                                        }
                                    } else {
                                        button {
                                            class: "icon-btn",
                                            style: "opacity: 0.55;",
                                            title: "Send this step to the kanban board",
                                            "aria-label": "Send step to kanban",
                                            onclick: {
                                                let sid = step_id.clone();
                                                let stitle = step_title.clone();
                                                let sdesc = step_desc.clone();
                                                let mut sent = sent_steps;
                                                let mut task_store = task_store;
                                                move |_| {
                                                    let sid = sid.clone();
                                                    let stitle = stitle.clone();
                                                    let sdesc = sdesc.clone();
                                                    spawn(async move {
                                                        let desc = format!("Plan step: {sdesc}");
                                                        match tauri_bridge::kanban_create_task(
                                                            &stitle,
                                                            Some(&desc),
                                                            Some(&sid),
                                                        )
                                                        .await
                                                        {
                                                            Ok(json) => {
                                                                sent.write().insert(sid.clone());
                                                                // Keep a mounted board in sync immediately.
                                                                if let Ok(tasks) = crate::stores::task::tasks_from_backend_json(&format!("[{json}]")) {
                                                                    if let Some(task) = tasks.into_iter().next() {
                                                                        task_store.write().add_task(task);
                                                                    }
                                                                }
                                                            }
                                                            Err(error) => {
                                                                web_sys::console::error_1(
                                                                    &format!("[plan] kanban create failed: {error:?}").into(),
                                                                );
                                                            }
                                                        }
                                                    });
                                                }
                                            },
                                            IconKanban { size: Some(13), color: Some("currentColor".to_string()) }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
