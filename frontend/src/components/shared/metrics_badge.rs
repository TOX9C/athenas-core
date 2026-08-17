//! Status-bar metrics badge.
//!
//! Reads `window.__athenaMetrics.snapshot` (a JSON string refreshed by the app
//! root on a slow interval) and renders a compact readout of the most
//! relevant counters:
//! root-shell renders vs. per-pane renders, IPC calls, and push-event bytes.
//! This makes the reactive-isolation guarantees observable in the live app and
//! in WebDriver screenshots without subscribing to any store signals.

use crate::utils::perf_metrics::MetricsSnapshot;
use dioxus::prelude::*;

/// Render the metrics badge into the status bar.
#[component]
pub fn MetricsBadge() -> Element {
    let snapshot = use_signal(MetricsSnapshot::default);

    use_future(move || {
        let mut snapshot = snapshot;
        async move {
            loop {
                let raw = web_sys::window()
                    .and_then(|w| {
                        js_sys::Reflect::get(
                            &w,
                            &wasm_bindgen::JsValue::from_str("__athenaMetrics"),
                        )
                        .ok()
                        .and_then(|metrics| {
                            js_sys::Reflect::get(
                                &metrics,
                                &wasm_bindgen::JsValue::from_str("snapshot"),
                            )
                            .ok()
                            .and_then(|v| v.as_string())
                        })
                    })
                    .unwrap_or_default();
                if let Some(parsed) = MetricsSnapshot::parse(&raw) {
                    snapshot.set(parsed);
                }
                gloo::timers::future::TimeoutFuture::new(2_000).await;
            }
        }
    });

    let renders = snapshot.read();
    let app_renders = renders.renders("App");
    let pane_renders = renders.renders("PaneItem");
    let terminal_controller_renders = renders.renders("TerminalController");
    let ipc_total: u64 = renders.ipc.values().sum();
    let event_bytes_kb = renders.event_bytes / 1024;
    // Average App render cost in milliseconds, derived from the cumulative
    // render-duration counter. A growing average signals a render storm in
    // the root shell (every store write re-renders App).
    let app_avg_ms = if app_renders > 0 {
        (renders.render_duration_us("App") as f64 / app_renders as f64 / 1000.0)
            .round()
            .to_string()
    } else {
        "-".to_string()
    };

    rsx! {
        span {
            title: "Render/IPC metrics — App renders vs PaneItem renders vs TerminalController renders; App avg render ms; IPC calls; push-event KB. Refreshed every 2s.",
            style: "display: inline-flex; align-items: center; gap: 6px; color: var(--textDim); font-size: var(--text-xs); font-variant-numeric: tabular-nums;",
            span { style: "color: var(--accent);", "R" }
            span { "App {app_renders}" }
            span { "·" }
            span { "Pane {pane_renders}" }
            span { "·" }
            span { "Ctrl {terminal_controller_renders}" }
            span { "·" }
            span { "App {app_avg_ms}ms" }
            span { "·" }
            span { "IPC {ipc_total}" }
            span { "·" }
            span { "ev {event_bytes_kb}KB" }
        }
    }
}
