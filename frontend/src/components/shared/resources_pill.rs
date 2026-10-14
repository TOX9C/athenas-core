//! Status-bar pill: aggregated CPU/RAM for the app family (main + WebKit
//! renderer + PTY shells). Polls `resource_stats` every 2 s and renders a
//! "412 MB · 1.8%" roll-up next to the theme label. Hover shows per-process
//! breakdown. A `~` on the renderer row means the footprint came from the
//! frontend (renderer-side rusage), where the main process cannot read it
//! under the sandbox.

use dioxus::prelude::*;

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct ProcStat {
    name: String,
    pid: u32,
    rss_bytes: u64,
    footprint_bytes: Option<u64>,
    cpu_pct: f64,
    is_renderer: bool,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[allow(dead_code)]
struct FamilySnapshot {
    procs: Vec<ProcStat>,
    total_rss_bytes: u64,
    total_footprint_bytes: u64,
    total_cpu_pct: f64,
    renderer_estimated: bool,
}

fn format_bytes(bytes: u64) -> String {
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{:.0} MB", mb)
    }
}

#[component]
pub fn ResourcesPill() -> Element {
    let mut snapshot = use_signal(|| Option::<FamilySnapshot>::None);

    use_future(move || async move {
        loop {
            let result = crate::tauri_bridge::invoke::<String>("resource_stats", "{}").await;
            match result {
                Ok(json) => {
                    if let Ok(parsed) = serde_json::from_str::<FamilySnapshot>(&json) {
                        snapshot.set(Some(parsed));
                    } else {
                        snapshot.set(None);
                    }
                }
                Err(_) => snapshot.set(None),
            }
            gloo::timers::future::TimeoutFuture::new(2000).await;
        }
    });

    let snap = snapshot.read();
    let Some(snap) = snap.as_ref() else {
        return rsx! { span {} };
    };

    let ram_label = format_bytes(snap.total_footprint_bytes.max(snap.total_rss_bytes));
    let cpu_label = format!("{:.1}%", snap.total_cpu_pct.max(0.0));
    let tooltip = snap
        .procs
        .iter()
        .map(|p| {
            let tilde = if p.is_renderer && snap.renderer_estimated { " ~" } else { "" };
            format!("{}{} {}", p.name, tilde, format_bytes(p.rss_bytes.max(p.footprint_bytes.unwrap_or(0))))
        })
        .collect::<Vec<_>>()
        .join("\n");

    let pill_title = if snap.renderer_estimated {
        format!("{}\n(renderer ~ = renderer-measured RAM)", tooltip)
    } else {
        tooltip
    };

    rsx! {
        span {
            title: pill_title,
            style: "display: inline-flex; align-items: center; gap: 6px; font-size: var(--text-xs); font-variant-numeric: tabular-nums; color: var(--textMuted);",
            "RAM {ram_label} · CPU {cpu_label}"
        }
    }
}
