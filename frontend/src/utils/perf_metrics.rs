//! Lightweight performance instrumentation used to verify reactive-isolation
//! work (e.g. root-shell subscription reduction under multi-pane terminal
//! load).
//!
//! All counters are `static AtomicU64`s: incrementing is a single relaxed
//! atomic op and never allocates, so this stays out of the hot path. Counters
//! are intentionally cumulative and never reset by normal app flow; the e2e
//! metrics spec reads deltas between snapshots.
//!
//! The snapshot is exposed as `window.__athenaMetrics.snapshot()` so WebDriver
//! tests (tauri-webdriver) and the in-app status-bar badge can read the same
//! numbers. Components register render counts with [`mark_render`]; the Tauri
//! bridge records IPC/event traffic with [`record_ipc`] / [`record_event`].

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use wasm_bindgen::JsCast;

/// Command name -> call count for Tauri `invoke` traffic.
static IPC_CALLS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());
/// Event name -> received count for Tauri `listen` push events.
static EVENT_COUNTS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());
/// Component label -> render count.
static RENDER_COUNTS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());
/// Component label -> cumulative render duration in microseconds.
static RENDER_DURATIONS: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());
/// Total `listen` payload bytes observed (JSON string length).
static EVENT_BYTES: AtomicU64 = AtomicU64::new(0);

/// Canonicalize a command/event name to a `'static` key without touching the
/// lock on the hot path. Known names resolve to literals; unknown names fall
/// back to the shared cache (bounded: the app exposes ~134 commands and a
/// fixed set of push events, so leaks are bounded and one-time).
fn static_key(key: &str) -> &'static str {
    match key {
        "pty:raw" => "pty:raw",
        "terminal:data" => "terminal:data",
        "terminal:exit" => "terminal:exit",
        "output-capture:batch" => "output-capture:batch",
        "output-capture:paneRegistered" => "output-capture:paneRegistered",
        "agent:status" => "agent:status",
        "agents:connected" => "agents:connected",
        "agents:disconnected" => "agents:disconnected",
        "agents:statusUpdate" => "agents:statusUpdate",
        "agents:inputRequested" => "agents:inputRequested",
        "notifications:new" => "notifications:new",
        "pty_write" => "pty_write",
        "pty_spawn" => "pty_spawn",
        "pty_spawn_agent" => "pty_spawn_agent",
        "pty_kill" => "pty_kill",
        "pty_resize" => "pty_resize",
        "workspace_add_trusted_root" => "workspace_add_trusted_root",
        _ => static_key_cached(key),
    }
}

/// Bounded one-time leak cache for names not covered by [`static_key`].
fn static_key_cached(key: &str) -> &'static str {
    static KEY_CACHE: Mutex<BTreeMap<String, &'static str>> = Mutex::new(BTreeMap::new());
    if let Ok(mut cache) = KEY_CACHE.lock() {
        if let Some(cached) = cache.get(key) {
            return cached;
        }
        let leaked: &'static str = Box::leak(key.to_string().into_boxed_str());
        cache.insert(key.to_string(), leaked);
        return leaked;
    }
    // Lock poisoned: fall back to a leak (safe, tiny, one-time).
    Box::leak(key.to_string().into_boxed_str())
}

fn incr(map: &Mutex<BTreeMap<&'static str, u64>>, key: &'static str) {
    if let Ok(mut guard) = map.lock() {
        *guard.entry(key).or_insert(0) += 1;
    }
}

fn snapshot_map(map: &Mutex<BTreeMap<&'static str, u64>>) -> BTreeMap<&'static str, u64> {
    map.lock().map(|g| g.clone()).unwrap_or_default()
}

/// Record one Tauri `invoke` call. Called from `tauri_bridge::invoke`.
pub fn record_ipc(command: &str) {
    incr(&IPC_CALLS, static_key(command));
}

/// Record one Tauri push event delivered through `tauri_bridge::listen`.
pub fn record_event(event: &str, payload_bytes: u64) {
    incr(&EVENT_COUNTS, static_key(event));
    EVENT_BYTES.fetch_add(payload_bytes, Ordering::Relaxed);
}

/// Count one invocation of a component body (i.e. a render pass, whether or
/// not Dioxus commits it). Called directly from component bodies —
/// deliberately *not* from an effect (effects only re-run on reactive
/// changes, which would undercount renders). Counts are best interpreted as
/// relative deltas between two snapshots, not absolute committed renders.
pub fn mark_render(label: &'static str) {
    incr(&RENDER_COUNTS, label);
}

/// Record the duration (microseconds) of a single render pass for a
/// component. Callers time the component body themselves and report the
/// delta; durations accumulate per component so e2e can compare total
/// render cost across two scenarios.
///
/// Like [`mark_render`], the `'static` label is used directly as the map key
/// (not run through `static_key`, which would `Box::leak` unknown names).
pub fn mark_render_duration(label: &'static str, duration_us: u64) {
    if let Ok(mut guard) = RENDER_DURATIONS.lock() {
        *guard.entry(label).or_insert(0) += duration_us;
    }
}

/// Serialize the full metrics snapshot as a JSON string.
///
/// ```json
/// {
///   "renders": { "App": 12, "PaneItem": 480 },
///   "renderDurations": { "App": 2400, "PaneItem": 960 },
///   "ipc": { "pty_write": 36 },
///   "events": { "output-capture:batch": 8 },
///   "eventBytes": 8192
/// }
/// ```
pub fn snapshot_json() -> String {
    serde_json::json!({
        "renders": snapshot_map(&RENDER_COUNTS),
        "renderDurations": snapshot_map(&RENDER_DURATIONS),
        "ipc": snapshot_map(&IPC_CALLS),
        "events": snapshot_map(&EVENT_COUNTS),
        "eventBytes": EVENT_BYTES.load(Ordering::Relaxed),
    })
    .to_string()
}

/// Install `window.__athenaMetrics.snapshot` so WebDriver and console tooling
/// can read counters.
///
/// The snapshot is a plain JSON **string** property on a plain JS object — no
/// functions are constructed, because `new Function` is indirect `eval` and
/// this app's CSP forbids it (`EvalError`; see `themes/mod.rs`). WebDriver
/// reads `window.__athenaMetrics.snapshot` and parses it on the test side.
/// Idempotent; safe to call repeatedly from the root.
pub fn install_window_snapshot() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let metrics_obj = js_sys::Object::new();
    let _ = js_sys::Reflect::set(
        &window,
        &wasm_bindgen::JsValue::from_str("__athenaMetrics"),
        &metrics_obj,
    );
    refresh_window_snapshot();
}

/// Refresh the JSON string at `window.__athenaMetrics.snapshot`. Called on a
/// slow interval from the app root and once at install time so the counters
/// stay live without per-render work.
pub fn refresh_window_snapshot() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(metrics_obj) =
        js_sys::Reflect::get(&window, &wasm_bindgen::JsValue::from_str("__athenaMetrics"))
            .ok()
            .and_then(|v| v.dyn_into::<js_sys::Object>().ok())
    else {
        return;
    };
    let _ = js_sys::Reflect::set(
        &metrics_obj,
        &wasm_bindgen::JsValue::from_str("snapshot"),
        &wasm_bindgen::JsValue::from_str(&snapshot_json()),
    );
}

/// Typed snapshot used by the status-bar badge.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MetricsSnapshot {
    pub renders: BTreeMap<String, u64>,
    pub render_durations: BTreeMap<String, u64>,
    pub ipc: BTreeMap<String, u64>,
    pub events: BTreeMap<String, u64>,
    pub event_bytes: u64,
}

impl MetricsSnapshot {
    /// Parse a JSON snapshot string into a typed struct (best-effort).
    ///
    /// Unknown or missing sections (e.g. `renderDurations` in snapshots
    /// produced before the field existed) parse to empty maps rather than
    /// failing the whole snapshot, so old e2e specs and badge code keep
    /// working.
    pub fn parse(json: &str) -> Option<Self> {
        let value: serde_json::Value = serde_json::from_str(json).ok()?;
        let renders = parse_map(value.get("renders")?);
        let render_durations = value
            .get("renderDurations")
            .map(parse_map)
            .unwrap_or_default();
        let ipc = parse_map(value.get("ipc")?);
        let events = parse_map(value.get("events")?);
        let event_bytes = value
            .get("eventBytes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        Some(Self {
            renders,
            render_durations,
            ipc,
            events,
            event_bytes,
        })
    }

    /// Render count for a component label.
    pub fn renders(&self, label: &str) -> u64 {
        self.renders.get(label).copied().unwrap_or(0)
    }

    /// Cumulative render duration (µs) for a component label.
    pub fn render_duration_us(&self, label: &str) -> u64 {
        self.render_durations.get(label).copied().unwrap_or(0)
    }
}

fn parse_map(value: &serde_json::Value) -> BTreeMap<String, u64> {
    value
        .as_object()
        .map(|obj| {
            obj.iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (k.clone(), n)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_json_contains_expected_sections() {
        let json = snapshot_json();
        let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
        assert!(value.get("renders").is_some());
        assert!(value.get("renderDurations").is_some());
        assert!(value.get("ipc").is_some());
        assert!(value.get("events").is_some());
        assert!(value.get("eventBytes").is_some());
    }

    #[test]
    fn mark_render_duration_accumulates_per_component() {
        mark_render_duration("App", 500);
        mark_render_duration("App", 700);
        let snap = MetricsSnapshot::parse(&snapshot_json()).expect("parse");
        assert_eq!(snap.render_duration_us("App"), 1200);
    }

    #[test]
    fn record_event_accumulates_payload_bytes() {
        let before = EVENT_BYTES.load(Ordering::Relaxed);
        record_event("output-capture:batch", 1234);
        assert_eq!(
            EVENT_BYTES.load(Ordering::Relaxed),
            before + 1234,
            "payload bytes accumulate"
        );
    }

    #[test]
    fn metrics_snapshot_round_trips() {
        let snap = MetricsSnapshot::parse(
            r#"{"renders":{"App":3},"renderDurations":{"App":1500},"ipc":{"pty_write":2},"events":{"output-capture:batch":1},"eventBytes":50}"#,
        )
        .expect("parse");
        assert_eq!(snap.renders("App"), 3);
        assert_eq!(snap.render_duration_us("App"), 1500);
        assert_eq!(snap.ipc.get("pty_write"), Some(&2));
        assert_eq!(snap.events.get("output-capture:batch"), Some(&1));
        assert_eq!(snap.event_bytes, 50);
    }

    #[test]
    fn metrics_snapshot_missing_render_durations_defaults_empty() {
        // Snapshots produced before `renderDurations` existed must still parse.
        let snap = MetricsSnapshot::parse(
            r#"{"renders":{"App":3},"ipc":{"pty_write":2},"events":{"output-capture:batch":1},"eventBytes":50}"#,
        )
        .expect("parse");
        assert_eq!(snap.render_duration_us("App"), 0);
        assert_eq!(snap.renders("App"), 3);
    }
}
