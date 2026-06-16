use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use athena_core::plan_manager::ExecutionPlan;
use athena_core::tool_executor::ToolEventSender;
use tauri::{AppHandle, Emitter};

// ---------------------------------------------------------------------------
// TauriEventSender — real implementation (wired to SessionManager)
// ---------------------------------------------------------------------------

/// Real [`ToolEventSender`] that emits Tauri events for user questions,
/// plan updates, plan evaluations, and PTY operations.
///
/// PTY methods (`agent_spawned`, `close_panes`, `pty_write`, `has_session`)
/// delegate to the real `SessionManager` from `athena-terminal`.
///
/// Plan/notification events can be extended later to emit Tauri events
/// over the app handle.
pub struct TauriEventSender {
    app_handle: Arc<parking_lot::Mutex<Option<AppHandle>>>,
    pending_questions: Arc<parking_lot::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>>,
    session_manager: Arc<tokio::sync::Mutex<athena_terminal::session::SessionManager>>,
    /// Best-effort synchronous cache of "known" session IDs so that
    /// `has_session` can answer from a sync trait method.
    active_sessions: Arc<parking_lot::Mutex<HashSet<String>>>,
    /// Lazily-cached tokio runtime handle so we can spawn async work from
    /// sync trait methods (e.g. when called from spawn_blocking).
    runtime_handle: Arc<parking_lot::Mutex<Option<tokio::runtime::Handle>>>,
    output_buffer: Arc<athena_core::output_buffer::OutputBuffer>,
}

impl TauriEventSender {
    pub fn new(
        app_handle: Arc<parking_lot::Mutex<Option<AppHandle>>>,
        pending_questions: Arc<
            parking_lot::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
        >,
        session_manager: Arc<tokio::sync::Mutex<athena_terminal::session::SessionManager>>,
        output_buffer: Arc<athena_core::output_buffer::OutputBuffer>,
    ) -> Self {
        Self {
            app_handle,
            pending_questions,
            session_manager,
            active_sessions: Arc::new(parking_lot::Mutex::new(HashSet::new())),
            runtime_handle: Arc::new(parking_lot::Mutex::new(None)),
            output_buffer,
        }
    }

    /// Return a clone of the cached tokio runtime handle if available,
    /// otherwise try to acquire the current handle and cache it.
    fn get_runtime_handle(&self) -> Option<tokio::runtime::Handle> {
        {
            let guard = self.runtime_handle.lock();
            if let Some(h) = guard.as_ref() {
                return Some(h.clone());
            }
        }
        match tokio::runtime::Handle::try_current() {
            Ok(handle) => {
                let mut guard = self.runtime_handle.lock();
                *guard = Some(handle.clone());
                Some(handle)
            }
            Err(e) => {
                log::error!("No tokio runtime available: {}", e);
                None
            }
        }
    }
}

impl ToolEventSender for TauriEventSender {
    fn agent_spawned(&self, id: &str, _agent_type: &str, agent_cmd: &str) {
        // Track as active so `has_session` can answer synchronously.
        self.active_sessions.lock().insert(id.to_string());

        let session_manager = Arc::clone(&self.session_manager);
        let app_handle = self.app_handle.lock().clone();
        let id = id.to_string();
        let agent_cmd = agent_cmd.to_string();
        let output_buffer = Arc::clone(&self.output_buffer);

        let handle = match self.get_runtime_handle() {
            Some(h) => h,
            None => {
                log::error!("No tokio runtime for agent_spawned, cannot spawn PTY");
                return;
            }
        };

        handle.spawn(async move {
            let default_shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
            let cwd = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| "/".to_string());

            let sm = session_manager.lock().await;
            let session_result = sm.spawn(id.clone(), &default_shell, &cwd, 80, 24).await;
            drop(sm);

            match session_result {
                Ok(session) => {
                    if let Err(e) = session.write(agent_cmd.as_bytes()).await {
                        log::error!("Failed to write agent command to PTY {}: {}", id, e);
                        return;
                    }

                    if let Some(ref handle) = app_handle {
                        let session_id_for_loop = id.clone();
                        let handle = handle.clone();
                        tokio::spawn(async move {
                            crate::commands::pty_read_loop(handle, session_id_for_loop, session, output_buffer)
                                .await;
                        });
                    }

                    log::info!("PTY agent session spawned via agent_spawned: id={}", id);
                }
                Err(e) => {
                    log::error!("Failed to spawn PTY agent session {}: {}", id, e);
                }
            }
        });
    }

    fn close_panes(&self, pane_ids: &[String]) {
        // Remove from active set first so `has_session` reflects the change.
        let mut guard = self.active_sessions.lock();
        for pid in pane_ids {
            guard.remove(pid);
        }

        let session_manager = Arc::clone(&self.session_manager);
        let pane_ids: Vec<String> = pane_ids.to_vec();

        let handle = match self.get_runtime_handle() {
            Some(h) => h,
            None => {
                log::error!("No tokio runtime for close_panes, cannot kill PTY sessions");
                return;
            }
        };

        handle.spawn(async move {
            for pane_id in pane_ids {
                let sm = session_manager.lock().await;
                if let Err(e) = sm.kill(&pane_id).await {
                    log::error!("Failed to kill PTY session {}: {}", pane_id, e);
                }
            }
        });
    }

    fn pty_write(&self, pane_id: &str, data: &str) {
        let session_manager = Arc::clone(&self.session_manager);
        let pane_id = pane_id.to_string();
        let data = data.to_string();

        let handle = match self.get_runtime_handle() {
            Some(h) => h,
            None => {
                log::error!("No tokio runtime for pty_write, cannot write to PTY");
                return;
            }
        };

        handle.spawn(async move {
            let sm = session_manager.lock().await;
            if let Err(e) = sm.write(&pane_id, data.as_bytes()).await {
                log::error!("Failed to write to PTY session {}: {}", pane_id, e);
            }
        });
    }

    fn has_session(&self, pane_id: &str) -> bool {
        // Fast path: check the active sessions cache.
        {
            let guard = self.active_sessions.lock();
            if guard.contains(pane_id) {
                return true;
            }
        }
        // Fallback: ask the real session manager.
        //
        // We must NOT use `handle.block_on(...)` or `blocking_lock()` here:
        // both panic with "Cannot start a runtime from within a runtime" when
        // this method is invoked from a tokio worker thread (which it is —
        // `execute_tool` dispatch runs via `spawn_blocking`, and the orchestrator
        // calls `has_session` synchronously from there).
        //
        // `try_lock()` is non-blocking: if the SessionManager is contended it
        // returns `Err`, which we treat as "session not confirmed" (false) —
        // safe because the cache fast-path above handles the common case, and
        // callers only use this to decide whether to write to a PTY.
        let session_manager = Arc::clone(&self.session_manager);
        let lock_result = session_manager.try_lock();
        match lock_result {
            Ok(sm) => sm.has_session_sync(pane_id),
            Err(_) => false,
        }
    }

    fn ask_user(&self, request_id: &str, question: &str, options: &[serde_json::Value]) -> String {
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        {
            let mut map = self.pending_questions.lock();
            map.insert(request_id.to_string(), tx);
        }

        let handle_guard = self.app_handle.lock();
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
            self.pending_questions.lock().remove(request_id);
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
    pub store: Arc<athena_store::KeyValueStore>,
    pub session_store: Arc<athena_store::SessionStore>,

    /// Tauri app handle for emitting events to the frontend.
    /// Set once after the Tauri app is built via `set_app_handle`.
    pub app_handle: Arc<parking_lot::Mutex<Option<AppHandle>>>,

    #[allow(dead_code)]
    pub browser_manager: athena_browser::BrowserManager,
    /// Labels of active child webviews managed in-browser (right sidebar).
    pub child_webview_labels: parking_lot::Mutex<std::collections::HashSet<String>>,
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
        Arc<parking_lot::Mutex<athena_core::shell_integration::Osc633Parser>>,

    /// Pending user-response questions shared between TauriEventSender and
    /// the athena_user_answer command.
    pub pending_questions:
        Arc<parking_lot::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>>,

    /// Tool executor -- holds shared `Arc<T>` refs to the same service
    /// instances stored above, plus a `TauriEventSender` for real PTY
    /// side-effects.
    pub tool_executor: Arc<parking_lot::Mutex<athena_core::tool_executor::ToolExecutor>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

impl AppState {
    pub fn new() -> Self {
        let store = Arc::new(
            athena_store::KeyValueStore::with_name_sync("store").unwrap_or_else(|e| {
                log::error!("KeyValueStore init failed, using empty fallback: {e}");
                athena_store::KeyValueStore::new_empty()
            }),
        );
        let session_store = Arc::new(athena_store::SessionStore::new_sync().unwrap_or_else(|e| {
            log::error!("SessionStore init failed, using empty fallback: {e}");
            athena_store::SessionStore::new_empty()
        }));
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
        // Tool executor reference for MCP server — wire it up after both are created
        let swarm_coordinator = Arc::new(tokio::sync::Mutex::new(
            athena_core::swarm::SwarmCoordinator::new(),
        ));
        let shell_integration_parser = Arc::new(parking_lot::Mutex::new(
            athena_core::shell_integration::Osc633Parser::new(),
        ));

        // -- Build the event sender ----------------------------------------

        let app_handle = Arc::new(parking_lot::Mutex::new(None::<AppHandle>));
        let pending_questions: Arc<
            parking_lot::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
        > = Arc::new(parking_lot::Mutex::new(HashMap::new()));

        let event_sender: Arc<dyn ToolEventSender> = Arc::new(TauriEventSender::new(
            Arc::clone(&app_handle),
            Arc::clone(&pending_questions),
            Arc::clone(&session_manager),
            Arc::clone(&output_buffer),
        ));

        // -- Build ToolExecutor with the SAME Arc<T> instances ---------

        let tool_executor = Arc::new(parking_lot::Mutex::new(
            athena_core::tool_executor::ToolExecutor::new(
                Arc::clone(&output_buffer),
                Arc::clone(&notification_service),
                Arc::clone(&plan_manager),
                Arc::clone(&agent_comms),
                event_sender,
                Arc::clone(&store),
            ),
        ));

        // Wire the MCP server to the tool executor so external tools
        // (Claude Code, OpenCode) can control agents, kanban, files, etc.
        {
            let mut mcp = mcp_server.blocking_lock();
            mcp.tool_executor = Some(Arc::clone(&tool_executor));
        }

        // -- Build the orchestrator, wired to the tool executor ------------

        let orchestrator = {
            let orch = athena_core::AthenaOrchestrator::with_context(
                Arc::clone(&tool_executor),
                Arc::clone(&output_buffer),
                Arc::clone(&plan_manager),
                Arc::clone(&agent_comms),
                Some(Arc::clone(&session_store)),
            );

            // Try to restore the active workspace name from the store
            if let Ok(Some(json)) = store.get::<String>("workspaces") {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(&json) {
                    if let Some(active_id) = val.get("active_space_id").and_then(|v| v.as_str()) {
                        if let Some(spaces) = val.get("spaces").and_then(|v| v.as_array()) {
                            for space in spaces {
                                if let Some(id) = space.get("id").and_then(|v| v.as_str()) {
                                    if active_id == id {
                                        if let Some(name) =
                                            space.get("name").and_then(|v| v.as_str())
                                        {
                                            orch.set_workspace_name(name.to_string());
                                        }
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            Arc::new(tokio::sync::Mutex::new(orch))
        };

        Self {
            store,
            session_store,
            app_handle,
            browser_manager,
            child_webview_labels: parking_lot::Mutex::new(HashSet::new()),
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
            let mut guard = self.app_handle.lock();
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

        // Auto-start the MCP server on a background thread
        let mcp_server = Arc::clone(&self.mcp_server);
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Runtime::new() {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("Failed to create tokio runtime for MCP server: {}", e);
                    return;
                }
            };
            rt.block_on(async {
                let mut server = mcp_server.lock().await;
                if let Err(e) = server.init(4545) {
                    log::error!("Failed to start MCP server: {}", e);
                } else {
                    log::info!("MCP server auto-started on port 4545");
                }
            });
        });
    }

    /// Retrieve a clone of the Tauri `AppHandle`.
    pub fn get_app_handle(&self) -> Option<AppHandle> {
        self.app_handle.lock().clone()
    }

    /// PTY read loops are started per-session when pty_spawn is invoked.
    /// This method can be extended to manage session lifecycle events.
    pub fn wire_pty_events(&self) {
        // PTY read loops are started per-session in pty_spawn command.
    }

    /// Wire notification service events to Tauri event emissions.
    fn wire_notification_events(&self) {
        let handle = self.app_handle.lock().clone();
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
        let handle = self.app_handle.lock().clone();
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
        let handle = self.app_handle.lock().clone();
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
        let handle = self.app_handle.lock().clone();
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
        let handle = self.app_handle.lock().clone();
        let handle = match handle {
            Some(h) => h,
            None => {
                log::error!("wire_swarm_events called before set_app_handle");
                return;
            }
        };
        let swarm_coordinator = self.swarm_coordinator.clone();
        // Need to lock the tokio mutex to get a reference
        let swarm = swarm_coordinator.blocking_lock().clone();
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
        let handle = self.app_handle.lock().clone();
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
        let handle = self.app_handle.lock().clone();
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
