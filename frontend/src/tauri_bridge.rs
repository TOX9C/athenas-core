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

    let args_value: JsValue = serde_json::from_str(args)
        .map(|v: serde_json::Value| {
            js_sys::JSON::parse(&v.to_string()).unwrap_or(JsValue::UNDEFINED)
        })
        .unwrap_or(JsValue::UNDEFINED);

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
    web_sys::console::log_1(
        &format!(
            "[tauri_bridge] store_set key={:?} value_len={}",
            key,
            value.len()
        )
        .into(),
    );
    let result = invoke(
        "store_set",
        &serde_json::json!({ "key": key, "value": value }).to_string(),
    )
    .await;
    if result.is_ok() {
        web_sys::console::log_1(&format!("[tauri_bridge] store_set SUCCESS key={:?}", key).into());
    } else {
        web_sys::console::error_1(&format!("[tauri_bridge] store_set FAILED key={:?}", key).into());
    }
    result
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
        &serde_json::json!({ "pane_id": pane_id, "data": data, "agent_type": agent_type })
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
        &serde_json::json!({ "pane_id": pane_id, "limit": limit, "offset": offset }).to_string(),
    )
    .await
}

pub async fn output_buffer_list() -> TauriResult<String> {
    invoke("output_buffer_list", "{}").await
}

pub async fn output_buffer_clear(pane_id: &str) -> TauriResult<String> {
    invoke(
        "output_buffer_clear",
        &serde_json::json!({ "pane_id": pane_id }).to_string(),
    )
    .await
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

pub async fn notification_mark_all_read() -> TauriResult<()> {
    invoke("notification_mark_all_read", "{}").await
}

pub async fn notification_clear_all() -> TauriResult<()> {
    invoke("notification_clear_all", "{}").await
}

pub async fn notification_dismiss(id: &str) -> TauriResult<()> {
    invoke(
        "notification_dismiss",
        &serde_json::json!({ "id": id }).to_string(),
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
        &serde_json::json!({ "step_id": step_id, "status": status, "pane_id": pane_id })
            .to_string(),
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
        &serde_json::json!({ "agent_id": agent_id, "method": method, "params": params })
            .to_string(),
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
pub async fn swarm_read_state(dir: &str) -> TauriResult<String> {
    invoke(
        "swarm_read_state",
        &serde_json::json!({ "dir": dir }).to_string(),
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
    invoke(
        "swarm_read_mailbox",
        &serde_json::json!({ "dir": dir, "agent_id": agent_id }).to_string(),
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
pub async fn pty_spawn(id: &str, cwd: &str, shell: &str, cols: u16, rows: u16) -> TauriResult<()> {
    invoke(
        "pty_spawn",
        &serde_json::json!({ "id": id, "cwd": cwd, "shell": shell, "cols": cols, "rows": rows })
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

/// Resize a PTY session.
pub async fn pty_resize(id: &str, cols: u16, rows: u16) -> TauriResult<()> {
    invoke(
        "pty_resize",
        &serde_json::json!({ "id": id, "cols": cols, "rows": rows }).to_string(),
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
        &serde_json::json!({ "id": id, "is_xterm": is_xterm }).to_string(),
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
        &serde_json::json!({ "pane_id": pane_id }).to_string(),
    )
    .await?;
    serde_json::from_str(&raw)
        .map_err(|e| js_sys::Error::new(&format!("failed to parse output history: {}", e)).into())
}
///
/// The backend emits `pty:raw` events with a JSON payload of the form
/// `{ "session_id": "<id>", "data": "<base64>" }`. This function filters
/// by `id` and decodes `data` from base64 to `Vec<u8>` before invoking
/// `callback`. Events for other sessions are dropped silently.
///
/// The returned unlisten function must be invoked on component unmount
/// to release the listener and the underlying closure. Discarding the
/// return value with `let _ = ...` will keep the listener alive for
/// the app lifetime.
pub fn pty_listen_raw(
    id: &str,
    mut callback: impl FnMut(Vec<u8>) + 'static,
) -> Result<Box<dyn FnOnce()>, TauriBridgeError> {
    let id_owned = id.to_string();
    listen("pty:raw", move |payload_str: String| {
        let parsed: serde_json::Value = match serde_json::from_str(&payload_str) {
            Ok(v) => v,
            Err(_) => return,
        };
        let session_id = match parsed.get("sessionId").and_then(|v| v.as_str()) {
            Some(s) => s,
            None => return,
        };
        if session_id != id_owned {
            return;
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

        callback(bytes);
    })
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

/// Send a chat message within a specific session.
pub async fn athena_chat_with_session(message: &str, session_id: &str) -> TauriResult<String> {
    invoke(
        "athena_chat_with_session",
        &serde_json::json!({ "message": message, "session_id": session_id }).to_string(),
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
        &serde_json::json!({ "plugin_id": plugin_id }).to_string(),
    )
    .await
}

/// Register a new plugin.
pub async fn plugin_register(plugin_id: &str, name: &str, version: &str) -> TauriResult<String> {
    invoke(
        "plugin_register",
        &serde_json::json!({ "plugin_id": plugin_id, "name": name, "version": version })
            .to_string(),
    )
    .await
}

/// Unregister a plugin.
pub async fn plugin_unregister(plugin_id: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_unregister",
        &serde_json::json!({ "plugin_id": plugin_id }).to_string(),
    )
    .await
}

/// Enable a plugin.
pub async fn plugin_enable(plugin_id: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_enable",
        &serde_json::json!({ "plugin_id": plugin_id }).to_string(),
    )
    .await
}

/// Disable a plugin.
pub async fn plugin_disable(plugin_id: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_disable",
        &serde_json::json!({ "plugin_id": plugin_id }).to_string(),
    )
    .await
}

/// Get a plugin's configuration.
pub async fn plugin_get_config(plugin_id: &str) -> TauriResult<String> {
    invoke(
        "plugin_get_config",
        &serde_json::json!({ "plugin_id": plugin_id }).to_string(),
    )
    .await
}

/// Set a plugin's configuration.
pub async fn plugin_set_config(plugin_id: &str, config: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_set_config",
        &serde_json::json!({ "plugin_id": plugin_id, "config": config }).to_string(),
    )
    .await
}

/// Set a plugin's error state.
pub async fn plugin_set_error(plugin_id: &str, error: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_set_error",
        &serde_json::json!({ "plugin_id": plugin_id, "error": error }).to_string(),
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
        &serde_json::json!({ "session_id": session_id }).to_string(),
    )
    .await
}

/// Emit a plugin host event.
pub async fn plugin_host_emit_event(event_type: &str, data: &str) -> TauriResult<JsValue> {
    invoke(
        "plugin_host_emit_event",
        &serde_json::json!({ "event_type": event_type, "data": data }).to_string(),
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
            callback(payload_str);
        }
    }) as Box<dyn FnMut(JsValue)>);

    // Tauri's listen() returns an UnlistenFn (a JS function).
    let unlisten_val = listen_fn
        .call2(&event_mod, &JsValue::from_str(event), closure.as_ref())
        .map_err(|e| TauriBridgeError {
            message: format!("listen({}): failed to register listener: {:?}", event, e),
        })?;

    // Convert the Rust closure to a JS value so it stays rooted in JS GC
    // as long as the returned unlisten box is alive. Once the unlisten box
    // is dropped (after calling unlisten), the JS GC can collect both.
    let closure_js = closure.into_js_value();

    // Build the unlisten function. It calls Tauri's unlisten and then
    // drops the JS references, allowing GC of both the closure and the
    // unlisten function object.
    let unlisten_fn = Box::new(move || {
        if let Ok(unlisten) = unlisten_val.dyn_into::<js_sys::Function>() {
            let _ = unlisten.call0(&JsValue::NULL);
        }
        // closure_js and unlisten_val are dropped here, releasing JS GC roots
        drop(closure_js);
    });

    Ok(unlisten_fn)
}
