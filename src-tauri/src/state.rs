use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use athena_core::plan_manager::ExecutionPlan;
use athena_core::tool_executor::ToolEventSender;
use tauri::{AppHandle, Emitter, Manager};

/// Watch-channel sender mirroring `AppState::mcp_runtime_stop`. The MCP
/// runtime thread parks on the receiver (`stop_rx.changed()`) instead of
/// busy-polling the AtomicBool at 100 ms (F12). Shutdown paths send `true`
/// through this AND store the AtomicBool (which `McpServer` itself reads).
pub(crate) static MCP_RUNTIME_STOP_TX: std::sync::Mutex<Option<tokio::sync::watch::Sender<bool>>> =
    std::sync::Mutex::new(None);

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
    /// Current streamed assistant request used to scope tool-generated UI events.
    request_context: Arc<parking_lot::Mutex<Option<(String, String)>>>,
    /// Tool question IDs currently blocked on a response, grouped by stream request.
    request_questions: Arc<parking_lot::Mutex<HashMap<String, Vec<String>>>>,
    /// Requests cancelled before a blocking tool could register its question.
    cancelled_requests: Arc<parking_lot::Mutex<HashSet<String>>>,
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
            request_context: Arc::new(parking_lot::Mutex::new(None)),
            request_questions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            cancelled_requests: Arc::new(parking_lot::Mutex::new(HashSet::new())),
        }
    }

    /// Remove a completed/failed question from both the global answer map and
    /// its stream ownership index. Lock order matches ask_user/cancel_request.
    fn remove_pending_question(&self, question_id: &str, stream_request_id: Option<&String>) {
        if let Some(stream_request_id) = stream_request_id {
            let mut questions = self.request_questions.lock();
            if let Some(question_ids) = questions.get_mut(stream_request_id) {
                question_ids.retain(|id| id != question_id);
                if question_ids.is_empty() {
                    questions.remove(stream_request_id);
                }
            }
        }
        self.pending_questions.lock().remove(question_id);
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
        let agent_key = athena_core::agent_detection::canonical_agent_key(agent_type)
            .or_else(|| {
                athena_core::agent_detection::AGENT_FG_NAMES
                    .iter()
                    .copied()
                    .find(|key| {
                        athena_core::agent_detection::command_contains_agent(agent_cmd, key)
                    })
            })
            .unwrap_or("agent");
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
                    // The frontend's `pty_spawn_agent` path appends a newline
                    // before invoking, but this tool-launched path did not — so
                    // `launch_builtin_agent`/`dispatch_plan_step` commands were
                    // only echoed, never executed. Terminate the command so the
                    // shell actually runs it. Skip empty commands (a "shell"
                    // agent with no task prompt is just an interactive shell).
                    if !agent_cmd.is_empty() {
                        let command = format!("{}\n", agent_cmd);
                        if let Err(e) = session.write(command.as_bytes()).await {
                            log::error!("Failed to write agent command to PTY {}: {}", id, e);
                            return;
                        }
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
        // Fast path: agents launched by Athena are registered immediately.
        {
            let guard = self.active_sessions.lock();
            if guard.contains(pane_id) {
                return true;
            }
        }

        // User-created panes are not added to `active_sessions`, but their
        // read loop registers output in this shared buffer. Treat that as a
        // useful identity signal while the session-manager lock is briefly
        // contended; otherwise a valid pane can be reported as missing.
        // Only trust a *live* buffer: once a PTY exits, `on_pty_exit` marks
        // the buffer dead, so buffered history from a gone session must not
        // make the pane look alive (the executor would otherwise keep writing
        // agent commands into a dead pane).
        let has_live_buffer = self
            .output_buffer
            .get_pane_buffer_info(pane_id)
            .is_some_and(|info| !info.dead);

        // `has_session` is called from the executor's `spawn_blocking` bridge,
        // so a synchronous read is appropriate here. Retry instead of turning
        // transient SessionManager contention into a false "No active PTY"
        // result. The live-buffer fallback covers panes that have already
        // produced output while their session lock is busy.
        let session_manager = Arc::clone(&self.session_manager);
        for _ in 0..4 {
            if let Ok(sm) = session_manager.try_lock() {
                return sm.has_session_sync(pane_id);
            }
            std::thread::yield_now();
        }

        has_live_buffer
    }

    fn ask_user(&self, request_id: &str, question: &str, options: &[serde_json::Value]) -> String {
        let (tx, rx) = std::sync::mpsc::channel::<String>();

        // Register the sender and its stream ownership while holding the same
        // lock order used by cancel_request. This closes the cancellation
        // window between inserting pending_questions and linking the question
        // ID to its stream request.
        let context = {
            let context_guard = self.request_context.lock();
            let context = context_guard.clone();
            let cancelled_requests = self.cancelled_requests.lock();
            if context.as_ref().is_some_and(|(stream_request_id, _)| {
                cancelled_requests.contains(stream_request_id)
            }) {
                return "error: request cancelled".to_string();
            }
            drop(cancelled_requests);
            let mut request_questions = self.request_questions.lock();
            let mut pending_questions = self.pending_questions.lock();
            pending_questions.insert(request_id.to_string(), tx);
            if let Some((stream_request_id, _)) = context.as_ref() {
                request_questions
                    .entry(stream_request_id.clone())
                    .or_default()
                    .push(request_id.to_string());
            }
            context
        };

        let handle_guard = self.app_handle.lock();
        if let Some(ref handle) = *handle_guard {
            let context = self.request_context.lock().clone();
            let payload = serde_json::json!({
                "requestId": request_id,
                "request_id": context.as_ref().map(|(id, _)| id),
                "sessionId": context.as_ref().map(|(_, session)| session),
                "question": question,
                "options": options,
            });
            let emit_result = match serde_json::to_string(&payload) {
                Ok(payload_str) => handle
                    .emit("athena:askUser", payload_str)
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            if let Err(e) = emit_result {
                log::warn!("failed to emit athena:askUser event: {}", e);
                self.remove_pending_question(request_id, context.as_ref().map(|(id, _)| id));
                return "error: unable to show user question".to_string();
            }
        } else {
            log::error!("ask_user called but app_handle is not set");
            self.remove_pending_question(request_id, context.as_ref().map(|(id, _)| id));
            return "error: app_handle not available".to_string();
        }
        drop(handle_guard);

        match rx.recv_timeout(std::time::Duration::from_secs(300)) {
            Ok(answer) => {
                self.remove_pending_question(request_id, context.as_ref().map(|(id, _)| id));
                answer
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                self.remove_pending_question(request_id, context.as_ref().map(|(id, _)| id));
                log::warn!("ask_user timed out for request_id: {}", request_id);
                "error: user response timed out".to_string()
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                self.remove_pending_question(request_id, context.as_ref().map(|(id, _)| id));
                log::warn!(
                    "receiver channel closed for request_id: {} -- question was: {}",
                    request_id,
                    question
                );
                "error: user response channel closed".to_string()
            }
        }
    }

    fn set_request_context(&self, request_id: &str, session_id: &str) {
        *self.request_context.lock() = Some((request_id.to_string(), session_id.to_string()));
    }

    fn clear_request_context(&self) {
        let Some((request_id, _)) = self.request_context.lock().take() else {
            return;
        };
        let question_ids = self
            .request_questions
            .lock()
            .remove(&request_id)
            .unwrap_or_default();
        for question_id in question_ids {
            if let Some(tx) = self.pending_questions.lock().remove(&question_id) {
                let _ = tx.send("error: request finished".to_string());
            }
        }
    }

    fn request_cancelled(&self, request_id: &str) -> bool {
        self.cancelled_requests.lock().contains(request_id)
    }

    fn finish_request(&self, request_id: &str) {
        self.cancelled_requests.lock().remove(request_id);
    }

    fn cancel_request(&self, request_id: &str) -> bool {
        // Hold the context guard through question cleanup. ask_user uses this
        // same lock order, so cancellation cannot miss a just-registered
        // blocking question.
        let context_guard = self.request_context.lock();
        if context_guard
            .as_ref()
            .is_none_or(|(active, _)| active != request_id)
        {
            // The stream may be cancelled while it is loading its session or
            // provider configuration, before the executor context is set.
            // Remember that cancellation so a later ask_user cannot block.
            self.cancelled_requests
                .lock()
                .insert(request_id.to_string());
            return false;
        }

        self.cancelled_requests
            .lock()
            .insert(request_id.to_string());
        let question_ids = self
            .request_questions
            .lock()
            .remove(request_id)
            .unwrap_or_default();
        // Wake every synchronous ask_user receiver belonging to this stream.
        // Removing the sender from the shared map also makes late UI answers
        // harmless: athena_user_answer will return false.
        for question_id in question_ids {
            if let Some(tx) = self.pending_questions.lock().remove(&question_id) {
                let _ = tx.send("error: request cancelled".to_string());
            }
        }
        // Keep the context until the stream's final cleanup. A tool may begin
        // after cancellation has been signalled; retaining the context lets
        // ask_user observe the cancellation tombstone instead of blocking.
        drop(context_guard);
        true
    }

    fn plan_update(&self, plan: &ExecutionPlan) {
        let handle_guard = self.app_handle.lock();
        if let Some(ref handle) = *handle_guard {
            let payload = serde_json::json!({
                "planId": plan.id,
                "requestId": self.request_context.lock().as_ref().map(|(id, _)| id),
                "sessionId": self.request_context.lock().as_ref().map(|(_, session)| session),
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
                    "agentType": s.agent_type,
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
            let context = self.request_context.lock().clone();
            let payload = serde_json::json!({
                "planId": plan_id,
                "requestId": context.as_ref().map(|(id, _)| id),
                "sessionId": context.as_ref().map(|(_, session)| session),
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
    pub mcp_runtime_started: Arc<AtomicBool>,
    /// Signals the dedicated MCP runtime thread to stop during app shutdown.
    pub mcp_runtime_stop: Arc<AtomicBool>,
    /// Kanban backend — same `Arc<KanbanBackend>` is shared with the
    /// ToolExecutor's internally-held instance (the store clone keeps storage
    /// consistent via the same backing KeyValueStore).
    pub kanban_backend: Arc<athena_core::kanban::KanbanBackend>,

    /// Panes the desktop has explicitly shared with the Mobile Mirror relay.
    /// In-memory and empty by default (per-pane sharing is opt-in); reset on
    /// app exit. The relay's terminal read/write surface is gated on this set
    /// (∪ panes a phone spawned itself). Desktop-only commands mutate it.
    pub relay_shared_panes: parking_lot::Mutex<HashSet<String>>,

    /// Pending Mobile Mirror pairing requests awaiting desktop approval.
    /// Keyed by request id; the value is a oneshot sender the desktop's
    /// `relay_pairing_respond` command uses to approve (`true`) or deny
    /// (`false`) the in-flight WebSocket upgrade. Tokio oneshots are
    /// runtime-agnostic, so the sender lives here (Tauri command runtime)
    /// while the receiver is awaited on the relay's dedicated runtime.
    pub relay_pairing_requests:
        Arc<parking_lot::Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>>,

    /// Last `relay_request_pane_share` timestamp per pane (ms since epoch),
    /// used to rate-limit share prompts so a paired phone can't spam the
    /// desktop operator with approval dialogs.
    pub relay_pane_share_last_request: Arc<parking_lot::Mutex<HashMap<String, u64>>>,

    /// Per-pane raw byte replay buffer (last ~64 KB per pane, ~4 MB across
    /// all panes) for the Mobile Mirror relay. Appended by the PTY read loop
    /// on every flush; dropped on `pty_kill` and evicted least-recently-first
    /// past the aggregate cap. A reconnecting phone replays these exact bytes
    /// to restore VT screen state (cursor position, colors, a partial
    /// in-flight line) instead of relying only on ANSI-stripped text history.
    pub relay_raw_replay: Arc<parking_lot::Mutex<crate::commands::RelayReplayStore>>,
    /// Live relay subscriptions to `pty:raw:<pane>` events. The PTY read
    /// loop emits the base64 event only while this count is positive —
    /// `Emitter::emit` evals webview JS even with no listeners, so an
    /// unconditional emission wasted work on every 8 ms flush once the
    /// desktop frontend moved to raw channel delivery.
    pub relay_raw_subscribers: Arc<parking_lot::Mutex<HashMap<String, usize>>>,

    /// Active microphone capture for Athena voice input (desktop). `Some`
    /// while recording; `voice_record_stop` takes it (dropping the cpal stream
    /// ends capture) and transcribes the clip on-device. Empty by default.
    pub voice_recording: parking_lot::Mutex<Option<crate::commands::voice::VoiceRecording>>,
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

        let tool_executor = Arc::new(parking_lot::RwLock::new(
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
            relay_shared_panes: parking_lot::Mutex::new(HashSet::new()),
            relay_pairing_requests: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            relay_pane_share_last_request: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            relay_raw_replay: Arc::new(parking_lot::Mutex::new(
                crate::commands::RelayReplayStore::default(),
            )),
            relay_raw_subscribers: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            voice_recording: parking_lot::Mutex::new(None),
            rate_limiter: crate::commands::caps::global_rate_limiter(),
            // Kept for command/API compatibility; desktop startup now owns
            // only the TCP transport and never consumes stdin/stdout.
            mcp_runtime_started: Arc::new(AtomicBool::new(false)),
            mcp_runtime_stop,
        }
    }

    /// Store the Tauri `AppHandle` so events can be emitted.
    /// Called once after the Tauri app is built.
    pub fn set_app_handle(&self, handle: AppHandle) {
        {
            let mut guard = self.app_handle.lock();
            *guard = Some(handle.clone());
        } // Drop the lock before calling wire methods that also acquire it

        // Stream events are request-scoped and use one stable channel. The
        // payload is already typed in athena-core; serialize exactly once at
        // the IPC boundary so the frontend can ignore stale request IDs.
        let stream_handle = handle.clone();
        let stream_store = Arc::clone(&self.store);
        self.orchestrator.set_stream_emitter(Some(Arc::new(
            move |event| {
                // Stale-flag recovery: if the provider rejected the request
                // (401 / model unavailable), drop the persisted "set" flag so
                // the next store_get re-checks the keyring instead of trusting
                // a keychain entry that was deleted out-of-band. Runs before
                // serialization so the frontend's very next status read sees it.
                if let athena_core::types::AthenaStreamEvent::Error {
                    message,
                    model_unavailable,
                    ..
                } = &event
                {
                    crate::commands::store::clear_api_key_flag_on_provider_error(
                        &stream_store,
                        *model_unavailable,
                        message,
                    );
                }
                match serde_json::to_string(&event) {
                    Ok(payload) => {
                        if let Err(error) = stream_handle.emit("athena:stream", payload) {
                            log::warn!("failed to emit athena stream event: {error}");
                        }
                    }
                    Err(error) => log::warn!("failed to serialize athena stream event: {error}"),
                }
            },
        )));

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

    /// Start the boot-time MCP TCP transport on a dedicated runtime.
    ///
    /// Stdio is intentionally not started from the desktop app: stdin/stdout
    /// belong to the launched application process and are not a trusted
    /// child-process boundary. Callers that explicitly launch a standalone MCP
    /// subprocess may use `McpServer::init_stdio()` instead.
    ///
    /// Startup is retried while the app is alive if runtime creation or the
    /// canonical TCP bind fails. Keeping the retry loop here makes resetting a
    /// one-time guard unnecessary: a failed first attempt cannot permanently
    /// disable MCP for the rest of the process.
    fn start_mcp_runtime(&self) {
        if self.mcp_runtime_started.compare_exchange(
            false,
            true,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) != Ok(false)
        {
            return;
        }

        let mcp_server = Arc::clone(&self.mcp_server);
        let mcp_runtime_stop = Arc::clone(&self.mcp_runtime_stop);
        let mcp_runtime_started = Arc::clone(&self.mcp_runtime_started);
        std::thread::spawn(move || {
            let rt = loop {
                if mcp_runtime_stop.load(Ordering::Relaxed) {
                    mcp_runtime_started.store(false, Ordering::SeqCst);
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
                // F12: shutdown is a `watch` channel, not a 100 ms busy-poll —
                // the parked await wakes the instant main.rs sends through
                // `MCP_RUNTIME_STOP_TX` (main.rs also stores the AtomicBool that
                // `McpServer` itself reads).
                let (stop_tx, mut stop_rx) = tokio::sync::watch::channel(false);
                if let Ok(mut guard) = MCP_RUNTIME_STOP_TX.lock() {
                    *guard = Some(stop_tx);
                }

                // Retry until init succeeds or shutdown fires: a transient port
                // conflict must not permanently disable MCP (doc contract above).
                loop {
                    if mcp_runtime_stop.load(Ordering::Relaxed) {
                        break;
                    }

                    // `init` performs the blocking `std::net::TcpListener::bind`
                    // + `local_addr`. It needs `&mut self`, so it runs under the
                    // async mutex; the bind is local and fast (no network I/O),
                    // so the hold is short. Kept inside per the API shape — do
                    // not grow this critical section.
                    let started = {
                        let mut server = mcp_server.lock().await;
                        match server.init(4545) {
                            Ok(()) => true,
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

                        // `McpServer::init` spawns its accept task on this
                        // runtime. Keep it alive until app shutdown; dropping
                        // this future would abort the TCP server.
                        let _ = stop_rx.changed().await;
                        break;
                    }

                    // Bind failed: retry after a delay, or exit immediately on
                    // shutdown.
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_millis(500)) => {}
                        _ = stop_rx.changed() => { break; }
                    }
                }
                mcp_runtime_started.store(false, Ordering::SeqCst);
            }); // rt.block_on
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
        service.set_event_emitter(Box::new(move |channel: &str, data: &serde_json::Value| {
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
                    | "notifications:resolved"
                    | "notifications:cleared"
            ) {
                let unread = service_for_closure.get_unread_count();
                if let Some(window) = handle.get_webview_window("main") {
                    if let Err(e) = window.set_badge_count(Some(unread as i64)) {
                        log::warn!("failed to set dock badge: {e}");
                    }
                }
            }
        }));
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
        service.set_event_emitter(Box::new(move |channel: &str, data: &serde_json::Value| {
            let mut data = data.clone();
            if channel.starts_with("agents:") && data.get("paneId").is_none() {
                if let Some(agent_id) = data.get("agentId").and_then(|v| v.as_str()) {
                    let pane_id = plugin_manager
                        .get_session_by_agent_id(agent_id)
                        .and_then(|s| s.pane_id)
                        .or_else(|| agent_id.starts_with("pane").then(|| agent_id.to_string()));
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

            // Plugin status transitions are lifecycle signals, not merely
            // badge updates. Route terminal states through the shared service
            // so a plugin-connected agent cannot become "Done" silently.
            if channel == "agents:statusUpdate" {
                let status = data
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("idle");
                let request_id = data
                    .get("requestId")
                    .or_else(|| data.get("request_id"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                // An explicit input request below is the canonical actionable
                // event. Do not also create a second NeedsInput record when a
                // plugin sends the accompanying waiting status with the same
                // request id.
                let notification_type = match status {
                    "waiting_for_input" | "waiting_input" if request_id.is_none() => {
                        Some(athena_core::notification::NotificationType::NeedsInput)
                    }
                    "completed" | "done" | "complete" => {
                        Some(athena_core::notification::NotificationType::TaskComplete)
                    }
                    "error" | "failed" => {
                        Some(athena_core::notification::NotificationType::TaskError)
                    }
                    "cancelled" | "canceled" => {
                        Some(athena_core::notification::NotificationType::Warning)
                    }
                    _ => None,
                };
                if let Some(notification_type) = notification_type {
                    let pane_id = data
                        .get("paneId")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let run_id = data
                        .get("runId")
                        .or_else(|| data.get("sessionId"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let event_timestamp = data
                        .get("timestamp")
                        .and_then(|v| v.as_u64())
                        .unwrap_or_else(crate::commands::now_ms);
                    let event_key = data
                        .get("eventId")
                        .or_else(|| data.get("eventKey"))
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                        .or_else(|| {
                            Some(format!(
                                "agent-status:{}:{}:{}:{}",
                                pane_id.as_deref().unwrap_or("unknown"),
                                run_id.as_deref().unwrap_or("unknown"),
                                status,
                                event_timestamp
                            ))
                        });
                    let title = match notification_type {
                        athena_core::notification::NotificationType::NeedsInput => {
                            "Agent needs input"
                        }
                        athena_core::notification::NotificationType::TaskComplete => {
                            "Agent finished"
                        }
                        athena_core::notification::NotificationType::TaskError => "Agent error",
                        _ => "Agent update",
                    };
                    // Compute before `push_notification` moves `notification_type`.
                    let requires_action = matches!(
                        notification_type,
                        athena_core::notification::NotificationType::NeedsInput
                    );
                    notification_service.push_notification(
                        athena_core::notification::NotificationEvent {
                            r#type: notification_type,
                            title: title.to_string(),
                            message: data
                                .get("message")
                                .or_else(|| data.get("prompt"))
                                .and_then(|v| v.as_str())
                                .unwrap_or(status)
                                .to_string(),
                            source: "agent".to_string(),
                            agent_id: data
                                .get("agentId")
                                .and_then(|v| v.as_str())
                                .map(str::to_string),
                            data: Some(data.clone()),
                            timestamp: data
                                .get("timestamp")
                                .and_then(|v| v.as_u64())
                                .unwrap_or_else(crate::commands::now_ms),
                            metadata: None,
                            actions: None,
                            request_id: request_id.clone(),
                            event_key,
                            run_id,
                            pane_id,
                            requires_action,
                        },
                    );
                }
            }

            // An explicit input request carries the stable request identity
            // needed to resolve the correct blocked agent later.
            if channel == "agents:inputRequested" {
                let pane_id = data
                    .get("paneId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                let request_id = data
                    .get("requestId")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                notification_service.push_notification(
                    athena_core::notification::NotificationEvent {
                        r#type: athena_core::notification::NotificationType::NeedsInput,
                        title: "Agent input requested".to_string(),
                        message: data
                            .get("message")
                            .or_else(|| data.get("prompt"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("Agent is requesting input")
                            .to_string(),
                        source: "agent".to_string(),
                        agent_id: data
                            .get("agentId")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        data: Some(data.clone()),
                        timestamp: data
                            .get("timestamp")
                            .and_then(|v| v.as_u64())
                            .unwrap_or_else(crate::commands::now_ms),
                        metadata: None,
                        actions: None,
                        request_id: request_id.clone(),
                        event_key: request_id.as_ref().map(|id| format!("agent-input:{id}")),
                        run_id: data
                            .get("runId")
                            .or_else(|| data.get("sessionId"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        pane_id,
                        requires_action: true,
                    },
                );
            }

            // Plugin notifications are high-fidelity equivalents of the
            // passive tracker notifications. Route them through the
            // shared service so the existing frontend, macOS sound, and
            // dock-badge paths all behave consistently.
            if channel == "agents:notification" {
                let level = data.get("level").and_then(|v| v.as_str()).unwrap_or("info");
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
                // Compute before `push_notification` moves `notification_type`.
                let requires_action = matches!(
                    notification_type,
                    athena_core::notification::NotificationType::NeedsInput
                );
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
                        request_id: data
                            .get("requestId")
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        event_key: data
                            .get("eventId")
                            .or_else(|| data.get("eventKey"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        run_id: data
                            .get("runId")
                            .or_else(|| data.get("sessionId"))
                            .and_then(|v| v.as_str())
                            .map(str::to_string),
                        pane_id: pane_id.clone(),
                        requires_action,
                    },
                );
                return;
            }

            if let Ok(data_str) = serde_json::to_string(&data) {
                if let Err(e) = handle.emit(channel, data_str) {
                    log::warn!("failed to emit agent_comms event {channel}: {e}");
                }
            }
        }));
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
                // perf#4: the config is re-read only when the KV store's
                // revision counter changes; a steady-state tick is now a
                // single relaxed atomic load instead of a Value clone plus
                // JSON parse.
                let mut cached_rev: Option<u64> = None;
                loop {
                    interval.tick().await;

                    // Apply the persisted per-type notification config (the
                    // frontend writes it via the existing `store_set` IPC —
                    // no new command surface needed). Best-effort: missing /
                    // malformed JSON keeps the previous config.
                    let rev = store.revision();
                    if cached_rev != Some(rev) {
                        if let Ok(Some(json)) = store.get::<String>("agent_notify_config") {
                            if let Ok(cfg) = serde_json::from_str::<
                                athena_core::agent_activity::AgentNotifyConfig,
                            >(&json)
                            {
                                agent_activity.set_notify_config(cfg);
                            }
                        }
                        cached_rev = Some(rev);
                    }

                    // Belt-and-braces leak guard: evict activity entries
                    // whose pane vanished without a teardown path (F5).
                    // 24 h: a pane quiet for a day is dead; 30 s would evict
                    // live quiet panes every tick.
                    agent_activity
                        .prune_stale_panes(crate::commands::now_ms(), 24 * 60 * 60 * 1000);
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
                        let history = fg_opt.as_deref().and_then(|agent_key| {
                            athena_core::agent_detection::scrape_agent_history_for_cwd(
                                agent_key,
                                std::path::Path::new(&session.cwd),
                                session.tty_path.as_deref(),
                            )
                        });
                        let tail = if fg_opt.is_some() {
                            let lines = output_buffer.get_output_tail(sid, 40);
                            Some(
                                lines
                                    .iter()
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
