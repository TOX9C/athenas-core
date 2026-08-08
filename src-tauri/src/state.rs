use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use athena_core::plan_manager::ExecutionPlan;
use athena_core::tool_executor::ToolEventSender;
use tauri::{AppHandle, Emitter, Manager};

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
    agent_activity: Arc<athena_core::agent_activity::AgentActivityTracker>,
}

impl TauriEventSender {
    pub fn new(
        app_handle: Arc<parking_lot::Mutex<Option<AppHandle>>>,
        pending_questions: Arc<
            parking_lot::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>,
        >,
        session_manager: Arc<tokio::sync::Mutex<athena_terminal::session::SessionManager>>,
        output_buffer: Arc<athena_core::output_buffer::OutputBuffer>,
        agent_activity: Arc<athena_core::agent_activity::AgentActivityTracker>,
    ) -> Self {
        Self {
            app_handle,
            pending_questions,
            session_manager,
            active_sessions: Arc::new(parking_lot::Mutex::new(HashSet::new())),
            runtime_handle: Arc::new(parking_lot::Mutex::new(None)),
            output_buffer,
            agent_activity,
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
    fn agent_spawned(&self, id: &str, agent_type: &str, agent_cmd: &str) {
        // Surface tool-launched agents immediately rather than waiting for the
        // heartbeat's foreground-process classifier. The tracker deduplicates
        // this against any later lifecycle signal for the same pane.
        let agent_key = if athena_core::agent_detection::is_known_agent_key(agent_type) {
            agent_type
        } else {
            agent_cmd
                .split_whitespace()
                .find_map(|word| {
                    let base = word.rsplit('/').next().unwrap_or(word);
                    athena_core::agent_detection::is_known_agent_key(base).then_some(base)
                })
                .unwrap_or("agent")
        };
        self.agent_activity
            .notify_agent_started(id, agent_key, crate::commands::now_ms());

        // Track as active so `has_session` can answer synchronously.
        self.active_sessions.lock().insert(id.to_string());

        let session_manager = Arc::clone(&self.session_manager);
        let app_handle = self.app_handle.lock().clone();
        let id = id.to_string();
        let agent_cmd = agent_cmd.to_string();
        let output_buffer = Arc::clone(&self.output_buffer);
        let agent_activity = Arc::clone(&self.agent_activity);

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
                        let tracker = Arc::clone(&agent_activity);
                        tokio::spawn(async move {
                            crate::commands::pty_read_loop(
                                handle,
                                session_id_for_loop,
                                session,
                                output_buffer,
                                Some(tracker),
                            )
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
            return "error: app_handle not available".to_string();
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
                "error: user response channel closed".to_string()
            }
        }
    }

    fn plan_update(&self, plan: &ExecutionPlan) {
        let handle_guard = self.app_handle.lock();
        if let Some(ref handle) = *handle_guard {
            let payload = serde_json::json!({
                "plan_id": plan.id,
                "goal": plan.goal,
                "steps": plan.steps.iter().map(|s| serde_json::json!({
                    "id": s.id,
                    "title": s.description,
                    "description": s.description,
                    "status": match s.status {
                        athena_core::plan_manager::StepStatus::Pending => "pending",
                        athena_core::plan_manager::StepStatus::InProgress => "in_progress",
                        athena_core::plan_manager::StepStatus::Completed => "completed",
                        athena_core::plan_manager::StepStatus::Failed => "failed",
                        athena_core::plan_manager::StepStatus::Cancelled => "cancelled",
                    },
                    "assignedPaneId": s.assigned_pane_id,
                })).collect::<Vec<_>>(),
                "status": match plan.status {
                    athena_core::plan_manager::PlanStatus::Pending => "pending",
                    athena_core::plan_manager::PlanStatus::InProgress => "in_progress",
                    athena_core::plan_manager::PlanStatus::Completed => "completed",
                    athena_core::plan_manager::PlanStatus::Failed => "failed",
                    athena_core::plan_manager::PlanStatus::Cancelled => "cancelled",
                },
            });
            if let Err(e) = handle.emit("athena:planUpdate", payload.to_string()) {
                log::warn!("failed to emit athena:planUpdate event: {}", e);
            }
        }
    }

    fn plan_evaluated(
        &self,
        plan_id: &str,
        overall_status: &str,
        step_evaluations: &[serde_json::Value],
        next_action: &str,
        reasoning: &str,
    ) {
        let handle_guard = self.app_handle.lock();
        if let Some(ref handle) = *handle_guard {
            let payload = serde_json::json!({
                "planId": plan_id,
                "overallStatus": overall_status,
                "stepEvaluations": step_evaluations,
                "nextAction": next_action,
                "reasoning": reasoning,
            });
            if let Err(e) = handle.emit("athena:planEvaluated", payload.to_string()) {
                log::warn!("failed to emit athena:planEvaluated event: {}", e);
            }
        }
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

    pub browser_manager: athena_browser::BrowserManager,
    /// Labels of active child webviews managed in-browser (right sidebar).
    pub child_webview_labels: parking_lot::Mutex<std::collections::HashSet<String>>,
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

    /// Backend-owned per-pane agent activity state machine. Emits
    /// `agent:status` events and pushes notifications on transitions.
    pub agent_activity: Arc<athena_core::agent_activity::AgentActivityTracker>,

    /// Internally synchronized -- no outer `Mutex` needed.
    /// Wired to the tool executor so LLM tool calls are dispatched
    /// through the same service instances and real PTY side-effects.
    pub orchestrator: Arc<athena_core::AthenaOrchestrator>,

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

    /// Per-command rate limiter to prevent DoS from IPC spam.
    pub rate_limiter: crate::commands::caps::RateLimiter,

    /// Guard to ensure the MCP background runtime is only started once.
    pub mcp_stdio_started: Arc<AtomicBool>,
    /// Signals the dedicated MCP runtime thread to stop during app shutdown.
    pub mcp_runtime_stop: Arc<AtomicBool>,
    /// Kanban backend — same `Arc<KanbanBackend>` is shared with the
    /// ToolExecutor's internally-held instance (the store clone keeps storage
    /// consistent via the same backing KeyValueStore).
    pub kanban_backend: Arc<athena_core::kanban::KanbanBackend>,
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
        let notification_service = Arc::new(
            athena_core::notification::NotificationService::new_with_store(Arc::clone(&store)),
        );
        let agent_comms = Arc::new(athena_core::agent_comms::AgentComms::new());
        let agent_activity = Arc::new(athena_core::agent_activity::AgentActivityTracker::new(
            Some(Arc::clone(&notification_service)),
        ));

        let session_manager = Arc::new(tokio::sync::Mutex::new(
            athena_terminal::session::SessionManager::new(),
        ));

        let mcp_runtime_stop = Arc::new(AtomicBool::new(false));
        let mcp_server = Arc::new(tokio::sync::Mutex::new(
            athena_core::mcp::McpServer::new_with_shutdown(Arc::clone(&mcp_runtime_stop)),
        ));
        // Tool executor reference for MCP server — wire it up after both are created
        let swarm_coordinator = Arc::new(tokio::sync::Mutex::new(
            athena_core::swarm::SwarmCoordinator::new(),
        ));
        let shell_integration_parser = Arc::new(parking_lot::Mutex::new(
            athena_core::shell_integration::Osc633Parser::new(),
        ));
        let kanban_backend = Arc::new(athena_core::kanban::KanbanBackend::new(Arc::clone(&store)));

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
            Arc::clone(&agent_activity),
        ));

        // -- Build ToolExecutor with the SAME Arc<T> instances ---------

        let tool_executor = Arc::new(parking_lot::Mutex::new(
            athena_core::tool_executor::ToolExecutor::new(
                Arc::clone(&output_buffer),
                Arc::clone(&plan_manager),
                Arc::clone(&agent_comms),
                event_sender,
                Arc::clone(&store),
                Some(Arc::clone(&notification_service)),
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
                Some(Arc::clone(&store)),
                Some(Arc::clone(&notification_service)),
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

            Arc::new(orch)
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
            agent_activity,
            orchestrator,
            mcp_server,
            swarm_coordinator,
            session_manager,
            shell_integration_parser,
            kanban_backend,
            pending_questions,
            rate_limiter: crate::commands::caps::global_rate_limiter(),
            mcp_stdio_started: Arc::new(AtomicBool::new(false)),
            mcp_runtime_stop,
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
        self.wire_agent_activity_events();

        // Start the agent-activity heartbeat poll on its own runtime.
        self.spawn_agent_activity_heartbeat();

        // One-time dock badge sync so an unread count persisted across
        // restarts shows immediately (event-driven updates only fire on
        // notification state changes).
        #[cfg(target_os = "macos")]
        if let Some(window) = self
            .app_handle
            .lock()
            .as_ref()
            .and_then(|h| h.get_webview_window("main"))
        {
            let unread = self.notification_service.get_unread_count();
            if let Err(e) = window.set_badge_count(Some(unread as i64)) {
                log::warn!("failed to set dock badge at startup: {e}");
            }
        }

        self.start_mcp_runtime();
    }

    /// Start the boot-time MCP TCP and stdio transports on a dedicated runtime.
    ///
    /// Startup is retried while the app is alive if runtime creation or the
    /// canonical TCP bind fails. Keeping the retry loop here makes resetting a
    /// one-time guard unnecessary: a failed first attempt cannot permanently
    /// disable MCP for the rest of the process.
    fn start_mcp_runtime(&self) {
        if self
            .mcp_stdio_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            != Ok(false)
        {
            return;
        }

        let mcp_server = Arc::clone(&self.mcp_server);
        let mcp_runtime_stop = Arc::clone(&self.mcp_runtime_stop);
        let mcp_stdio_started = Arc::clone(&self.mcp_stdio_started);
        std::thread::spawn(move || {
            let rt = loop {
                if mcp_runtime_stop.load(Ordering::Relaxed) {
                    mcp_stdio_started.store(false, Ordering::SeqCst);
                    return;
                }
                match tokio::runtime::Runtime::new() {
                    Ok(rt) => break rt,
                    Err(e) => {
                        log::error!("Failed to create tokio runtime for MCP server: {e}");
                        std::thread::sleep(std::time::Duration::from_millis(500));
                    }
                }
            };

            rt.block_on(async {
                loop {
                    if mcp_runtime_stop.load(Ordering::Relaxed) {
                        break;
                    }

                    let started = {
                        let mut server = mcp_server.lock().await;
                        match server.init(4545) {
                            Ok(()) => {
                                server.init_stdio();
                                true
                            }
                            Err(e) => {
                                log::error!(
                                    "Failed to start MCP TCP server on 127.0.0.1:4545: {e}; retrying"
                                );
                                false
                            }
                        }
                    };

                    if started {
                        log::info!("MCP TCP server started on 127.0.0.1:4545");
                        log::info!("MCP stdio server started");

                        // `McpServer::init` and `init_stdio` spawn their tasks
                        // on this runtime. Keep it alive until app shutdown;
                        // dropping it here would abort both servers.
                        while !mcp_runtime_stop.load(Ordering::Relaxed) {
                            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                        }
                        break;
                    }

                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
            });
            mcp_stdio_started.store(false, Ordering::SeqCst);
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

    /// Generic helper to wire a service's event emitter to the Tauri app handle.
    ///
    /// Serializes the `&serde_json::Value` to an owned `String` before
    /// calling `handle.emit` to avoid Tauri 2 race conditions where borrows
    /// are shared across concurrent emit() calls.
    fn wire_emitter(
        &self,
        name: &str,
        setter: impl FnOnce(Box<dyn Fn(&str, &serde_json::Value) + Send + Sync>),
    ) {
        let handle = match self.app_handle.lock().clone() {
            Some(h) => h,
            None => {
                log::error!("wire_{name}_events called before set_app_handle");
                return;
            }
        };
        let name = name.to_string();
        setter(Box::new(
            move |channel: &str, data: &serde_json::Value| match serde_json::to_string(data) {
                Ok(data_str) => {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit {name} event {channel}: {e}");
                    }
                }
                Err(e) => {
                    log::error!("failed to serialize data for {name} event {channel}: {e}");
                }
            },
        ));
    }

    /// Wire notification service events to Tauri event emissions.
    ///
    /// Every `notifications:new` is forwarded to the frontend (existing
    /// in-app behavior). On top of that, agent-source notifications are
    /// mirrored to a **native macOS notification with sound** (the app may
    /// be backgrounded while an agent finishes), and the **dock badge** is
    /// kept in sync with the unread count. Both are no-ops on non-macOS
    /// targets (the badge path falls back to a harmless no-op elsewhere).
    fn wire_notification_events(&self) {
        let service = Arc::clone(&self.notification_service);
        let service_for_closure = Arc::clone(&service);
        let handle = match self.app_handle.lock().clone() {
            Some(h) => h,
            None => {
                log::error!("wire_notification_events called before set_app_handle");
                return;
            }
        };
        service.set_event_emitter(Box::new(
            move |channel: &str, data: &serde_json::Value| {
                // 1) Forward to the frontend exactly as before.
                if let Ok(data_str) = serde_json::to_string(data) {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit notification event {channel}: {e}");
                    }
                }

                if channel == "notifications:new" {
                    // 2) Mirror agent-source notifications to macOS with a
                    // per-type sound. Only "agent" source (the activity
                    // tracker) hits the OS — generic backend/tool noise stays
                    // in-app. The payload's `source`/`agentId` come straight
                    // from NotificationEvent.
                    let is_agent = data
                        .get("source")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "agent")
                        .unwrap_or(false);
                    if is_agent {
                        let title = data
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("Agent")
                            .to_string();
                        let body = data
                            .get("message")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string();
                        let ntype = data
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("info")
                            .to_string();
                        let sound = match ntype.as_str() {
                            "needs_input" => "Sosumi",
                            "task_error" | "error" => "Basso",
                            _ => "Glass",
                        };
                        #[cfg(target_os = "macos")]
                        {
                            use tauri_plugin_notification::NotificationExt;
                            if let Err(e) = handle
                                .notification()
                                .builder()
                                .title(title)
                                .body(body)
                                .sound(sound)
                                .show()
                            {
                                log::warn!("failed to show macOS notification: {e}");
                            }
                        }
                    }
                }

                // 3) Dock badge = unread count (all sources), refreshed on
                // every notification state change (new / mark-read /
                // dismiss), so marking a notification read in the UI clears
                // the badge without a restart.
                #[cfg(target_os = "macos")]
                if matches!(
                    channel,
                    "notifications:new"
                        | "notifications:updated"
                        | "notifications:dismissed"
                        | "notifications:cleared"
                ) {
                    let unread = service_for_closure.get_unread_count();
                    if let Some(window) = handle.get_webview_window("main") {
                        if let Err(e) = window.set_badge_count(Some(unread as i64)) {
                            log::warn!("failed to set dock badge: {e}");
                        }
                    }
                }
            },
        ));
    }

    /// Wire plan manager events to Tauri event emissions.
    fn wire_plan_manager_events(&self) {
        let service = Arc::clone(&self.plan_manager);
        self.wire_emitter("plan", move |emitter| {
            service.set_event_emitter(emitter);
        });
    }

    /// Wire output buffer events to Tauri event emissions.
    fn wire_output_buffer_events(&self) {
        let service = Arc::clone(&self.output_buffer);
        self.wire_emitter("output_buffer", move |emitter| {
            service.set_event_emitter(emitter);
        });
    }

    /// Wire agent comms events to Tauri emissions, enriching `agents:*`
    /// payloads with the frontend `paneId` (the SINGLE translation owner).
    ///
    /// Agent-comms events carry `sessionId`/`agentId` only; the frontend's
    /// `agents:*` listeners read `paneId` and drop events without it. The
    /// plugin-host registry is keyed by pane id, so we resolve agentId →
    /// paneId here (fallback: the plugin may pass its pane id AS the agent id).
    fn wire_agent_comms_events(&self) {
        let service = Arc::clone(&self.agent_comms);
        let plugin_manager = self.plugin_manager.clone();
        let notification_service = Arc::clone(&self.notification_service);
        let handle = match self.app_handle.lock().clone() {
            Some(h) => h,
            None => {
                log::error!("wire_agent_comms_events called before set_app_handle");
                return;
            }
        };
        service.set_event_emitter(Box::new(
            move |channel: &str, data: &serde_json::Value| {
                let mut data = data.clone();
                if channel.starts_with("agents:") && data.get("paneId").is_none() {
                    if let Some(agent_id) = data.get("agentId").and_then(|v| v.as_str()) {
                        let pane_id = plugin_manager
                            .get_session_by_agent_id(agent_id)
                            .and_then(|s| s.pane_id)
                            .or_else(|| {
                                agent_id
                                    .starts_with("pane")
                                    .then(|| agent_id.to_string())
                            });
                        if let Some(pid) = pane_id {
                            if let Some(obj) = data.as_object_mut() {
                                obj.insert("paneId".into(), serde_json::Value::String(pid));
                            }
                        }
                    }
                }

                // Normalize the plugin protocol's `active` / `waiting_input`
                // vocabulary to the frontend AgentRunStatus contract. Without
                // this adapter, plugin activity silently falls through to Idle.
                if channel == "agents:statusUpdate" {
                    let status = data
                        .get("status")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    if let Some(status) = status {
                        let normalized = match status.as_str() {
                            "active" | "running" => "working",
                            "waiting_input" => "waiting_for_input",
                            "done" | "complete" => "completed",
                            other => other,
                        };
                        if normalized != status {
                            if let Some(obj) = data.as_object_mut() {
                                obj.insert(
                                    "status".into(),
                                    serde_json::Value::String(normalized.to_string()),
                                );
                            }
                        }
                    }
                }

                // Plugin notifications are high-fidelity equivalents of the
                // passive tracker notifications. Route them through the
                // shared service so the existing frontend, macOS sound, and
                // dock-badge paths all behave consistently.
                if channel == "agents:notification" {
                    let level = data
                        .get("level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("info");
                    let notification_type = match level {
                        "needs_input" | "waiting_input" => {
                            athena_core::notification::NotificationType::NeedsInput
                        }
                        "error" => athena_core::notification::NotificationType::TaskError,
                        "success" | "completed" | "done" => {
                            athena_core::notification::NotificationType::Success
                        }
                        "warning" => athena_core::notification::NotificationType::Warning,
                        _ => athena_core::notification::NotificationType::Info,
                    };
                    let pane_id = data
                        .get("paneId")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    notification_service.push_notification(
                        athena_core::notification::NotificationEvent {
                            r#type: notification_type,
                            title: data
                                .get("title")
                                .and_then(|v| v.as_str())
                                .unwrap_or("Agent notification")
                                .to_string(),
                            message: data
                                .get("message")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            source: "agent".to_string(),
                            agent_id: pane_id.clone(),
                            data: Some(data.clone()),
                            timestamp: data
                                .get("timestamp")
                                .and_then(|v| v.as_u64())
                                .unwrap_or_else(crate::commands::now_ms),
                            metadata: None,
                            actions: None,
                            request_id: None,
                        },
                    );
                    return;
                }

                if let Ok(data_str) = serde_json::to_string(&data) {
                    if let Err(e) = handle.emit(channel, data_str) {
                        log::warn!("failed to emit agent_comms event {channel}: {e}");
                    }
                }
            },
        ));
    }

    /// Wire agent-activity events (`agent:status`) to Tauri emissions.
    fn wire_agent_activity_events(&self) {
        let service = Arc::clone(&self.agent_activity);
        self.wire_emitter("agent_activity", move |emitter| {
            service.set_event_emitter(emitter);
        });
    }

    /// Slow heartbeat poll: for every live PTY session, classify the
    /// foreground process (spawn_blocking `ps`), scrape the agent's session
    /// file when an agent is present, and feed the activity tracker.
    fn spawn_agent_activity_heartbeat(&self) {
        let session_manager = Arc::clone(&self.session_manager);
        let agent_activity = Arc::clone(&self.agent_activity);
        let output_buffer = Arc::clone(&self.output_buffer);
        let plugin_manager = self.plugin_manager.clone();
        let store = Arc::clone(&self.store);
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    log::error!("failed to create agent-activity heartbeat runtime: {}", e);
                    return;
                }
            };
            rt.block_on(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(1500));
                loop {
                    interval.tick().await;

                    // Apply the persisted per-type notification config (the
                    // frontend writes it via the existing `store_set` IPC —
                    // no new command surface needed). Best-effort: missing /
                    // malformed JSON keeps the previous config.
                    if let Ok(Some(json)) = store.get::<String>("agent_notify_config") {
                        if let Ok(cfg) = serde_json::from_str::<
                            athena_core::agent_activity::AgentNotifyConfig,
                        >(&json)
                        {
                            agent_activity.set_notify_config(cfg);
                        }
                    }

                    let sessions = {
                        let sm = session_manager.lock().await;
                        sm.list_sessions().await
                    };
                    if sessions.is_empty() {
                        continue;
                    }
                    let plugin_panes: HashSet<String> = plugin_manager
                        .list_sessions()
                        .iter()
                        .filter_map(|s| s.pane_id.clone())
                        .collect();
                    for sid in &sessions {
                        let connected = plugin_panes.contains(sid);
                        agent_activity.set_plugin_connected(sid, connected);
                        let session = {
                            let sm = session_manager.lock().await;
                            sm.get_session(sid).await
                        };
                        let Some(session) = session else { continue };
                        let fg = crate::commands::session_foreground_label(&session).await;
                        let fg_opt = if fg == "shell" { None } else { Some(fg) };
                        // Only scrape history / tail when an agent is present
                        // (never for plain shell panes).
                        let history = fg_opt
                            .as_deref()
                            .and_then(athena_core::agent_detection::scrape_agent_history);
                        let tail = if fg_opt.is_some() {
                            let lines = output_buffer.get_output(sid, None);
                            Some(
                                lines
                                    .iter()
                                    .rev()
                                    .take(40)
                                    .rev()
                                    .map(|l| l.text.as_str())
                                    .collect::<Vec<_>>()
                                    .join("\n"),
                            )
                        } else {
                            None
                        };
                        agent_activity.heartbeat(
                            sid,
                            fg_opt.as_deref(),
                            history.as_ref(),
                            tail.as_deref(),
                            crate::commands::now_ms(),
                        );
                    }
                }
            });
        });
    }

    /// Wire swarm coordinator events to Tauri event emissions.
    fn wire_swarm_events(&self) {
        let swarm = self.swarm_coordinator.clone().blocking_lock().clone();
        self.wire_emitter("swarm", move |emitter| {
            swarm.set_event_emitter(emitter);
        });
    }

    /// Wire browser manager events to Tauri event emissions.
    fn wire_browser_events(&self) {
        let service = self.browser_manager.clone();
        self.wire_emitter("browser", move |emitter| {
            service.set_event_emitter(emitter);
        });
    }

    /// Wire plugin manager events to Tauri event emissions.
    fn wire_plugin_events(&self) {
        let service = self.plugin_manager.clone();
        self.wire_emitter("plugin", move |emitter| {
            service.set_event_emitter(emitter);
        });
    }
}
