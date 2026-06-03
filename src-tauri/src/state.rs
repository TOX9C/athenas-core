use std::collections::HashMap;
use std::sync::Arc;

use athena_core::plan_manager::ExecutionPlan;
use athena_core::tool_executor::ToolEventSender;
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// TauriEventSender — real implementation (PTY operations are no-ops)
// ---------------------------------------------------------------------------

/// Real [`ToolEventSender`] that emits Tauri events for user questions,
/// plan updates, and plan evaluations. PTY operations are no-ops now that
/// the terminal crate has been removed.
///
/// Plan/notification events are currently no-ops; they can be extended
/// later to emit Tauri events over the app handle.
pub struct TauriEventSender {
    app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,
    pending_questions: Arc<std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>>,
}

impl TauriEventSender {
    pub fn new(
        app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,
        pending_questions: Arc<std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>>,
    ) -> Self {
        Self {
            app_handle,
            pending_questions,
        }
    }
}

impl ToolEventSender for TauriEventSender {
    fn agent_spawned(&self, _id: &str, _agent_type: &str, _agent_cmd: &str) {
        log::warn!("agent_spawned is a no-op: PTY support has been removed");
    }

    fn close_panes(&self, _pane_ids: &[String]) {
        log::warn!("close_panes is a no-op: PTY support has been removed");
    }

    fn pty_write(&self, _pane_id: &str, _data: &str) {
        log::warn!("pty_write is a no-op: PTY support has been removed");
    }

    fn has_session(&self, _pane_id: &str) -> bool {
        log::warn!("has_session is a no-op: PTY support has been removed");
        false
    }

    fn ask_user(&self, request_id: &str, question: &str, options: &[serde_json::Value]) -> String {
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        {
            let mut map = match self.pending_questions.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    log::error!("pending_questions lock poisoned in ask_user: {}", e);
                    return format!("error: pending_questions lock poisoned");
                }
            };
            map.insert(request_id.to_string(), tx);
        }

        let handle_guard = match self.app_handle.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("app_handle lock poisoned in ask_user: {}", e);
                return format!("error: app_handle lock poisoned");
            }
        };
        if let Some(ref handle) = *handle_guard {
            let payload = serde_json::json!({
                "requestId": request_id,
                "question": question,
                "options": options,
            });
            match serde_json::to_string(&payload) {
                Ok(payload_str) => {
                    if let Err(e) = handle.emit("athena:askUser", payload_str) {
                        log::warn!("failed to emit athena:askUser event: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("failed to serialize athena:askUser payload: {}", e);
                }
            }
        } else {
            log::error!("ask_user called but app_handle is not set");
            let mut map = match self.pending_questions.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::error!("pending_questions lock poisoned in ask_user cleanup: {}", e);
                    return format!("error: app_handle not available and lock poisoned");
                }
            };
            map.remove(request_id);
            return format!("error: app_handle not available");
        }
        drop(handle_guard);

        match rx.recv() {
            Ok(answer) => answer,
            Err(_) => {
                log::warn!(
                    "receiver channel closed for request_id: {} -- question was: {}",
                    request_id,
                    question
                );
                format!("error: user response channel closed")
            }
        }
    }

    fn plan_update(&self, _plan: &ExecutionPlan) {
        // TODO: Emit a Tauri event so the frontend can update the plan UI.
        // No-op for now to avoid silent drops; the plan_manager state is
        // already persisted internally.
    }

    fn plan_evaluated(
        &self,
        _plan_id: &str,
        _overall_status: &str,
        _step_evaluations: &[serde_json::Value],
        _next_action: &str,
        _reasoning: &str,
    ) {
        // TODO: Emit a Tauri event so the frontend can display evaluation results.
    }
}

// ---------------------------------------------------------------------------
// AppState
// ---------------------------------------------------------------------------

/// Shared application state managed by Tauri's state container.
///
/// Fields that are internally synchronized (`OutputBuffer`,
/// `NotificationService`, `PlanManager`, `AgentComms`) are stored as bare
/// `Arc<T>` -- no outer `Mutex` -- because those types manage their own
/// internal locking. This allows the same `Arc<T>` to be shared directly
/// with `ToolExecutor`, eliminating the previous bug where `ToolExecutor`
/// received separate, disconnected instances.
///
/// Types that require external synchronization (`SessionManager`,
/// `Osc633Parser`) remain wrapped in `Arc<Mutex<T>>`.
pub struct AppState {
    pub store: athena_store::KeyValueStore,
    pub session_store: athena_store::SessionStore,

    /// Tauri app handle for emitting events to the frontend.
    /// Set once after the Tauri app is built via `set_app_handle`.
    pub app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,

    #[allow(dead_code)]
    pub browser_manager: athena_browser::BrowserManager,
    #[allow(dead_code)]
    pub plugin_manager: athena_plugins::PluginManager,

    /// Internally synchronized -- no outer `Mutex` needed.
    /// The same `Arc` is shared with `ToolExecutor`.
    pub output_buffer: Arc<athena_core::output_buffer::OutputBuffer>,

    /// Internally synchronized -- no outer `Mutex` needed.
    /// The same `Arc` is shared with `ToolExecutor`.
    pub plan_manager: Arc<athena_core::plan_manager::PlanManager>,

    /// Internally synchronized -- no outer `Mutex` needed.
    /// The same `Arc` is shared with `ToolExecutor`.
    pub notification_service: Arc<athena_core::notification::NotificationService>,

    /// Internally synchronized -- no outer `Mutex` needed.
    /// The same `Arc` is shared with `ToolExecutor`.
    pub agent_comms: Arc<athena_core::agent_comms::AgentComms>,

    /// Internally synchronized -- no outer `Mutex` needed.
    /// Wired to the tool executor so LLM tool calls are dispatched
    /// through the same service instances and real PTY side-effects.
    pub orchestrator: Arc<tokio::sync::Mutex<athena_core::AthenaOrchestrator>>,

    /// Async services that require `tokio::sync::Mutex` for use across
    /// `.await` boundaries.
    pub mcp_server: Arc<tokio::sync::Mutex<athena_core::mcp::McpServer>>,
    pub swarm_coordinator: Arc<tokio::sync::Mutex<athena_core::swarm::SwarmCoordinator>>,

    /// PTY session manager for terminal operations.
    pub session_manager: Arc<tokio::sync::Mutex<athena_terminal::session::SessionManager>>,

    /// Requires `Mutex` because `Osc633Parser::feed` takes `&mut self`.
    pub shell_integration_parser:
        Arc<std::sync::Mutex<athena_core::shell_integration::Osc633Parser>>,

    /// Pending user-response questions shared between TauriEventSender and
    /// the athena_user_answer command.
    pub pending_questions: Arc<std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>>,

    /// Tool executor -- holds shared `Arc<T>` refs to the same service
    /// instances stored above, plus a `TauriEventSender` for real PTY
    /// side-effects.
    pub tool_executor: Arc<std::sync::Mutex<athena_core::tool_executor::ToolExecutor>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let store = match athena_store::KeyValueStore::with_name_sync("store") {
            Ok(s) => s,
            Err(e) => {
                log::error!("KeyValueStore init failed, using empty fallback: {e}");
                athena_store::KeyValueStore::with_name_sync("store")
                    .unwrap_or_else(|_| athena_store::KeyValueStore::new_empty())
            }
        };
        let session_store = match athena_store::SessionStore::new_sync() {
            Ok(s) => s,
            Err(e) => {
                log::error!("SessionStore init failed, using empty fallback: {e}");
                athena_store::SessionStore::new_sync()
                    .unwrap_or_else(|_| athena_store::SessionStore::new_empty())
            }
        };
        let browser_manager = athena_browser::BrowserManager::new();
        let plugin_manager = athena_plugins::PluginManager::new();

        // -- Create the shared service instances first (bare Arc<T>) -------

        let output_buffer = Arc::new(athena_core::output_buffer::OutputBuffer::new());
        let plan_manager = Arc::new(athena_core::plan_manager::PlanManager::new());
        let notification_service = Arc::new(athena_core::notification::NotificationService::new());
        let agent_comms = Arc::new(athena_core::agent_comms::AgentComms::new());

        let session_manager = Arc::new(tokio::sync::Mutex::new(
            athena_terminal::session::SessionManager::new(),
        ));

        let mcp_server = Arc::new(tokio::sync::Mutex::new(athena_core::mcp::McpServer::new()));
        let swarm_coordinator = Arc::new(tokio::sync::Mutex::new(
            athena_core::swarm::SwarmCoordinator::new(),
        ));
        let shell_integration_parser = Arc::new(std::sync::Mutex::new(
            athena_core::shell_integration::Osc633Parser::new(),
        ));

        // -- Build the event sender ----------------------------------------

        let app_handle = Arc::new(std::sync::Mutex::new(None::<AppHandle>));
        let pending_questions: Arc<
            std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
        > = Arc::new(std::sync::Mutex::new(HashMap::new()));

        let event_sender: Arc<dyn ToolEventSender> = Arc::new(TauriEventSender::new(
            Arc::clone(&app_handle),
            Arc::clone(&pending_questions),
        ));

        // -- Build ToolExecutor with the SAME Arc<T> instances ---------

        let tool_executor = Arc::new(std::sync::Mutex::new(
            athena_core::tool_executor::ToolExecutor::new(
                Arc::clone(&output_buffer),
                Arc::clone(&notification_service),
                Arc::clone(&plan_manager),
                Arc::clone(&agent_comms),
                event_sender,
            ),
        ));

        // -- Build the orchestrator, wired to the tool executor ------------

        let orchestrator = Arc::new(tokio::sync::Mutex::new(
            athena_core::AthenaOrchestrator::new_with_executor(Arc::clone(&tool_executor)),
        ));

        Self {
            store,
            session_store,
            app_handle,
            browser_manager,
            plugin_manager,
            output_buffer,
            plan_manager,
            notification_service,
            agent_comms,
            orchestrator,
            mcp_server,
            swarm_coordinator,
            session_manager,
            shell_integration_parser,
            pending_questions,
            tool_executor,
        }
    }

    /// Store the Tauri `AppHandle` so events can be emitted.
    /// Called once after the Tauri app is built.
    pub fn set_app_handle(&self, handle: AppHandle) {
        {
            let mut guard = match self.app_handle.lock() {
                Ok(g) => g,
                Err(e) => {
                    log::error!("app_handle lock poisoned in set_app_handle: {}", e);
                    return;
                }
            };
            *guard = Some(handle);
        } // Drop the lock before calling wire methods that also acquire it

        // Wire event emitters for all services
        self.wire_notification_events();
        self.wire_plan_manager_events();
        self.wire_output_buffer_events();
        self.wire_agent_comms_events();
        self.wire_swarm_events();
        self.wire_browser_events();
        self.wire_plugin_events();
    }

    /// PTY read loops are started per-session when pty_spawn is invoked.
    /// This method can be extended to manage session lifecycle events.
    pub fn wire_pty_events(&self) {
        // PTY read loops are started per-session in pty_spawn command.
    }

    /// Wire notification service events to Tauri event emissions.
    fn wire_notification_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!(
                    "app_handle lock poisoned in wire_notification_events: {}",
                    e
                );
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_notification_events called before set_app_handle");
                return;
            }
        };
        let notification_service = Arc::clone(&self.notification_service);
        notification_service.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit notification event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "failed to serialize data for notification event {}: {}",
                        channel,
                        e
                    );
                }
            }
        });
    }

    /// Wire plan manager events to Tauri event emissions.
    fn wire_plan_manager_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!(
                    "app_handle lock poisoned in wire_plan_manager_events: {}",
                    e
                );
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_plan_manager_events called before set_app_handle");
                return;
            }
        };
        let plan_manager = Arc::clone(&self.plan_manager);
        plan_manager.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit plan event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!("failed to serialize data for plan event {}: {}", channel, e);
                }
            }
        });
    }

    /// Wire output buffer events to Tauri event emissions.
    fn wire_output_buffer_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!(
                    "app_handle lock poisoned in wire_output_buffer_events: {}",
                    e
                );
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_output_buffer_events called before set_app_handle");
                return;
            }
        };
        let output_buffer = Arc::clone(&self.output_buffer);
        output_buffer.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit output buffer event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "failed to serialize data for output buffer event {}: {}",
                        channel,
                        e
                    );
                }
            }
        });
    }

    /// Wire agent comms events to Tauri event emissions.
    fn wire_agent_comms_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_agent_comms_events: {}", e);
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_agent_comms_events called before set_app_handle");
                return;
            }
        };
        let agent_comms = Arc::clone(&self.agent_comms);
        agent_comms.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit agent comms event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "failed to serialize data for agent comms event {}: {}",
                        channel,
                        e
                    );
                }
            }
        });
    }

    /// Wire swarm coordinator events to Tauri event emissions.
    fn wire_swarm_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_swarm_events: {}", e);
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_swarm_events called before set_app_handle");
                return;
            }
        };
        let swarm_coordinator = self.swarm_coordinator.clone();
        // Need to lock the tokio mutex to get a reference
        let swarm = match swarm_coordinator.blocking_lock() {
            guard => guard.clone(),
        };
        swarm.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit swarm event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "failed to serialize data for swarm event {}: {}",
                        channel,
                        e
                    );
                }
            }
        });
    }

    /// Wire browser manager events to Tauri event emissions.
    fn wire_browser_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_browser_events: {}", e);
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_browser_events called before set_app_handle");
                return;
            }
        };
        let browser_manager = self.browser_manager.clone();
        browser_manager.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit browser event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "failed to serialize data for browser event {}: {}",
                        channel,
                        e
                    );
                }
            }
        });
    }

    /// Wire plugin manager events to Tauri event emissions.
    fn wire_plugin_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_plugin_events: {}", e);
                return;
            }
        };
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_plugin_events called before set_app_handle");
                return;
            }
        };
        let plugin_manager = self.plugin_manager.clone();
        plugin_manager.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
            // Serialize to an owned String: `&Value` borrows shared across concurrent emit() calls have been observed to race in Tauri 2.
            match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit plugin event {}: {}", channel, e);
                    }
                }
                Err(e) => {
                    log::error!(
                        "failed to serialize data for plugin event {}: {}",
                        channel,
                        e
                    );
                }
            }
        });
    }
}
