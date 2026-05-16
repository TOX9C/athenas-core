//! Circuit breaker pattern — ported from src/utils/circuitBreaker.ts
//!
//! Implements the three-state circuit breaker (closed → open → half_open)
//! with configurable thresholds and time-based reset.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub reset_timeout: Duration,
    pub half_open_max_attempts: u32,
    pub monitoring_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            half_open_max_attempts: 1,
            monitoring_window: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub failure_count: u32,
    pub success_count: u32,
    pub last_failure_at: Option<u64>,
    pub last_success_at: Option<u64>,
    pub last_state_change_at: Option<u64>,
    pub total_trips: u32,
    pub consecutive_failures: u32,
    pub next_retry_at: Option<u64>,
}

/// Error returned when the circuit is open.
#[derive(Debug)]
pub struct CircuitOpenError {
    pub circuit_state: CircuitState,
    pub retry_at: Option<u64>,
}

impl std::fmt::Display for CircuitOpenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Circuit is {:?}", self.circuit_state)
    }
}

impl std::error::Error for CircuitOpenError {}

/// Error returned by [`CircuitBreaker::execute`], wrapping either a circuit-open
/// condition or the inner error from the supplied closure.
#[derive(Debug)]
pub enum CircuitError<E> {
    Open(CircuitOpenError),
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for CircuitError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitError::Open(e) => write!(f, "circuit open: {}", e),
            CircuitError::Inner(e) => write!(f, "inner error: {}", e),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for CircuitError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CircuitError::Open(e) => Some(e),
            CircuitError::Inner(e) => Some(e),
        }
    }
}

// ---------------------------------------------------------------------------
// Internal state
// ---------------------------------------------------------------------------

struct Inner {
    config: CircuitBreakerConfig,
    state: CircuitState,
    failure_count: u32,
    success_count: u32,
    last_failure_at: Option<Instant>,
    last_success_at: Option<Instant>,
    last_state_change_at: Option<Instant>,
    total_trips: u32,
    consecutive_failures: u32,
    half_open_attempts: u32,
    failure_timestamps: Vec<Instant>,
    on_state_change: Option<Box<dyn Fn(CircuitState, CircuitState) + Send + Sync>>,
}

// ---------------------------------------------------------------------------
// CircuitBreaker
// ---------------------------------------------------------------------------

pub struct CircuitBreaker {
    inner: Arc<Mutex<Inner>>,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                state: CircuitState::Closed,
                failure_count: 0,
                success_count: 0,
                last_failure_at: None,
                last_success_at: None,
                last_state_change_at: None,
                total_trips: 0,
                consecutive_failures: 0,
                half_open_attempts: 0,
                failure_timestamps: Vec::new(),
                on_state_change: None,
                config,
            })),
        }
    }

    pub fn with_state_change(
        config: CircuitBreakerConfig,
        callback: Box<dyn Fn(CircuitState, CircuitState) + Send + Sync>,
    ) -> Self {
        let cb = Self::new(config.clone());
        {
            let mut inner = cb.inner.lock().unwrap();
            inner.on_state_change = Some(callback);
        }
        cb
    }

    fn transition_to(inner: &mut Inner, new_state: CircuitState) {
        let old = inner.state;
        if old == new_state {
            return;
        }
        inner.state = new_state;
        inner.last_state_change_at = Some(Instant::now());
        if new_state == CircuitState::Closed {
            inner.consecutive_failures = 0;
            inner.failure_count = 0;
            inner.half_open_attempts = 0;
        } else if new_state == CircuitState::Open {
            inner.total_trips += 1;
            inner.half_open_attempts = 0;
        }
        if let Some(ref cb) = inner.on_state_change {
            cb(old, new_state);
        }
    }

    fn prune_failures(inner: &mut Inner) {
        let now = Instant::now();
        let window = inner.config.monitoring_window;
        inner
            .failure_timestamps
            .retain(|t| now.duration_since(*t) < window);
        inner.failure_count = inner.failure_timestamps.len() as u32;
    }

    fn check_state(inner: &mut Inner) {
        if inner.state == CircuitState::Open {
            if let Some(last_change) = inner.last_state_change_at {
                if Instant::now().duration_since(last_change) >= inner.config.reset_timeout {
                    Self::transition_to(inner, CircuitState::HalfOpen);
                }
            }
        }
    }

    pub fn can_execute(&self) -> bool {
        let mut inner = self.inner.lock().unwrap();
        Self::prune_failures(&mut inner);
        Self::check_state(&mut inner);
        match inner.state {
            CircuitState::Closed => true,
            CircuitState::Open => false,
            CircuitState::HalfOpen => {
                inner.half_open_attempts < inner.config.half_open_max_attempts
            }
        }
    }

    pub fn record_success(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.success_count += 1;
        inner.last_success_at = Some(Instant::now());
        if inner.state == CircuitState::HalfOpen {
            Self::transition_to(&mut inner, CircuitState::Closed);
        }
    }

    pub fn record_failure(&self) {
        let mut inner = self.inner.lock().unwrap();
        let now = Instant::now();
        inner.failure_count += 1;
        inner.consecutive_failures += 1;
        inner.last_failure_at = Some(now);
        inner.failure_timestamps.push(now);

        if inner.state == CircuitState::HalfOpen {
            Self::transition_to(&mut inner, CircuitState::Open);
        } else if inner.state == CircuitState::Closed {
            Self::prune_failures(&mut inner);
            if inner.failure_count >= inner.config.failure_threshold
                || inner.consecutive_failures >= inner.config.failure_threshold
            {
                Self::transition_to(&mut inner, CircuitState::Open);
            }
        }
    }

    /// Execute a closure through the circuit breaker.
    pub fn execute<T, E, F>(&self, f: F) -> Result<T, CircuitError<E>>
    where
        F: FnOnce() -> Result<T, E>,
    {
        if !self.can_execute() {
            let inner = self.inner.lock().unwrap();
            return Err(CircuitError::Open(CircuitOpenError {
                circuit_state: inner.state,
                retry_at: inner.last_state_change_at.map(|t| {
                    let elapsed = Instant::now().duration_since(t);
                    let remaining = inner.config.reset_timeout.saturating_sub(elapsed);
                    now_ms() + remaining.as_millis() as u64
                }),
            }));
        }
        match f() {
            Ok(v) => {
                self.record_success();
                Ok(v)
            }
            Err(e) => {
                self.record_failure();
                Err(CircuitError::Inner(e))
            }
        }
    }

    pub fn reset(&self) {
        let mut inner = self.inner.lock().unwrap();
        Self::transition_to(&mut inner, CircuitState::Closed);
    }

    pub fn get_stats(&self) -> CircuitBreakerStats {
        let inner = self.inner.lock().unwrap();
        CircuitBreakerStats {
            state: inner.state,
            failure_count: inner.failure_count,
            success_count: inner.success_count,
            last_failure_at: inner.last_failure_at.map(instant_to_ms),
            last_success_at: inner.last_success_at.map(instant_to_ms),
            last_state_change_at: inner.last_state_change_at.map(instant_to_ms),
            total_trips: inner.total_trips,
            consecutive_failures: inner.consecutive_failures,
            next_retry_at: if inner.state == CircuitState::Open {
                inner.last_state_change_at.map(|t| {
                    let elapsed = Instant::now().duration_since(t);
                    let remaining = inner.config.reset_timeout.saturating_sub(elapsed);
                    now_ms() + remaining.as_millis() as u64
                })
            } else {
                None
            },
        }
    }

    pub fn get_state(&self) -> CircuitState {
        let mut inner = self.inner.lock().unwrap();
        Self::check_state(&mut inner);
        inner.state
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn instant_to_ms(_t: Instant) -> u64 {
    // Monotonic instants can't be converted to wall-clock ms directly.
    // We return 0 as a placeholder — consumers should use relative durations.
    now_ms()
}

/// Create a circuit breaker tuned for LLM assistant API calls.
pub fn create_assistant_circuit_breaker(
    on_state_change: Option<Box<dyn Fn(CircuitState, CircuitState) + Send + Sync>>,
) -> CircuitBreaker {
    let config = CircuitBreakerConfig {
        failure_threshold: 5,
        reset_timeout: Duration::from_secs(30),
        half_open_max_attempts: 2,
        monitoring_window: Duration::from_secs(60),
    };
    match on_state_change {
        Some(cb) => CircuitBreaker::with_state_change(config, cb),
        None => CircuitBreaker::new(config),
    }
}
