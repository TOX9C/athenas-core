use std::sync::OnceLock;
use wasm_bindgen::prelude::*;

/// Result type for Tauri command invocations.
pub type TauriResult<T> = Result<T, JsValue>;

/// Invoke a Tauri command.
/// Maps to `window.__TAURI__.core.invoke(command, args)` in the browser context.
pub async fn invoke<T>(command: &str, args: &str) -> TauriResult<T>
where
    T: JsValueCast,
{
    // Perf accounting happens once in `invoke_js_value`; recording here too
    // would double-count every JSON invoke.
    let args_value: JsValue = serde_json::from_str(args)
        .map(|v: serde_json::Value| {
            js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::UNDEFINED)
        })
        .unwrap_or(JsValue::UNDEFINED);

    invoke_js_value(command, args_value).await
}

/// Invoke with live-built JS args. Required for arguments that cannot cross
/// JSON (e.g. a `Channel` instance for raw binary PTY delivery).
pub async fn invoke_js_value<T>(command: &str, args_value: JsValue) -> TauriResult<T>
where
    T: JsValueCast,
{
    crate::utils::perf_metrics::record_ipc(command);
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__"))
        .map_err(|e| JsValue::from(format!("Reflect get error: {:?}", e)))?;
    let core = js_sys::Reflect::get(&tauri, &JsValue::from_str("core"))
        .map_err(|e| JsValue::from(format!("Reflect get __TAURI__.core error: {:?}", e)))?;
    let invoke_fn = js_sys::Reflect::get(&core, &JsValue::from_str("invoke"))
        .map_err(|e| JsValue::from(format!("Reflect invoke error: {:?}", e)))?;
    let invoke_fn = invoke_fn
        .dyn_into::<js_sys::Function>()
        .map_err(|e| JsValue::from(format!("__TAURI__.core.invoke not found: {:?}", e)))?;

    let promise = invoke_fn
        .call2(&core, &JsValue::from_str(command), &args_value)
        .map_err(|e| JsValue::from(format!("Invoke error: {:?}", e)))?;

    let result = wasm_bindgen_futures::JsFuture::from(js_sys::Promise::from(promise))
        .await
        .map_err(|e| JsValue::from(format!("Promise error: {:?}", e)))?;

    T::from_js_value(result)
}

/// Trait to cast JsValue to a typed result.
pub trait JsValueCast: Sized {
    fn from_js_value(value: JsValue) -> TauriResult<Self>;
    fn to_js_value(&self) -> JsValue;
}

impl JsValueCast for String {
    fn from_js_value(value: JsValue) -> TauriResult<Self> {
        value
            .as_string()
            .ok_or_else(|| JsValue::from_str("Not a string"))
    }
    fn to_js_value(&self) -> JsValue {
        JsValue::from_str(self)
    }
}

impl JsValueCast for JsValue {
    fn from_js_value(value: JsValue) -> TauriResult<Self> {
        Ok(value)
    }
    fn to_js_value(&self) -> JsValue {
        self.clone()
    }
}

impl JsValueCast for bool {
    fn from_js_value(value: JsValue) -> TauriResult<Self> {
        Ok(value.as_bool().unwrap_or(false))
    }
    fn to_js_value(&self) -> JsValue {
        JsValue::from_bool(*self)
    }
}

impl JsValueCast for () {
    fn from_js_value(_value: JsValue) -> TauriResult<Self> {
        Ok(())
    }
    fn to_js_value(&self) -> JsValue {
        JsValue::UNDEFINED
    }
}

impl JsValueCast for Option<String> {
    fn from_js_value(value: JsValue) -> TauriResult<Self> {
        if value.is_null() || value.is_undefined() {
            Ok(None)
        } else {
            value
                .as_string()
                .map(Some)
                .ok_or_else(|| JsValue::from_str("Not a string or null"))
        }
    }
    fn to_js_value(&self) -> JsValue {
        match self {
            Some(value) => JsValue::from_str(value),
            None => JsValue::NULL,
        }
    }
}

// ---------------------------------------------------------------------------
// Typed Tauri command wrappers
// ---------------------------------------------------------------------------

/// Window operations
pub async fn window_minimize() -> TauriResult<JsValue> {
    invoke("window_minimize", "{}").await
}

pub async fn window_maximize() -> TauriResult<JsValue> {
    invoke("window_maximize", "{}").await
}

pub async fn window_close() -> TauriResult<JsValue> {
    invoke("window_close", "{}").await
}

pub async fn window_is_maximized() -> TauriResult<bool> {
    invoke("window_is_maximized", "{}").await
}

pub async fn window_platform() -> TauriResult<String> {
    invoke("window_platform", "{}").await
}

/// File system operations
pub async fn fs_read_file(path: &str) -> TauriResult<String> {
    invoke(
        "fs_read_file",
        &serde_json::json!({ "path": path }).to_string(),
    )
    .await
}

pub async fn fs_list_dir(path: &str) -> TauriResult<String> {
    invoke(
        "fs_list_dir",
        &serde_json::json!({ "path": path }).to_string(),
    )
    .await
}

pub async fn fs_write_file(path: &str, content: &str) -> TauriResult<String> {
    invoke(
        "fs_write_file",
        &serde_json::json!({ "path": path, "content": content }).to_string(),
    )
    .await
}

/// Open a native file/folder dialog via custom Tauri command.
/// Returns the selected path on single selection, empty string on cancel.
pub async fn fs_show_open_dialog(
    title: Option<&str>,
    directory: bool,
    multiple: bool,
) -> TauriResult<String> {
    let mut args = serde_json::json!({
        "multiple": multiple,
        "directory": directory,
    });
    if let Some(t) = title {
        args["title"] = serde_json::Value::String(t.to_string());
    }

    invoke("fs_show_open_dialog", &args.to_string()).await
}

/// Open a native image file dialog via custom Tauri command.
pub async fn fs_show_image_dialog() -> TauriResult<String> {
    invoke("fs_show_image_dialog", "{}").await
}

/// Store operations
pub async fn store_get(key: &str) -> TauriResult<String> {
    invoke("store_get", &serde_json::json!({ "key": key }).to_string()).await
}

pub async fn store_set(key: &str, value: &str) -> TauriResult<()> {
    let result = invoke(
        "store_set",
        &serde_json::json!({ "key": key, "value": value }).to_string(),
    )
    .await;
    if let Err(error) = &result {
        web_sys::console::error_1(
            &format!("[tauri_bridge] store_set FAILED key={key:?}: {error:?}").into(),
        );
    }
    result
}

/// Export a redacted diagnostic bundle assembled by the native backend.
/// `frontend_logs` and `frontend_metrics` are supplied by the bounded browser
/// diagnostics ring in `frontend/index.html`.
pub async fn diagnostics_export(
    frontend_logs: &str,
    frontend_metrics: &str,
) -> TauriResult<String> {
    invoke(
        "diagnostics_export",
        &serde_json::json!({
            "frontendLogs": frontend_logs,
            "frontendMetrics": frontend_metrics,
        })
        .to_string(),
    )
    .await
}

/// Delete a key from the persistent key-value store.
pub async fn store_delete(key: &str) -> TauriResult<()> {
    invoke(
        "store_delete",
        &serde_json::json!({ "key": key }).to_string(),
    )
    .await
}

/// Session operations
pub async fn session_create(title: Option<&str>) -> TauriResult<String> {
    invoke(
        "session_create",
        &serde_json::json!({ "title": title }).to_string(),
    )
    .await
}

pub async fn session_list() -> TauriResult<String> {
    invoke("session_list", "{}").await
}

pub async fn session_get(id: &str) -> TauriResult<String> {
    invoke("session_get", &serde_json::json!({ "id": id }).to_string()).await
}

pub async fn session_update(
    id: &str,
    title: Option<&str>,
    messages: Option<&str>,
) -> TauriResult<String> {
    invoke(
        "session_update",
        &serde_json::json!({
            "id": id,
            "title": title,
            "messages": messages
        })
        .to_string(),
    )
    .await
}

pub async fn session_delete(id: &str) -> TauriResult<String> {
    invoke(
        "session_delete",
        &serde_json::json!({ "id": id }).to_string(),
    )
    .await
}

/// Output buffer operations
pub async fn output_buffer_append(
    pane_id: &str,
    data: &str,
    agent_type: Option<&str>,
) -> TauriResult<JsValue> {
    invoke(
        "output_buffer_append",
        // Tauri 2 converts command args to camelCase on the wire (see
        // Tauri command parameter naming — `paneId`/`agentType`, not snake.
        &serde_json::json!({ "paneId": pane_id, "data": data, "agentType": agent_type })
            .to_string(),
    )
    .await
}

pub async fn output_buffer_get(
    pane_id: &str,
    limit: Option<usize>,
    offset: Option<usize>,
) -> TauriResult<String> {
    invoke(
        "output_buffer_get",
        &serde_json::json!({ "paneId": pane_id, "limit": limit, "offset": offset }).to_string(),
    )
    .await
}

pub async fn output_buffer_list() -> TauriResult<String> {
    invoke("output_buffer_list", "{}").await
}

fn output_buffer_clear_args(pane_id: &str) -> String {
    // The relay dispatcher consumes this command's raw JSON and expects the
    // Rust parameter name; unlike Tauri's desktop invoke path it does not
    // perform camelCase conversion.
    serde_json::json!({ "pane_id": pane_id }).to_string()
}

pub async fn output_buffer_clear(pane_id: &str) -> TauriResult<String> {
    invoke("output_buffer_clear", &output_buffer_clear_args(pane_id)).await
}

#[cfg(test)]
mod contract_tests {
    #[test]
    fn output_buffer_clear_uses_the_relay_pane_id_wire_key() {
        let args: serde_json::Value =
            serde_json::from_str(&super::output_buffer_clear_args("pane-7")).unwrap();

        assert_eq!(
            args.get("pane_id").and_then(|value| value.as_str()),
            Some("pane-7")
        );
        assert!(args.get("paneId").is_none());
    }
}

/// Notification operations
pub async fn notification_push(
    title: &str,
    message: &str,
    level: Option<&str>,
) -> TauriResult<String> {
    invoke(
        "notification_push",
        &serde_json::json!({ "title": title, "message": message, "level": level }).to_string(),
    )
    .await
}

pub async fn notification_history(limit: Option<usize>) -> TauriResult<String> {
    invoke(
        "notification_history",
        &serde_json::json!({ "limit": limit }).to_string(),
    )
    .await
}

pub async fn notification_count() -> TauriResult<String> {
    invoke("notification_count", "{}").await
}

pub async fn notification_mark_read(id: &str) -> TauriResult<()> {
    invoke(
        "notification_mark_read",
        &serde_json::json!({ "notificationId": id }).to_string(),
    )
    .await
}

pub async fn notification_mark_all_read() -> TauriResult<()> {
    invoke("notification_mark_all_read", "{}").await
}

pub async fn notification_clear_all() -> TauriResult<()> {
    invoke("notification_clear_all", "{}").await
}

pub async fn notification_dismiss(id: &str) -> TauriResult<()> {
    invoke(
        "notification_dismiss",
        &serde_json::json!({ "notificationId": id }).to_string(),
    )
    .await
}

pub async fn notification_resolve(id: &str) -> TauriResult<()> {
    invoke(
        "notification_resolve",
        &serde_json::json!({ "notificationId": id }).to_string(),
    )
    .await
}

/// Install the agent-notification emitter for one agent (or `"all"`).
/// Writes the OSC 6337 emitter script and wires each agent's native hooks
/// (Claude Code `Stop`/`PermissionRequest`/`Notification`, Codex lifecycle
/// hooks) non-destructively. Returns a human-readable summary of the files
/// written, or an error string.
pub async fn agent_notify_install(agent: &str) -> TauriResult<String> {
    invoke(
        "agent_notify_install",
        &serde_json::json!({ "agent": agent }).to_string(),
    )
    .await
}

/// Plan operations
pub async fn plan_create(goal: &str, reasoning: &str, steps: &str) -> TauriResult<String> {
    invoke(
        "plan_create",
        &serde_json::json!({ "goal": goal, "reasoning": reasoning, "steps": steps }).to_string(),
    )
    .await
}

pub async fn plan_get() -> TauriResult<String> {
    invoke("plan_get", "{}").await
}

pub async fn plan_update_step(
    step_id: &str,
    status: &str,
    pane_id: Option<&str>,
) -> TauriResult<String> {
    invoke(
        "plan_update_step",
        &serde_json::json!({ "stepId": step_id, "status": status, "paneId": pane_id }).to_string(),
    )
    .await
}

/// Agent comms operations
pub async fn agent_comms_token() -> TauriResult<String> {
    invoke("agent_comms_token", "{}").await
}

pub async fn agent_comms_sessions() -> TauriResult<String> {
    invoke("agent_comms_sessions", "{}").await
}

pub async fn agent_comms_send(agent_id: &str, method: &str, params: &str) -> TauriResult<String> {
    invoke(
        "agent_comms_send",
        &serde_json::json!({ "agentId": agent_id, "method": method, "params": params }).to_string(),
    )
    .await
}

/// Respond to an agent comms input request.
pub async fn agent_respond_input(request_id: &str, response: &str) -> TauriResult<JsValue> {
    invoke(
        "agent_respond_input",
        &serde_json::json!({ "requestId": request_id, "response": response }).to_string(),
    )
    .await
}

/// Search operation
pub async fn search_code(pattern: &str, path: &str) -> TauriResult<String> {
    invoke(
        "search_code",
        &serde_json::json!({ "pattern": pattern, "path": path }).to_string(),
    )
    .await
}

/// MCP server operations
pub async fn mcp_init(port: u16) -> TauriResult<String> {
    invoke("mcp_init", &serde_json::json!({ "port": port }).to_string()).await
}

pub async fn mcp_shutdown() -> TauriResult<String> {
    invoke("mcp_shutdown", "{}").await
}

pub async fn mcp_handle_request(request: &str) -> TauriResult<String> {
    invoke(
        "mcp_handle_request",
        &serde_json::json!({ "request": request }).to_string(),
    )
    .await
}

pub async fn mcp_broadcast(method: &str, params: &str) -> TauriResult<String> {
    invoke(
        "mcp_broadcast",
        &serde_json::json!({ "method": method, "params": params }).to_string(),
    )
    .await
}

pub async fn mcp_tools() -> TauriResult<String> {
    invoke("mcp_tools", "{}").await
}

/// Swarm operations
pub async fn swarm_create(dir: &str, swarm_state: &str) -> TauriResult<String> {
    // Tauri v2 expects camelCase JSON keys for command arguments. The backend
    // param is `swarm_state` (snake_case, a required String), so the wire key
    // must be `swarmState` — sending `swarm_state` makes the command fail with
    // "missing required key swarmState" (Tauri command parameter naming
    // vs. tauri-macros' default `rename_all = "camelCase"`).
    invoke(
        "swarm_create",
        &serde_json::json!({ "dir": dir, "swarmState": swarm_state }).to_string(),
    )
    .await
}

pub async fn swarm_read_state(dir: &str) -> TauriResult<String> {
    invoke(
        "swarm_read_state",
        &serde_json::json!({ "dir": dir }).to_string(),
    )
    .await
}

pub async fn swarm_start_watch(dir: &str) -> TauriResult<()> {
    invoke(
        "swarm_start_watch",
        &serde_json::json!({ "dir": dir }).to_string(),
    )
    .await
}

pub async fn swarm_stop_watch(dir: &str) -> TauriResult<()> {
    invoke(
        "swarm_stop_watch",
        &serde_json::json!({ "dir": dir }).to_string(),
    )
    .await
}

pub async fn swarm_update_agent(
    dir: &str,
    agent_id: &str,
    status: Option<&str>,
    last_action: Option<&str>,
    current_task: Option<Option<&str>>,
) -> TauriResult<String> {
    // camelCase wire keys (`agentId`, `lastAction`, `currentTask`) — see
    // swarm_create comment.
    invoke(
        "swarm_update_agent",
        &serde_json::json!({
            "dir": dir,
            "agentId": agent_id,
            "status": status,
            "lastAction": last_action,
            "currentTask": current_task,
        })
        .to_string(),
    )
    .await
}

pub async fn swarm_set_status(dir: &str, status: &str) -> TauriResult<String> {
    invoke(
        "swarm_set_status",
        &serde_json::json!({ "dir": dir, "status": status }).to_string(),
    )
    .await
}

pub async fn swarm_create_task(
    dir: &str,
    title: &str,
    description: &str,
    assigned_agent_id: &str,
) -> TauriResult<String> {
    // camelCase wire key (`assignedAgentId`) — see swarm_create comment.
    invoke(
        "swarm_create_task",
        &serde_json::json!({
            "dir": dir,
            "title": title,
            "description": description,
            "assignedAgentId": assigned_agent_id,
        })
        .to_string(),
    )
    .await
}

pub async fn swarm_update_task(dir: &str, task_id: &str, status: &str) -> TauriResult<String> {
    // camelCase wire key (`taskId`) — see swarm_create comment.
    invoke(
        "swarm_update_task",
        &serde_json::json!({ "dir": dir, "taskId": task_id, "status": status }).to_string(),
    )
    .await
}

pub async fn swarm_send_message(
    dir: &str,
    from: &str,
    to: &str,
    content: &str,
) -> TauriResult<String> {
    invoke(
        "swarm_send_message",
        &serde_json::json!({ "dir": dir, "from": from, "to": to, "content": content }).to_string(),
    )
    .await
}

pub async fn swarm_read_mailbox(dir: &str, agent_id: &str) -> TauriResult<String> {
    // camelCase wire key (`agentId`) — see swarm_create comment.
    invoke(
        "swarm_read_mailbox",
        &serde_json::json!({ "dir": dir, "agentId": agent_id }).to_string(),
    )
    .await
}

/// Shell integration operations
pub async fn shell_integration_parse(data: &str) -> TauriResult<String> {
    invoke(
        "shell_integration_parse",
        &serde_json::json!({ "data": data }).to_string(),
    )
    .await
}

pub async fn shell_integration_script(shell: &str) -> TauriResult<String> {
    invoke(
        "shell_integration_script",
        &serde_json::json!({ "shell": shell }).to_string(),
    )
    .await
}

pub async fn shell_integration_compatible(shell: &str) -> TauriResult<String> {
    invoke(
        "shell_integration_compatible",
        &serde_json::json!({ "shell": shell }).to_string(),
    )
    .await
}

pub async fn shell_integration_strip(data: &str) -> TauriResult<String> {
    invoke(
        "shell_integration_strip",
        &serde_json::json!({ "data": data }).to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// Kanban operations
// ---------------------------------------------------------------------------

/// Get all kanban tasks for the active workspace.
pub async fn kanban_get_tasks() -> TauriResult<String> {
    invoke("kanban_get_tasks", "{}").await
}

/// Create a new kanban task in the active workspace.
///
/// `plan_step_id` optionally back-links the card to a plan step (Kanban ↔ plan
/// deep link) so the card can jump back to the step in the Athena plan.
pub async fn kanban_create_task(
    title: &str,
    description: Option<&str>,
    plan_step_id: Option<&str>,
) -> TauriResult<String> {
    invoke(
        "kanban_create_task",
        &serde_json::json!({
            "title": title,
            "description": description,
            "planStepId": plan_step_id,
        })
        .to_string(),
    )
    .await
}

/// Update an existing kanban task. Only the supplied fields are modified;
/// passing `None` leaves that field untouched on the backend.
pub async fn kanban_update_task(
    task_id: &str,
    title: Option<&str>,
    description: Option<&str>,
    status: Option<&str>,
) -> TauriResult<String> {
    // camelCase wire key (`taskId`) — Tauri v2 expects camelCase args; the
    // old snake_case key made the required `task_id` param fail, so cards
    // could never move between columns.
    invoke(
        "kanban_update_task",
        &serde_json::json!({
            "taskId": task_id,
            "title": title,
            "description": description,
            "status": status
        })
        .to_string(),
    )
    .await
}

/// Delete a kanban task by ID.
pub async fn kanban_delete_task(task_id: &str) -> TauriResult<JsValue> {
    // camelCase wire key (`taskId`) — see kanban_update_task comment.
    invoke(
        "kanban_delete_task",
        &serde_json::json!({ "taskId": task_id }).to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// PTY / Terminal operations
// ---------------------------------------------------------------------------

/// Get the default shell for the current platform.
pub async fn pty_default_shell() -> TauriResult<String> {
    invoke("pty_default_shell", "{}").await
}

static DEFAULT_SHELL_CACHE: OnceLock<String> = OnceLock::new();

/// Get the default shell, cached after the first call. Falls back to
/// `/bin/zsh` on any error so terminal spawns never block waiting for IPC.
pub async fn pty_default_shell_cached() -> String {
    if let Some(s) = DEFAULT_SHELL_CACHE.get() {
        return s.clone();
    }
    let s = pty_default_shell()
        .await
        .unwrap_or_else(|_| "/bin/zsh".to_string());
    let _ = DEFAULT_SHELL_CACHE.set(s.clone());
    s
}

/// Response from `pty_agent_info`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct AgentInfo {
    pub foreground_process: String,
    pub task_title: Option<String>,
    /// Session ID from the agent's history file (used for de-duplication).
    pub session_id: Option<String>,
    /// Unix timestamp (ms) of the last prompt for the session.
    pub timestamp: Option<u64>,
    /// Raw prompt text (available for LLM summarization). Only set when the
    /// feature is enabled so the frontend can call the summarizer.
    pub raw_prompt: Option<String>,
}

/// Get the active foreground process and, if known, the agent's current task title.
pub async fn pty_agent_info(id: &str) -> TauriResult<AgentInfo> {
    let raw: String = invoke(
        "pty_agent_info",
        &serde_json::json!({ "id": id }).to_string(),
    )
    .await?;
    serde_json::from_str(&raw)
        .map_err(|e| js_sys::Error::new(&format!("failed to parse AgentInfo: {}", e)).into())
}

/// Spawn a new PTY session with the given shell and dimensions.
pub async fn pty_spawn(
    id: &str,
    cwd: &str,
    shell: &str,
    cols: u16,
    rows: u16,
    start_paused: bool,
    listener_owner: Option<&str>,
) -> TauriResult<()> {
    invoke(
        "pty_spawn",
        &serde_json::json!({
            "id": id,
            "cwd": cwd,
            "shell": shell,
            "cols": cols,
            "rows": rows,
            "startPaused": start_paused,
            "listenerOwner": listener_owner,
        })
        .to_string(),
    )
    .await
}

/// Spawn a PTY session and execute an agent command after the shell starts.
///
/// This wrapper intentionally mirrors the backend command's full argument
/// list, including the listener lifecycle handshake.
#[allow(clippy::too_many_arguments)]
pub async fn pty_spawn_agent(
    id: &str,
    cwd: &str,
    shell: &str,
    agent_cmd: &str,
    cols: u16,
    rows: u16,
    start_paused: bool,
    listener_owner: Option<&str>,
) -> TauriResult<()> {
    invoke(
        "pty_spawn_agent",
        // Tauri 2 command arguments use camelCase on the wire. The backend
        // parameter is `agent_cmd`, but sending `agent_cmd` here causes the
        // required `agentCmd` argument to be reported missing. OMP is the
        // only built-in path through this command, so the bug is agent-specific
        // while ordinary shell panes continue to spawn successfully.
        &serde_json::json!({
            "id": id,
            "cwd": cwd,
            "shell": shell,
            "agentCmd": agent_cmd,
            "cols": cols,
            "rows": rows,
            "startPaused": start_paused,
            "listenerOwner": listener_owner,
        })
        .to_string(),
    )
    .await
}

/// Stage a data-only dropped image in an app-owned temporary directory and
/// return its path. Used when CleanShot X supplies image bytes without a path.
pub async fn pty_stage_drop_file(
    file_name: &str,
    mime_type: &str,
    base64_data: &str,
) -> TauriResult<String> {
    invoke(
        "pty_stage_drop_file",
        &serde_json::json!({
            "fileName": file_name,
            "mimeType": mime_type,
            "base64Data": base64_data,
        })
        .to_string(),
    )
    .await
}

/// Write data to a PTY session.
pub async fn pty_write(id: &str, data: &str) -> TauriResult<()> {
    invoke(
        "pty_write",
        &serde_json::json!({ "id": id, "data": data }).to_string(),
    )
    .await
}

/// Read the OS clipboard as plain text.
pub async fn read_clipboard_text() -> TauriResult<String> {
    invoke("read_clipboard_text", "{}").await
}

/// Kill a PTY session.
pub async fn pty_kill(id: &str) -> TauriResult<()> {
    invoke("pty_kill", &serde_json::json!({ "id": id }).to_string()).await
}

/// Resize a PTY session. `owner` identifies the current xterm mount so a
/// stale remounted instance cannot resize the replacement PTY.
pub async fn pty_resize(id: &str, cols: u16, rows: u16, owner: Option<&str>) -> TauriResult<()> {
    invoke(
        "pty_resize",
        &serde_json::json!({
            "id": id,
            "cols": cols,
            "rows": rows,
            "owner": owner,
        })
        .to_string(),
    )
    .await
}

/// Check if a PTY session exists.
pub async fn pty_has_session(id: &str) -> TauriResult<bool> {
    invoke(
        "pty_has_session",
        &serde_json::json!({ "id": id }).to_string(),
    )
    .await
}

/// Check if a PTY session is ready.
pub async fn pty_is_ready(id: &str) -> TauriResult<bool> {
    invoke("pty_is_ready", &serde_json::json!({ "id": id }).to_string()).await
}

/// Get the current working directory of a PTY session.
pub async fn pty_get_cwd(id: &str) -> TauriResult<Option<String>> {
    invoke("pty_get_cwd", &serde_json::json!({ "id": id }).to_string()).await
}

/// Get the current foreground process name for a PTY session.
pub async fn pty_foreground_process(id: &str) -> TauriResult<String> {
    invoke(
        "pty_foreground_process",
        &serde_json::json!({ "id": id }).to_string(),
    )
    .await
}

/// Mark a PTY session as being rendered by xterm.js (or not).
/// When true, the backend skips emitting `terminal:data` cell-delta
/// events because xterm.js parses raw ANSI bytes itself.
pub async fn pty_set_xterm(id: &str, is_xterm: bool) -> TauriResult<()> {
    invoke(
        "pty_set_xterm",
        &serde_json::json!({ "id": id, "isXterm": is_xterm }).to_string(),
    )
    .await
}

/// Pause/resume raw `pty:raw` event emission for a session.
///
/// When paused, the backend read loop keeps reading from the PTY fd (so the
/// shell doesn't block on a full pipe) but suppresses `pty:raw` emission.
/// Accumulated bytes are flushed as a single burst when unpaused. Used by
/// the xterm.js remount path to close the stream-gap desync during pane swaps:
/// pause before unlisten on unmount, unpause after replay on remount.
/// Tell the backend a `pty:raw` listener has (re)subscribed for `id` — the
/// "someone is listening again" handshake. Clears `raw_paused`; the backend
/// read loop detects the true→false transition on its next iteration and
/// flushes the accumulated burst. Call this right after `pty_listen_raw`
/// subscribes. Makes a session that was paused (e.g. pane dropped without
/// remount) self-heal on the next re-show, closing the stuck-paused gap.
/// No-op (and Ok) if the session does not exist yet on a brand-new spawn.
pub async fn pty_attach_listener(id: &str, owner: &str, replace_current: bool) -> TauriResult<u64> {
    // Keep the generation as a string across IPC: JavaScript numbers cannot
    // represent every u64 exactly, while generations are part of a race-safety
    // lease and must never be rounded.
    let generation: String = invoke(
        "pty_attach_listener",
        &serde_json::json!({
            "id": id,
            "owner": owner,
            "replaceCurrent": replace_current,
        })
        .to_string(),
    )
    .await?;
    generation
        .parse::<u64>()
        .map_err(|_| JsValue::from_str("invalid listener generation"))
}

/// Detach a raw PTY listener lease. Older generations are ignored by the
/// backend, so a stale xterm teardown cannot pause a newer mount.
pub async fn pty_detach_listener(id: &str, owner: &str, generation: u64) -> TauriResult<bool> {
    invoke(
        "pty_detach_listener",
        &serde_json::json!({
            "id": id,
            "owner": owner,
            "generation": generation.to_string(),
        })
        .to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// Trusted workspace roots
// ---------------------------------------------------------------------------

/// Add a directory to the backend's trusted-roots set. Called when the user
/// launches a Space (the authorization gesture that lets a terminal / agent
/// operate in a directory outside the app's own project root). Idempotent.
pub async fn workspace_add_trusted_root(dir: &str) -> TauriResult<()> {
    invoke(
        "workspace_add_trusted_root",
        &serde_json::json!({ "dir": dir }).to_string(),
    )
    .await
}

/// Remove a directory from the backend's trusted-roots set.
pub async fn workspace_remove_trusted_root(dir: &str) -> TauriResult<()> {
    invoke(
        "workspace_remove_trusted_root",
        &serde_json::json!({ "dir": dir }).to_string(),
    )
    .await
}

/// List the canonicalized trusted workspace roots.
pub async fn workspace_list_trusted_roots() -> TauriResult<Vec<String>> {
    let raw: String = invoke(
        "workspace_list_trusted_roots",
        &serde_json::json!({}).to_string(),
    )
    .await?;
    serde_json::from_str(&raw)
        .map_err(|e| js_sys::Error::new(&format!("failed to parse trusted roots: {}", e)).into())
}

// ---------------------------------------------------------------------------
// Pane history operations
// ---------------------------------------------------------------------------

/// A line of output history from a pane.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct OutputLine {
    pub pane_id: String,
    pub line_num: u32,
    pub timestamp: u64,
    pub text: String,
}

/// Get the accumulated output history for a pane.
pub async fn get_pane_history(pane_id: &str) -> TauriResult<Vec<OutputLine>> {
    let raw: String = invoke(
        "get_pane_history",
        &serde_json::json!({ "paneId": pane_id }).to_string(),
    )
    .await?;
    serde_json::from_str(&raw)
        .map_err(|e| js_sys::Error::new(&format!("failed to parse output history: {}", e)).into())
}
/// A raw PTY listener: subscribes to the base64 `pty:raw:<id>` event stream.
/// On unmount invoke the returned `unlisten` and call `pty_detach_listener`
/// so the backend pauses the raw flushes for this session.
pub struct PtyRawListener {
    /// Event-mode unlisten closure.
    pub unlisten: Option<Box<dyn FnOnce()>>,
    /// Relay binary-mode unlisten closure (shim `relayRaw.listen`). Present
    /// only when running over the mobile-mirror relay.
    pub unlisten_raw: Option<Box<dyn FnOnce()>>,
}

/// Register a relay binary-frame sink for `pane` when the shim exposes
/// `__TAURI__.relayRaw` (mobile mirror only). Returns `None` on the desktop
/// webview (no shim) or if the call fails.
///
/// The callback closure is intentionally leaked (`forget`) — the sink must
/// stay callable for the whole relay session, matching the leak-for-lifetime
/// pattern used by the text event listener registration.
fn relay_raw_listen(
    pane: &str,
    mut callback: impl FnMut(Vec<u8>) + 'static,
) -> Option<Box<dyn FnOnce()>> {
    let window = web_sys::window()?;
    let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__")).ok()?;
    let relay_raw = js_sys::Reflect::get(&tauri, &JsValue::from_str("relayRaw")).ok()?;
    let listen_fn = js_sys::Reflect::get(&relay_raw, &JsValue::from_str("listen"))
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    let cb = Closure::wrap(Box::new(move |data: js_sys::Uint8Array| {
        let mut bytes = vec![0u8; data.length() as usize];
        data.copy_to(&mut bytes);
        callback(bytes);
    }) as Box<dyn FnMut(js_sys::Uint8Array)>);
    let unlisten = listen_fn
        .call2(
            &relay_raw,
            &JsValue::from_str(pane),
            cb.as_ref().unchecked_ref(),
        )
        .ok()?
        .dyn_into::<js_sys::Function>()
        .ok()?;
    cb.forget();
    Some(Box::new(move || {
        let _ = unlisten.call0(&JsValue::NULL);
    }))
}

/// Subscribe to raw PTY byte chunks for a session.
///
/// Bytes cross IPC as base64 inside the `pty:raw:<id>` event payload; the
/// backend coalesces to one emit per 8 ms tick, so decode cost is amortized
/// per flush rather than per read.
pub fn pty_listen_raw(
    id: &str,
    callback: impl FnMut(Vec<u8>) + 'static,
) -> Result<PtyRawListener, TauriBridgeError> {
    let event_name = format!("pty:raw:{id}");

    // Shared cell so the relay binary sink and the legacy text listener can
    // both invoke the caller's callback (only one path actually fires: under
    // the relay the payload crosses as binary frames; on the desktop webview
    // the text event carries base64).
    let callback = std::rc::Rc::new(std::cell::RefCell::new(callback));

    // Relay (mobile mirror) fast path: the relay converts `pty:raw:<id>`
    // events into binary WS frames; the shim routes them to a per-pane sink
    // as raw bytes — no JSON parse, no base64, no per-flush atob on the phone.
    // The text event listener below is STILL registered: the relay keys the
    // backend's per-pane subscription (and the ownership forward gate) off
    // the `listen` frame, so skipping it would silence the binary stream too.
    let unlisten_raw = relay_raw_listen(id, {
        let callback = callback.clone();
        move |bytes| (callback.borrow_mut())(bytes)
    });

    let unlisten = listen(&event_name, {
        let callback = callback.clone();
        move |payload_str: String| {
        // The backend emits this event with a String payload, which the IPC
        // layer JSON-quotes — parse again when we land on a quoted string
        // instead of the object.
        let mut parsed: serde_json::Value = match serde_json::from_str(&payload_str) {
            Ok(v) => v,
            Err(_) => return,
        };
        if let serde_json::Value::String(inner) = &parsed {
            parsed = match serde_json::from_str(inner) {
                Ok(v) => v,
                Err(_) => return,
            };
        }
        let data_b64 = match parsed.get("data").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return,
        };

        let window = match web_sys::window() {
            Some(w) => w,
            None => return,
        };
        let atob_val = match js_sys::Reflect::get(&window, &JsValue::from_str("atob")) {
            Ok(v) => v,
            Err(_) => return,
        };
        let Ok(atob_fn) = atob_val.dyn_into::<js_sys::Function>() else {
            return;
        };
        let s_val = match atob_fn.call1(&JsValue::NULL, &JsValue::from_str(data_b64)) {
            Ok(v) => v,
            Err(_) => return,
        };
        let Some(s) = s_val.as_string() else {
            return;
        };

        // atob returns a "binary string" where each char's code point
        // equals the byte value (0-255). Iterate to build Vec<u8>.
        let mut bytes = Vec::with_capacity(s.len());
        for c in s.chars() {
            bytes.push(c as u8);
        }

        (callback.borrow_mut())(bytes);
        }
    })?;

    Ok(PtyRawListener {
        unlisten: Some(unlisten),
        unlisten_raw,
    })
}

/// Fetch the last ~64 KB of raw PTY bytes buffered for replay (base64 on the
/// wire), decoded to bytes. Returns `Ok(None)` when the pane has no replay
/// buffer (fresh spawn, killed pane, or nothing flushed yet).
///
/// Used by the mobile xterm mount to restore exact VT screen state (cursor
/// position, colors, a partial in-flight line) after a relay reconnect — the
/// ANSI-stripped `output_buffer_get` text history cannot reproduce those.
pub async fn pty_raw_replay(pane_id: &str) -> TauriResult<Option<Vec<u8>>> {
    let b64: String = invoke(
        "pty_raw_replay",
        &serde_json::json!({ "paneId": pane_id }).to_string(),
    )
    .await?;
    if b64.is_empty() {
        return Ok(None);
    }
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("No window"))?;
    let atob_val = js_sys::Reflect::get(&window, &JsValue::from_str("atob"))
        .map_err(|e| JsValue::from(format!("Reflect atob error: {e:?}")))?;
    let atob_fn = atob_val
        .dyn_into::<js_sys::Function>()
        .map_err(|_| JsValue::from_str("atob not a function"))?;
    let s_val = atob_fn
        .call1(&JsValue::NULL, &JsValue::from_str(&b64))
        .map_err(|e| JsValue::from(format!("atob error: {e:?}")))?;
    let s = s_val
        .as_string()
        .ok_or_else(|| JsValue::from_str("atob returned a non-string"))?;
    let mut bytes = Vec::with_capacity(s.len());
    for c in s.chars() {
        bytes.push(c as u8);
    }
    Ok(Some(bytes))
}

/// Start capturing the microphone for Athena voice input (desktop). The
/// backend records on-device until `voice_record_stop`; audio never leaves the
/// Mac. Errors surface permission problems (mic access, speech recognition)
/// and "already recording" states.
pub async fn voice_record_start() -> TauriResult<()> {
    invoke("voice_record_start", "{}").await
}

/// Stop the voice capture and transcribe the clip on-device. Returns the
/// transcript text on success (errors when nothing was recorded, the mic was
/// silent, or recognition failed).
pub async fn voice_record_stop() -> TauriResult<String> {
    invoke("voice_record_stop", "{}").await
}

/// Tool executor operations
pub async fn tool_execute(tool_name: &str, arguments: &str) -> TauriResult<String> {
    invoke(
        "tool_execute",
        &serde_json::json!({ "tool_name": tool_name, "arguments": arguments }).to_string(),
    )
    .await
}

pub async fn tool_list() -> TauriResult<String> {
    invoke("tool_list", "{}").await
}

pub async fn tool_openai_schema() -> TauriResult<String> {
    invoke("tool_openai_schema", "{}").await
}

// ---------------------------------------------------------------------------
// Athena chat (orchestrator) operations
// ---------------------------------------------------------------------------

/// Send a chat message to the orchestrator and return the assistant's reply.
pub async fn athena_chat(message: &str) -> TauriResult<String> {
    invoke(
        "athena_chat",
        &serde_json::json!({ "message": message }).to_string(),
    )
    .await
}

/// Start a request-scoped streaming chat turn. Events arrive on
/// `athena:stream` and are filtered by request ID in the Athena store.
pub async fn athena_chat_stream(
    message: &str,
    session_id: &str,
    request_id: &str,
) -> TauriResult<String> {
    invoke(
        "athena_chat_stream",
        &serde_json::json!({
            "message": message,
            "sessionId": session_id,
            "requestId": request_id,
        })
        .to_string(),
    )
    .await
}

/// Cancel an in-flight streaming chat turn.
pub async fn athena_cancel_stream(request_id: &str) -> TauriResult<bool> {
    invoke(
        "athena_cancel_stream",
        &serde_json::json!({ "requestId": request_id }).to_string(),
    )
    .await
}

/// Send a chat message within a specific session.
pub async fn athena_chat_with_session(message: &str, session_id: &str) -> TauriResult<String> {
    invoke(
        "athena_chat_with_session",
        &serde_json::json!({ "message": message, "sessionId": session_id }).to_string(),
    )
    .await
}

/// Send a chat message with image attachments.
pub async fn athena_chat_with_images(message: &str, images: &str) -> TauriResult<String> {
    invoke(
        "athena_chat_with_images",
        &serde_json::json!({ "message": message, "images": images }).to_string(),
    )
    .await
}

/// Summarize a raw prompt into a short (2-3 word) title using the
/// configured LLM. Does NOT touch conversation history.
pub async fn summarize_agent_title(raw_prompt: &str) -> TauriResult<String> {
    invoke(
        "summarize_agent_title",
        &serde_json::json!({ "rawPrompt": raw_prompt }).to_string(),
    )
    .await
}

/// Respond to Athena's pending ask_user tool.
pub async fn athena_user_answer(request_id: &str, answer: &str) -> TauriResult<bool> {
    invoke(
        "athena_user_answer",
        &serde_json::json!({ "requestId": request_id, "answer": answer }).to_string(),
    )
    .await
}

/// Clear the orchestrator's conversation context.
pub async fn athena_clear_context() -> TauriResult<JsValue> {
    invoke("athena_clear_context", "{}").await
}

/// Set session history context on the orchestrator.
pub async fn athena_set_session_context(history: &str) -> TauriResult<JsValue> {
    invoke(
        "athena_set_session_context",
        &serde_json::json!({ "history": history }).to_string(),
    )
    .await
}

/// Test whether the saved LLM API key can be read from the keyring.
/// Returns a JSON object: { ok: bool, message: string }.
pub async fn test_llm_api_key() -> TauriResult<String> {
    invoke("test_llm_api_key", "{}").await
}

/// List models available from an OpenAI-compatible `/models` endpoint.
/// Returns a JSON string: { ok: bool, models: [string], message: string }.
/// The `api_key` param carries a freshly-typed (not-yet-saved) key so
/// "Fetch models" works before the user hits Save; the backend falls back to
/// the keyring slot for `provider` (scoped when a preset id, legacy otherwise)
/// when it is empty. Wire keys are camelCase (Tauri 2 renames the snake_case
/// Rust params `base_url`/`api_key`).
pub async fn llm_list_models(base_url: &str, api_key: &str, provider: &str) -> TauriResult<String> {
    invoke(
        "llm_list_models",
        &serde_json::json!({ "baseUrl": base_url, "apiKey": api_key, "provider": provider })
            .to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// Browser operations
// ---------------------------------------------------------------------------

/// Open/show a browser panel.
pub async fn browser_show(id: &str, url: &str) -> TauriResult<JsValue> {
    invoke(
        "browser_show",
        &serde_json::json!({ "id": id, "url": url }).to_string(),
    )
    .await
}

/// Hide/close a browser panel.
pub async fn browser_hide(id: &str) -> TauriResult<JsValue> {
    invoke("browser_hide", &serde_json::json!({ "id": id }).to_string()).await
}

/// Navigate a browser panel to a new URL.
pub async fn browser_navigate(id: &str, url: &str) -> TauriResult<JsValue> {
    invoke(
        "browser_navigate",
        &serde_json::json!({ "id": id, "url": url }).to_string(),
    )
    .await
}

/// Navigate a browser panel back in history.
pub async fn browser_back(id: &str) -> TauriResult<String> {
    invoke("browser_back", &serde_json::json!({ "id": id }).to_string()).await
}

/// Navigate a browser panel forward in history.
pub async fn browser_forward(id: &str) -> TauriResult<String> {
    invoke(
        "browser_forward",
        &serde_json::json!({ "id": id }).to_string(),
    )
    .await
}

/// Reload a browser panel.
pub async fn browser_reload(id: &str) -> TauriResult<JsValue> {
    invoke(
        "browser_reload",
        &serde_json::json!({ "id": id }).to_string(),
    )
    .await
}

/// Reposition/resize the browser child webview to a frontend-measured rect
/// (logical pixels). Off-screen coordinates "park" the webview while keeping the
/// page alive.
pub async fn browser_set_bounds(
    id: &str,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> TauriResult<JsValue> {
    invoke(
        "browser_set_bounds",
        &serde_json::json!({
            "id": id,
            "x": x,
            "y": y,
            "width": width,
            "height": height,
        })
        .to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// Plugin operations
// ---------------------------------------------------------------------------

/// List all registered plugins.
pub async fn plugin_list() -> TauriResult<String> {
    invoke("plugin_list", "{}").await
}

/// Get a specific plugin's info.
pub async fn plugin_get(plugin_id: &str) -> TauriResult<String> {
    invoke(
        "plugin_get",
        &serde_json::json!({ "pluginId": plugin_id }).to_string(),
    )
    .await
}

/// Register a new plugin.
pub async fn plugin_register(plugin_id: &str, name: &str, version: &str) -> TauriResult<String> {
    invoke(
        "plugin_register",
        &serde_json::json!({ "pluginId": plugin_id, "name": name, "version": version }).to_string(),
    )
    .await
}

/// Unregister a plugin.
pub async fn plugin_unregister(plugin_id: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_unregister",
        &serde_json::json!({ "pluginId": plugin_id }).to_string(),
    )
    .await
}

/// Enable a plugin.
pub async fn plugin_enable(plugin_id: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_enable",
        &serde_json::json!({ "pluginId": plugin_id }).to_string(),
    )
    .await
}

/// Disable a plugin.
pub async fn plugin_disable(plugin_id: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_disable",
        &serde_json::json!({ "pluginId": plugin_id }).to_string(),
    )
    .await
}

/// Get a plugin's configuration.
pub async fn plugin_get_config(plugin_id: &str) -> TauriResult<String> {
    invoke(
        "plugin_get_config",
        &serde_json::json!({ "pluginId": plugin_id }).to_string(),
    )
    .await
}

/// Set a plugin's configuration.
pub async fn plugin_set_config(plugin_id: &str, config: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_set_config",
        &serde_json::json!({ "pluginId": plugin_id, "config": config }).to_string(),
    )
    .await
}

/// Set a plugin's error state.
pub async fn plugin_set_error(plugin_id: &str, error: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_set_error",
        &serde_json::json!({ "pluginId": plugin_id, "error": error }).to_string(),
    )
    .await
}

/// List all plugin host sessions.
pub async fn plugin_host_list_sessions() -> TauriResult<String> {
    invoke("plugin_host_list_sessions", "{}").await
}

/// Get a specific plugin host session.
pub async fn plugin_host_get_session(session_id: &str) -> TauriResult<String> {
    invoke(
        "plugin_host_get_session",
        &serde_json::json!({ "sessionId": session_id }).to_string(),
    )
    .await
}

/// Emit a plugin host event.
pub async fn plugin_host_emit_event(event_type: &str, data: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_host_emit_event",
        &serde_json::json!({ "eventType": event_type, "data": data }).to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// Mobile mirror relay operations
// ---------------------------------------------------------------------------

/// Query the live mobile-mirror relay status. Returns a JSON object
/// `{ "running": bool, "url": Option<String>, "port": u16, "qr_svg_base64": Option<String> }`
/// — parsed by the Settings panel (raw string return follows the
/// `session_list` / `output_buffer_list` convention).
pub async fn relay_status() -> TauriResult<String> {
    invoke("relay_status", "{}").await
}

/// Start the mobile-mirror relay. On success returns the bound socket
/// address as a string. The port is ephemeral (fresh per start), so the URL
/// changes on every enable.
pub async fn relay_start() -> TauriResult<String> {
    invoke("relay_start", "{}").await
}

/// Stop the mobile-mirror relay. Idempotent — succeeds even if not running.
pub async fn relay_stop() -> TauriResult<()> {
    invoke("relay_stop", "{}").await
}

/// Mark a pane as shared (or unshared) with the mobile mirror. Desktop-only;
/// a paired phone cannot self-authorize panes through the relay. Tauri 2 maps
/// the snake_case `pane_id` param to the `paneId` wire key.
pub async fn relay_set_pane_shared(pane_id: &str, shared: bool) -> TauriResult<()> {
    invoke(
        "relay_set_pane_shared",
        &serde_json::json!({ "paneId": pane_id, "shared": shared }).to_string(),
    )
    .await
}

/// List the panes currently shared with the mobile mirror (sorted).
pub async fn relay_list_shared_panes() -> TauriResult<Vec<String>> {
    let raw: String = invoke("relay_list_shared_panes", "{}").await?;
    serde_json::from_str(&raw)
        .map_err(|e| js_sys::Error::new(&format!("failed to parse shared panes: {}", e)).into())
}

/// Approve (`approved = true`) or deny a pending Mobile Mirror pairing
/// request surfaced by the `relay:pairingRequest` event. Desktop-only; a
/// paired phone cannot self-authorize its own connection.
pub async fn relay_pairing_respond(request_id: &str, approved: bool) -> TauriResult<()> {
    invoke(
        "relay_pairing_respond",
        &serde_json::json!({ "requestId": request_id, "approved": approved }).to_string(),
    )
    .await
}

/// Ask the desktop to share a pane with this phone. The desktop operator
/// receives a `relay:paneShareRequest` prompt and may approve (flipping the
/// pane's share toggle) or ignore it. Harmless if the pane is already
/// accessible — the operator can just dismiss the prompt.
pub async fn relay_request_pane_share(pane_id: &str) -> TauriResult<()> {
    invoke(
        "relay_request_pane_share",
        &serde_json::json!({ "paneId": pane_id }).to_string(),
    )
    .await
}

// ---------------------------------------------------------------------------
// Event listener infrastructure
// ---------------------------------------------------------------------------

/// Error type for Tauri bridge operations that can fail outside the
/// Tauri context (e.g. running in a plain browser without the __TAURI__
/// global).
#[derive(Debug, Clone)]
pub struct TauriBridgeError {
    pub message: String,
}

impl std::fmt::Display for TauriBridgeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TauriBridgeError: {}", self.message)
    }
}

impl std::error::Error for TauriBridgeError {}

/// Listen for Tauri push events from the backend.
/// The callback receives the event payload as a String.
/// Returns a boxed unlisten function that, when called, removes the listener
/// and allows the closure to be garbage-collected. Callers that want
/// cleanup should store and invoke the returned function on component unmount.
/// Callers that discard the return value ("let _ = ") will have the listener
/// live for the app lifetime (no behavioral change from before).
pub fn listen(
    event: &str,
    callback: impl FnMut(String) + 'static,
) -> Result<Box<dyn FnOnce()>, TauriBridgeError> {
    // Performance instrumentation: every delivered push event is counted here
    // (the single chokepoint for backend→frontend traffic).
    let event_for_metrics = event.to_string();
    let window = web_sys::window().ok_or_else(|| TauriBridgeError {
        message: format!("listen({}): no window object", event),
    })?;

    let tauri = js_sys::Reflect::get(&window, &JsValue::from_str("__TAURI__")).map_err(|_| {
        TauriBridgeError {
            message: format!("listen({}): __TAURI__ not available", event),
        }
    })?;
    if tauri.is_undefined() || tauri.is_null() {
        return Err(TauriBridgeError {
            message: format!("listen({}): __TAURI__ is null/undefined", event),
        });
    }

    let event_mod = js_sys::Reflect::get(&tauri, &JsValue::from_str("event")).map_err(|_| {
        TauriBridgeError {
            message: format!("listen({}): __TAURI__.event not available", event),
        }
    })?;
    if event_mod.is_undefined() || event_mod.is_null() {
        return Err(TauriBridgeError {
            message: format!("listen({}): __TAURI__.event is null/undefined", event),
        });
    }

    let listen_fn_val =
        js_sys::Reflect::get(&event_mod, &JsValue::from_str("listen")).map_err(|_| {
            TauriBridgeError {
                message: format!("listen({}): __TAURI__.event.listen not found", event),
            }
        })?;
    if listen_fn_val.is_undefined() {
        return Err(TauriBridgeError {
            message: format!("listen({}): __TAURI__.event.listen is undefined", event),
        });
    }
    let listen_fn = listen_fn_val
        .dyn_into::<js_sys::Function>()
        .map_err(|_| TauriBridgeError {
            message: format!("listen({}): listen is not a function", event),
        })?;

    let mut callback = callback;
    let closure = wasm_bindgen::closure::Closure::wrap(Box::new(move |event_obj: JsValue| {
        if let Ok(obj) = event_obj.dyn_into::<js_sys::Object>() {
            let payload = js_sys::Reflect::get(&obj, &JsValue::from_str("payload"))
                .unwrap_or(JsValue::UNDEFINED);
            let payload_str = if payload.is_string() {
                payload.as_string().unwrap_or_default()
            } else {
                js_sys::JSON::stringify(&payload)
                    .map(|s| s.as_string().unwrap_or_default())
                    .unwrap_or_default()
            };
            crate::utils::perf_metrics::record_event(&event_for_metrics, payload_str.len() as u64);
            callback(payload_str);
        }
    }) as Box<dyn FnMut(JsValue)>);

    // Tauri v2's `event.listen` returns a Promise<UnlistenFn>. Keep the
    // registration value alive and resolve it during teardown. Supporting a
    // direct function as well keeps this bridge compatible with older test
    // shims and avoids leaking a listener when a component remounts before
    // native registration has completed.
    let registration = listen_fn
        .call2(&event_mod, &JsValue::from_str(event), closure.as_ref())
        .map_err(|e| TauriBridgeError {
            message: format!("listen({}): failed to register listener: {:?}", event, e),
        })?;

    // Keep the Rust callback rooted until the resolved unlisten function has
    // run. This is especially important for xterm panes: pane swaps unmount
    // and remount listeners quickly, so a stale callback must be removed even
    // when registration is still pending.
    let closure_js = closure.into_js_value();

    let unlisten_fn = Box::new(move || {
        if let Ok(unlisten) = registration.clone().dyn_into::<js_sys::Function>() {
            let _ = unlisten.call0(&JsValue::NULL);
            drop(closure_js);
            return;
        }

        let Ok(then_val) = js_sys::Reflect::get(&registration, &JsValue::from_str("then")) else {
            drop(closure_js);
            return;
        };
        let Ok(then_fn) = then_val.dyn_into::<js_sys::Function>() else {
            drop(closure_js);
            return;
        };

        let cleanup = wasm_bindgen::closure::Closure::once_into_js(Box::new(
            move |resolved_unlisten: JsValue| {
                if let Ok(unlisten) = resolved_unlisten.dyn_into::<js_sys::Function>() {
                    let _ = unlisten.call0(&JsValue::NULL);
                }
                drop(closure_js);
            },
        )
            as Box<dyn FnOnce(JsValue)>);
        let _ = then_fn.call1(&registration, cleanup.as_ref());
    });

    Ok(unlisten_fn)
}
