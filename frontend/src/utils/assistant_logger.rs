//! Assistant logger — ported from src/utils/assistantLogger.ts
//!
//! Structured logging for Athena assistant actions with bounded storage
//! and listener support.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LogLevel {
    Debug = 0,
    Info = 1,
    Warn = 2,
    Error = 3,
    Critical = 4,
}

impl serde::Serialize for LogLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(match self {
            LogLevel::Debug => "debug",
            LogLevel::Info => "info",
            LogLevel::Warn => "warn",
            LogLevel::Error => "error",
            LogLevel::Critical => "critical",
        })
    }
}

/// Named assistant action types for structured logging.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantAction {
    ChatSend,
    ChatReceive,
    ChatError,
    ChatRetry,
    PanelOpen,
    PanelClose,
    AgentSpawn,
    AgentClose,
    AgentCommand,
    AgentExit,
    CircuitOpen,
    CircuitClose,
    CircuitHalfOpen,
    HealthCheck,
    HealthDegraded,
    HealthRecovery,
    StateReset,
    ProviderCall,
    ProviderError,
    ProviderTimeout,
    ToolExecute,
    ToolError,
    ToolSuccess,
    SessionStart,
    SessionEnd,
    RecoveryAttempt,
    RecoverySuccess,
    RecoveryFailure,
}

impl AssistantAction {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ChatSend => "chat_send",
            Self::ChatReceive => "chat_receive",
            Self::ChatError => "chat_error",
            Self::ChatRetry => "chat_retry",
            Self::PanelOpen => "panel_open",
            Self::PanelClose => "panel_close",
            Self::AgentSpawn => "agent_spawn",
            Self::AgentClose => "agent_close",
            Self::AgentCommand => "agent_command",
            Self::AgentExit => "agent_exit",
            Self::CircuitOpen => "circuit_open",
            Self::CircuitClose => "circuit_close",
            Self::CircuitHalfOpen => "circuit_half_open",
            Self::HealthCheck => "health_check",
            Self::HealthDegraded => "health_degraded",
            Self::HealthRecovery => "health_recovery",
            Self::StateReset => "state_reset",
            Self::ProviderCall => "provider_call",
            Self::ProviderError => "provider_error",
            Self::ProviderTimeout => "provider_timeout",
            Self::ToolExecute => "tool_execute",
            Self::ToolError => "tool_error",
            Self::ToolSuccess => "tool_success",
            Self::SessionStart => "session_start",
            Self::SessionEnd => "session_end",
            Self::RecoveryAttempt => "recovery_attempt",
            Self::RecoverySuccess => "recovery_success",
            Self::RecoveryFailure => "recovery_failure",
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogEntry {
    pub id: String,
    pub timestamp: u64,
    pub level: LogLevel,
    pub action: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<LogError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct LogError {
    pub name: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

pub type LogListener = Box<dyn Fn(&LogEntry) + Send + Sync>;

/// Options for filtering log entries.
#[derive(Debug, Clone, Default)]
pub struct GetEntriesOptions {
    pub level: Option<LogLevel>,
    pub action: Option<AssistantAction>,
    pub since: Option<u64>,
    pub limit: Option<usize>,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const MAX_LOG_ENTRIES: usize = 500;
const MIN_LEVEL: LogLevel = LogLevel::Debug;

// ---------------------------------------------------------------------------
// Inner state
// ---------------------------------------------------------------------------

struct Inner {
    entries: VecDeque<LogEntry>,
    listeners: Vec<LogListener>,
    id_counter: u64,
}

// ---------------------------------------------------------------------------
// AssistantLogger
// ---------------------------------------------------------------------------

pub struct AssistantLogger {
    inner: Arc<Mutex<Inner>>,
}

impl Default for AssistantLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl AssistantLogger {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                entries: VecDeque::with_capacity(MAX_LOG_ENTRIES),
                listeners: Vec::new(),
                id_counter: 0,
            })),
        }
    }

    fn log(&self, level: LogLevel, action: AssistantAction, message: &str, opts: LogOptions) {
        if level < MIN_LEVEL {
            return;
        }

        let mut inner = self.inner.lock().unwrap();
        inner.id_counter += 1;
        let entry = LogEntry {
            id: format!("log-{}-{}", now_ms(), inner.id_counter),
            timestamp: now_ms(),
            level,
            action: action.as_str().to_string(),
            message: message.to_string(),
            correlation_id: opts.correlation_id,
            provider: opts.provider,
            duration_ms: opts.duration_ms,
            error: opts.error,
            meta: opts.meta,
        };

        if inner.entries.len() >= MAX_LOG_ENTRIES {
            inner.entries.pop_front();
        }
        inner.entries.push_back(entry.clone());

        // Console logging at warn/error/critical
        match level {
            LogLevel::Error | LogLevel::Critical => {
                log::error!("[{}] {} — {}", entry.action, entry.level_as_str(), message)
            }
            LogLevel::Warn => {
                log::warn!("[{}] {} — {}", entry.action, entry.level_as_str(), message)
            }
            _ => {}
        }

        // Notify listeners
        for listener in &inner.listeners {
            listener(&entry);
        }
    }

    pub fn debug(&self, action: AssistantAction, message: &str, opts: LogOptions) {
        self.log(LogLevel::Debug, action, message, opts);
    }

    pub fn info(&self, action: AssistantAction, message: &str, opts: LogOptions) {
        self.log(LogLevel::Info, action, message, opts);
    }

    pub fn warn(&self, action: AssistantAction, message: &str, opts: LogOptions) {
        self.log(LogLevel::Warn, action, message, opts);
    }

    pub fn error(&self, action: AssistantAction, message: &str, opts: LogOptions) {
        self.log(LogLevel::Error, action, message, opts);
    }

    pub fn critical(&self, action: AssistantAction, message: &str, opts: LogOptions) {
        self.log(LogLevel::Critical, action, message, opts);
    }

    pub fn get_entries(&self, opts: &GetEntriesOptions) -> Vec<LogEntry> {
        let inner = self.inner.lock().unwrap();
        let mut results: Vec<LogEntry> = inner
            .entries
            .iter()
            .filter(|e| {
                if let Some(min_level) = opts.level {
                    if e.level < min_level {
                        return false;
                    }
                }
                if let Some(ref action) = opts.action {
                    if e.action != action.as_str() {
                        return false;
                    }
                }
                if let Some(since) = opts.since {
                    if e.timestamp < since {
                        return false;
                    }
                }
                true
            })
            .cloned()
            .collect();

        if let Some(limit) = opts.limit {
            let start = results.len().saturating_sub(limit);
            results = results.split_off(start);
        }

        results
    }

    pub fn get_error_count(&self, since_ms: u64) -> usize {
        let now = now_ms();
        let cutoff = now.saturating_sub(since_ms);
        let inner = self.inner.lock().unwrap();
        inner
            .entries
            .iter()
            .filter(|e| {
                e.timestamp >= cutoff
                    && (e.level == LogLevel::Error || e.level == LogLevel::Critical)
            })
            .count()
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.entries.clear();
    }

    pub fn on_log(&self, listener: LogListener) {
        let mut inner = self.inner.lock().unwrap();
        inner.listeners.push(listener);
    }
}

impl LogEntry {
    fn level_as_str(&self) -> &'static str {
        match self.level {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
            LogLevel::Critical => "CRITICAL",
        }
    }
}

use crate::utils::time::now_ms;

/// Options for a single log call.
#[derive(Debug, Clone, Default)]
pub struct LogOptions {
    pub correlation_id: Option<String>,
    pub provider: Option<String>,
    pub duration_ms: Option<u64>,
    pub error: Option<LogError>,
    pub meta: Option<serde_json::Value>,
}

/// Generate a correlation ID for linking related log entries.
pub fn create_correlation_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("corr-{}-{}", now_ms(), n)
}
