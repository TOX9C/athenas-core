//! Assistant health monitoring — ported from src/utils/assistantHealth.ts
//!
//! Tracks health of the LLM assistant via provider checks, session checks,
//! circuit breaker checks, and heartbeat checks.

use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
    Offline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthCheckType {
    Provider,
    Session,
    Circuit,
    Heartbeat,
}

#[derive(Debug, Clone)]
pub struct HealthCheckResult {
    pub check_type: HealthCheckType,
    pub status: HealthStatus,
    pub message: String,
    pub timestamp: u64,
    pub latency_ms: Option<u64>,
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct AssistantHealthSnapshot {
    pub overall: HealthStatus,
    pub checks: Vec<HealthCheckResult>,
    pub last_checked_at: u64,
    pub consecutive_failures: u32,
    pub last_healthy_at: Option<u64>,
    pub last_unhealthy_at: Option<u64>,
    pub uptime_ms: u64,
    pub total_requests: u64,
    pub total_failures: u64,
    pub total_successes: u64,
}

pub type HealthChangeListener = Arc<dyn Fn(&AssistantHealthSnapshot) + Send + Sync>;

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

struct Inner {
    snapshot: AssistantHealthSnapshot,
    started_at: Option<u64>,
    listeners: Vec<HealthChangeListener>,
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn compute_overall(checks: &[HealthCheckResult]) -> HealthStatus {
    if checks.is_empty() {
        return HealthStatus::Offline;
    }
    let mut has_degraded = false;
    for c in checks {
        match c.status {
            HealthStatus::Unhealthy | HealthStatus::Offline => return HealthStatus::Unhealthy,
            HealthStatus::Degraded => has_degraded = true,
            HealthStatus::Healthy => {}
        }
    }
    if has_degraded {
        HealthStatus::Degraded
    } else {
        HealthStatus::Healthy
    }
}

fn initial_snapshot() -> AssistantHealthSnapshot {
    AssistantHealthSnapshot {
        overall: HealthStatus::Offline,
        checks: Vec::new(),
        last_checked_at: 0,
        consecutive_failures: 0,
        last_healthy_at: None,
        last_unhealthy_at: None,
        uptime_ms: 0,
        total_requests: 0,
        total_failures: 0,
        total_successes: 0,
    }
}

// ---------------------------------------------------------------------------
// AssistantHealth
// ---------------------------------------------------------------------------

pub struct AssistantHealth {
    inner: Arc<Mutex<Inner>>,
}

impl Default for AssistantHealth {
    fn default() -> Self {
        Self::new()
    }
}

impl AssistantHealth {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                snapshot: initial_snapshot(),
                started_at: None,
                listeners: Vec::new(),
            })),
        }
    }

    pub fn start(&self) {
        let (snapshot, listeners) = {
            let mut inner = self.inner.lock().unwrap();
            let now = now_ms();
            inner.snapshot = initial_snapshot();
            inner.snapshot.overall = HealthStatus::Healthy;
            inner.snapshot.last_healthy_at = Some(now);
            inner.started_at = Some(now);
            (inner.snapshot.clone(), inner.listeners.clone())
        };
        self.notify(snapshot, listeners);
    }

    pub fn record_check(&self, result: HealthCheckResult) {
        let (snapshot, listeners) = {
            let mut inner = self.inner.lock().unwrap();
            let ts = result.timestamp;

            // Replace existing check of the same type
            inner
                .snapshot
                .checks
                .retain(|c| c.check_type != result.check_type);
            inner.snapshot.checks.push(result);
            inner.snapshot.last_checked_at = ts;
            inner.snapshot.overall = compute_overall(&inner.snapshot.checks);

            match inner.snapshot.overall {
                HealthStatus::Healthy | HealthStatus::Degraded => {
                    inner.snapshot.last_healthy_at = Some(ts);
                    inner.snapshot.consecutive_failures = 0;
                }
                HealthStatus::Unhealthy | HealthStatus::Offline => {
                    inner.snapshot.last_unhealthy_at = Some(ts);
                    inner.snapshot.consecutive_failures += 1;
                }
            }

            if let Some(started) = inner.started_at {
                inner.snapshot.uptime_ms = ts.saturating_sub(started);
            }
            (inner.snapshot.clone(), inner.listeners.clone())
        };
        self.notify(snapshot, listeners);
    }

    pub fn record_request(&self, success: bool) {
        let mut inner = self.inner.lock().unwrap();
        inner.snapshot.total_requests += 1;
        if success {
            inner.snapshot.total_successes += 1;
        } else {
            inner.snapshot.total_failures += 1;
        }
    }

    pub fn get_snapshot(&self) -> AssistantHealthSnapshot {
        let inner = self.inner.lock().unwrap();
        inner.snapshot.clone()
    }

    pub fn get_status(&self) -> HealthStatus {
        let inner = self.inner.lock().unwrap();
        inner.snapshot.overall
    }

    pub fn is_healthy(&self) -> bool {
        matches!(
            self.get_status(),
            HealthStatus::Healthy | HealthStatus::Degraded
        )
    }

    pub fn should_recover(&self) -> bool {
        let inner = self.inner.lock().unwrap();
        inner.snapshot.consecutive_failures >= 3
            || inner.snapshot.overall == HealthStatus::Unhealthy
    }

    pub fn reset(&self) {
        let (snapshot, listeners) = {
            let mut inner = self.inner.lock().unwrap();
            inner.snapshot = initial_snapshot();
            inner.started_at = None;
            (inner.snapshot.clone(), inner.listeners.clone())
        };
        self.notify(snapshot, listeners);
        // Re-start immediately
        self.start();
    }

    pub fn on_health_change(&self, listener: HealthChangeListener) {
        let mut inner = self.inner.lock().unwrap();
        inner.listeners.push(listener);
    }

    fn notify(&self, snapshot: AssistantHealthSnapshot, listeners: Vec<HealthChangeListener>) {
        for listener in &listeners {
            listener(&snapshot);
        }
    }
}

// ---------------------------------------------------------------------------
// Factory helpers
// ---------------------------------------------------------------------------

impl AssistantHealth {
    pub fn create_provider_check(
        provider: &str,
        latency_ms: u64,
        error: Option<&str>,
    ) -> HealthCheckResult {
        let (status, message) = match error {
            Some(err) if latency_ms > 10_000 => (
                HealthStatus::Unhealthy,
                format!("Provider {} error with high latency: {}", provider, err),
            ),
            Some(err) => (
                HealthStatus::Degraded,
                format!("Provider {} error: {}", provider, err),
            ),
            None if latency_ms > 10_000 => (
                HealthStatus::Degraded,
                format!("Provider {} slow response ({}ms)", provider, latency_ms),
            ),
            None => (
                HealthStatus::Healthy,
                format!("Provider {} responding normally", provider),
            ),
        };
        HealthCheckResult {
            check_type: HealthCheckType::Provider,
            status,
            message,
            timestamp: now_ms(),
            latency_ms: Some(latency_ms),
            details: None,
        }
    }

    pub fn create_circuit_check(circuit_state: &str, trip_count: u32) -> HealthCheckResult {
        let (status, message) = match circuit_state {
            "open" => (
                HealthStatus::Unhealthy,
                format!("Circuit breaker open ({} trips)", trip_count),
            ),
            "half_open" => (
                HealthStatus::Degraded,
                format!("Circuit breaker half-open ({} trips)", trip_count),
            ),
            _ => (HealthStatus::Healthy, "Circuit breaker closed".to_string()),
        };
        HealthCheckResult {
            check_type: HealthCheckType::Circuit,
            status,
            message,
            timestamp: now_ms(),
            latency_ms: None,
            details: Some(serde_json::json!({ "tripCount": trip_count })),
        }
    }

    pub fn create_session_check(active: u32, stalled: u32) -> HealthCheckResult {
        let (status, message) = if stalled == 0 {
            (
                HealthStatus::Healthy,
                format!("{} active sessions, no stalls", active),
            )
        } else if stalled <= 2 {
            (
                HealthStatus::Degraded,
                format!("{} active, {} stalled", active, stalled),
            )
        } else {
            (
                HealthStatus::Unhealthy,
                format!("{} active, {} stalled", active, stalled),
            )
        };
        HealthCheckResult {
            check_type: HealthCheckType::Session,
            status,
            message,
            timestamp: now_ms(),
            latency_ms: None,
            details: Some(serde_json::json!({ "active": active, "stalled": stalled })),
        }
    }

    pub fn create_heartbeat_check(responsive: bool, latency_ms: Option<u64>) -> HealthCheckResult {
        let (status, message) = if responsive {
            (HealthStatus::Healthy, "Heartbeat responsive".to_string())
        } else {
            (
                HealthStatus::Unhealthy,
                "Heartbeat not responsive".to_string(),
            )
        };
        HealthCheckResult {
            check_type: HealthCheckType::Heartbeat,
            status,
            message,
            timestamp: now_ms(),
            latency_ms,
            details: None,
        }
    }
}
