use std::collections::HashMap;
use std::sync::Arc;

use athena_core::plan_manager::ExecutionPlan;
use athena_core::tool_executor::ToolEventSender;
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// TauriEventSender — real implementation that bridges to SessionManager
// ---------------------------------------------------------------------------

/// Real [`ToolEventSender`] that delegates PTY operations to the
/// [`athena_terminal::SessionManager`] owned by [`AppState`].
///
/// Holds only the PTY-manager `Arc` to avoid a circular reference to the
/// full `AppState`. Plan/notification events are currently no-ops; they
/// can be extended later to emit Tauri events over the app handle.
pub struct TauriEventSender {
    pty_manager: Arc<std::sync::Mutex<athena_terminal::SessionManager>>,
    app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,
    pending_questions: Arc<
        std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
    >,
}

impl TauriEventSender {
    pub fn new(
        pty_manager: Arc<std::sync::Mutex<athena_terminal::SessionManager>>,
        app_handle: Arc<std::sync::Mutex<Option<AppHandle>>>,
        pending_questions: Arc<
            std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
        >,
    ) -> Self {
        Self {
            pty_manager,
            app_handle,
            pending_questions,
        }
    }
}

impl ToolEventSender for TauriEventSender {
    fn agent_spawned(&self, id: &str, agent_type: &str, agent_cmd: &str) {
        let manager = match self.pty_manager.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "pty_manager lock poisoned in agent_spawned: pane_id={}, error={}",
                    id,
                    e
                );
                return;
            }
        };

        let cwd = std::env::var("HOME").unwrap_or_else(|_| "/".to_string());
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());

        if let Err(e) = manager.spawn(id.to_string(), cwd, shell, None) {
            log::error!("failed to spawn PTY for agent: pane_id={}, error={}", id, e);
            return;
        }

        // If an agent command was provided, write it into the freshly spawned
        // PTY so the agent starts executing immediately.
        if !agent_cmd.is_empty() {
            if let Err(e) = manager.write(id, format!("{}\r", agent_cmd)) {
                log::error!(
                    "failed to write agent command to PTY: pane_id={}, error={}",
                    id,
                    e
                );
            }
        }

        drop(manager);

        log::info!(
            "agent spawned via TauriEventSender: pane_id={}, agent_type={}",
            id,
            agent_type
        );
    }

    fn close_panes(&self, pane_ids: &[String]) {
        let manager = match self.pty_manager.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!("pty_manager lock poisoned in close_panes: error={}", e);
                return;
            }
        };

        for pane_id in pane_ids {
            manager.kill(pane_id);
            log::info!("pane killed via TauriEventSender: pane_id={}", pane_id);
        }
    }

    fn pty_write(&self, pane_id: &str, data: &str) {
        let manager = match self.pty_manager.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "pty_manager lock poisoned in pty_write: pane_id={}, error={}",
                    pane_id,
                    e
                );
                return;
            }
        };

        if let Err(e) = manager.write(pane_id, data.to_string()) {
            log::error!("failed to write to PTY: pane_id={}, error={}", pane_id, e);
        }
    }

    fn has_session(&self, pane_id: &str) -> bool {
        let manager = match self.pty_manager.lock() {
            Ok(guard) => guard,
            Err(e) => {
                log::error!(
                    "pty_manager lock poisoned in has_session: pane_id={}, error={}",
                    pane_id,
                    e
                );
                return false;
            }
        };

        manager.has_session(pane_id)
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
            if let Err(e) = handle.emit("athena:askUser", &payload) {
                log::warn!("failed to emit athena:askUser event: {}", e);
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

    /// PTY session manager. Requires `Mutex` because `SessionManager` is
    /// not `Clone` and exposes `&mut self`-style semantics through interior
    /// locking of its session map.
    pub pty_manager: Arc<std::sync::Mutex<athena_terminal::SessionManager>>,

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

    /// Requires `Mutex` because `Osc633Parser::feed` takes `&mut self`.
    pub shell_integration_parser:
        Arc<std::sync::Mutex<athena_core::shell_integration::Osc633Parser>>,

    /// Pending user-response questions shared between TauriEventSender and
    /// the athena_user_answer command.
    pub pending_questions: Arc<
        std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
    >,

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
                athena_store::KeyValueStore::with_name_sync("store").unwrap_or_else(|_| {
                    athena_store::KeyValueStore::new_empty()
                })
            }
        };
        let session_store = match athena_store::SessionStore::new_sync() {
            Ok(s) => s,
            Err(e) => {
                log::error!("SessionStore init failed, using empty fallback: {e}");
                athena_store::SessionStore::new_sync().unwrap_or_else(|_| {
                    athena_store::SessionStore::new_empty()
                })
            }
        };
        let browser_manager = athena_browser::BrowserManager::new();
        let plugin_manager = athena_plugins::PluginManager::new();

        // -- Create the shared service instances first (bare Arc<T>) -------

        let pty_manager = Arc::new(std::sync::Mutex::new(athena_terminal::SessionManager::new()));

        let output_buffer = Arc::new(athena_core::output_buffer::OutputBuffer::new());
        let plan_manager = Arc::new(athena_core::plan_manager::PlanManager::new());
        let notification_service = Arc::new(athena_core::notification::NotificationService::new());
        let agent_comms = Arc::new(athena_core::agent_comms::AgentComms::new());

        let mcp_server = Arc::new(tokio::sync::Mutex::new(athena_core::mcp::McpServer::new()));
        let swarm_coordinator = Arc::new(tokio::sync::Mutex::new(
            athena_core::swarm::SwarmCoordinator::new(),
        ));
        let shell_integration_parser = Arc::new(std::sync::Mutex::new(
            athena_core::shell_integration::Osc633Parser::new(),
        ));

        // -- Build the event sender with a clone of the PTY manager Arc ---

        let app_handle = Arc::new(std::sync::Mutex::new(None::<AppHandle>));
        let pending_questions: Arc<
            std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
        > = Arc::new(std::sync::Mutex::new(HashMap::new()));

        let event_sender: Arc<dyn ToolEventSender> = Arc::new(TauriEventSender::new(
            Arc::clone(&pty_manager),
            Arc::clone(&app_handle),
            Arc::clone(&pending_questions),
        ));

        // -- Build ToolExecutor with the SAME Arc<T> instances -----------

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
            pty_manager,
            browser_manager,
            plugin_manager,
            output_buffer,
            plan_manager,
            notification_service,
            agent_comms,
            orchestrator,
            mcp_server,
            swarm_coordinator,
            shell_integration_parser,
            pending_questions,
            tool_executor,
        }
    }

    /// Store the Tauri `AppHandle` so PTY data events can be emitted.
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

    /// Replace the PTY manager with one that has data, ready, and exit callbacks wired
    /// to emit `terminal:data:{pane_id}`, `terminal:ready:{pane_id}`, and
    /// `terminal:exit:{pane_id}` events via the stored `AppHandle`.
    /// Must be called after `set_app_handle`.
    pub fn wire_pty_events(&self) {
        let handle_guard = match self.app_handle.lock() {
            Ok(g) => g,
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_pty_events: {}", e);
                return;
            }
        };
        let handle = match handle_guard.as_ref() {
            Some(h) => h.clone(),
            None => {
                log::error!("wire_pty_events called before set_app_handle");
                return;
            }
        };
        drop(handle_guard);

        let output_buffer = Arc::clone(&self.output_buffer);
        let handle_for_ready = handle.clone();
        let handle_for_exit = handle.clone();
        let output_buffer_for_exit = Arc::clone(&output_buffer);
        let new_manager = athena_terminal::SessionManager::new_with_callbacks(
            Some(Box::new(move |id: &str, data: &[u8]| {
                let event_name = format!("terminal:data:{}", id);
                let payload = String::from_utf8_lossy(data).into_owned();
                if let Err(e) = handle.emit(&event_name, &payload) {
                    log::warn!("failed to emit terminal data event: {}", e);
                }
                // Also append to output buffer
                output_buffer.append_output(id, &payload, None);
            })),
            Some(Box::new(move |id: &str| {
                let event_name = format!("terminal:ready:{}", id);
                if let Err(e) = handle_for_ready.emit(&event_name, &serde_json::json!({ "id": id })) {
                    log::warn!("failed to emit terminal ready event: {}", e);
                }
            })),
            Some(Box::new(move |id: &str, exit_code: Option<i32>| {
                let event_name = format!("terminal:exit:{}", id);
                if let Err(e) = handle_for_exit.emit(&event_name, &serde_json::json!({ "id": id, "exitCode": exit_code })) {
                    log::warn!("failed to emit terminal exit event: {}", e);
                }
                // Mark pane as dead in output buffer
                output_buffer_for_exit.mark_pane_dead(id);
            })),
        );

        let mut pty_guard = match self.pty_manager.lock() {
            Ok(g) => g,
            Err(e) => {
                log::error!("pty_manager lock poisoned in wire_pty_events: {}", e);
                return;
            }
        };
        *pty_guard = new_manager;
    }

    /// Wire notification service events to Tauri event emissions.
    fn wire_notification_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_notification_events: {}", e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit notification event {}: {}", channel, e);
            }
        });
    }

    /// Wire plan manager events to Tauri event emissions.
    fn wire_plan_manager_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_plan_manager_events: {}", e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit plan event {}: {}", channel, e);
            }
        });
    }

    /// Wire output buffer events to Tauri event emissions.
    fn wire_output_buffer_events(&self) {
        let handle = match self.app_handle.lock() {
            Ok(g) => g.clone(),
            Err(e) => {
                log::error!("app_handle lock poisoned in wire_output_buffer_events: {}", e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit output buffer event {}: {}", channel, e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit agent comms event {}: {}", channel, e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit swarm event {}: {}", channel, e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit browser event {}: {}", channel, e);
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
            if let Err(e) = handle.emit(channel, data) {
                log::warn!("failed to emit plugin event {}: {}", channel, e);
            }
        });
    }
}
