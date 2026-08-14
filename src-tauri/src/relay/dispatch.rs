//! Dispatch: map `{cmd, args}` WS messages to the real `#[tauri::command]`
//! implementations.
//!
//! The relay is a transparent L7 proxy. Each `invoke(cmd, args)` over the WS
//! deserialises `args` into the command's typed parameters, borrows
//! `State<'_, AppState>` from the app handle for the duration of the call, and
//! invokes the underlying function directly — the exact same code path the
//! desktop Tauri handler uses. There is no duplicated command logic.
//!
//! Conventions:
//!   - `args` is a serde_json::Value::Object keyed by parameter name.
//!   - Optional params use `opt::<T>("name")`; required use `req::<T>("name")`.
//!   - The return is JSON-serialised success (Value) or a string error.
//!   - Error types like `CommandError` (which serializes to a string) and plain
//!     `String` all map to `Err(String)` — the shape the frontend bridge unpicks.

use serde_json::{Map, Value};
use tauri::Manager;

use crate::commands;
use crate::state::AppState;

use super::RelayCtx;

/// Dispatch an `invoke(cmd, args)` to the real command implementation.
/// Returns JSON-serialised success or a string error; the WS layer maps these
/// into `{t:"resp", id, ok, result|error}` on the wire.
pub async fn dispatch(ctx: &RelayCtx, cmd: &str, args: Value) -> Result<Value, String> {
    let app = &ctx.app_handle;
    let state = app.state::<AppState>();
    let opts = Args::new(args);

    match cmd {
        "agent_comms_token" => {
            let out = commands::agent_comms_token()?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_comms_sessions" => {
            let out = commands::agent_comms_sessions(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_comms_send" => {
            let agent_id = opts.req::<String>("agent_id")?;
            let method = opts.req::<String>("method")?;
            let params = opts.req::<String>("params")?;
            let out = commands::agent_comms_send(state, agent_id, method, params)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agents_list" => {
            let out = commands::agents_list(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_get_status" => {
            let agent_id = opts.req::<String>("agent_id")?;
            let out = commands::agent_get_status(state, agent_id).map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_respond_input" => {
            let request_id = opts.req::<String>("request_id")?;
            let response = opts.req::<String>("response")?;
            let out = commands::agent_respond_input(state, request_id, response)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_cancel_input" => {
            let request_id = opts.req::<String>("request_id")?;
            let out = commands::agent_cancel_input(state, request_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_send_message" => {
            let agent_id = opts.req::<String>("agent_id")?;
            let method = opts.req::<String>("method")?;
            let params = opts.req::<String>("params")?;
            let out = commands::agent_send_message(state, agent_id, method, params)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_disconnect" => {
            let agent_id = opts.req::<String>("agent_id")?;
            let out = commands::agent_disconnect(state, agent_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "agent_get_token" => {
            let out = commands::agent_get_token()?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "athena_chat" => {
            let message = opts.req::<String>("message")?;
            let out = commands::athena_chat(state, message).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "athena_chat_stream" => {
            let message = opts.req::<String>("message")?;
            let session_id = opts.req::<String>("session_id")?;
            let request_id = opts.req::<String>("request_id")?;
            let out = commands::athena_chat_stream(state, message, session_id, request_id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "athena_cancel_stream" => {
            let request_id = opts.req::<String>("request_id")?;
            let out = commands::athena_cancel_stream(state, request_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "athena_chat_with_session" => {
            let message = opts.req::<String>("message")?;
            let session_id = opts.req::<String>("session_id")?;
            let out = commands::athena_chat_with_session(state, message, session_id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "athena_chat_with_images" => {
            let message = opts.req::<String>("message")?;
            let images = opts.req::<String>("images")?;
            let out = commands::athena_chat_with_images(state, message, images).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "summarize_agent_title" => {
            let raw_prompt = opts
                .req::<String>("rawPrompt")
                .or_else(|_| opts.req::<String>("raw_prompt"))?;
            let out = commands::summarize_agent_title(state, raw_prompt).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "athena_clear_context" => {
            commands::athena_clear_context(state).await?;
            Ok(Value::Null)
        }
        "athena_set_session_context" => {
            let history = opts.req::<String>("history")?;
            commands::athena_set_session_context(state, history).await?;
            Ok(Value::Null)
        }
        "athena_user_answer" => {
            let request_id = opts.req::<String>("request_id")?;
            let answer = opts.req::<String>("answer")?;
            let out = commands::athena_user_answer(state, request_id, answer)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "store_api_key" => {
            let key = opts.req::<String>("key")?;
            commands::store_api_key(key)?;
            Ok(Value::Null)
        }
        "clear_api_key" => {
            commands::clear_api_key()?;
            Ok(Value::Null)
        }
        "browser_show" => {
            let id = opts.req::<String>("id")?;
            let url = opts.req::<String>("url")?;
            commands::browser_show(state, id, url)?;
            Ok(Value::Null)
        }
        "browser_hide" => {
            let id = opts.req::<String>("id")?;
            commands::browser_hide(state, id)?;
            Ok(Value::Null)
        }
        "browser_navigate" => {
            let id = opts.req::<String>("id")?;
            let url = opts.req::<String>("url")?;
            commands::browser_navigate(state, id, url)?;
            Ok(Value::Null)
        }
        "browser_back" => {
            let id = opts.req::<String>("id")?;
            let out = commands::browser_back(state, id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "browser_forward" => {
            let id = opts.req::<String>("id")?;
            let out = commands::browser_forward(state, id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "browser_reload" => {
            let id = opts.req::<String>("id")?;
            commands::browser_reload(state, id)?;
            Ok(Value::Null)
        }
        "browser_set_bounds" => {
            let id = opts.req::<String>("id")?;
            let x = opts.req::<f64>("x")?;
            let y = opts.req::<f64>("y")?;
            let width = opts.req::<f64>("width")?;
            let height = opts.req::<f64>("height")?;
            commands::browser_set_bounds(state, id, x, y, width, height)?;
            Ok(Value::Null)
        }
        "fs_read_file" => {
            let path = opts.req::<String>("path")?;
            let out = commands::fs_read_file(state, path).await.map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "fs_list_dir" => {
            let path = opts.req::<String>("path")?;
            let out = commands::fs_list_dir(state, path).await.map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "fs_write_file" => {
            let path = opts.req::<String>("path")?;
            let content = opts.req::<String>("content")?;
            commands::fs_write_file(state, path, content)
                .await
                .map_err(to_err)?;
            Ok(Value::Null)
        }
        "fs_exists" => {
            let path = opts.req::<String>("path")?;
            let out = commands::fs_exists(state, path);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "fs_read_file_as_base64" => {
            let path = opts.req::<String>("path")?;
            let out = commands::fs_read_file_as_base64(state, path)
                .await
                .map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "fs_show_open_dialog" => {
            let title = opts.opt::<Option<String>>("title")?;
            let multiple = opts.opt::<Option<bool>>("multiple")?;
            let directory = opts.opt::<Option<bool>>("directory")?;
            let out =
                commands::fs_show_open_dialog(ctx.app_handle.clone(), title, multiple, directory)
                    .await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "fs_show_image_dialog" => {
            let out = commands::fs_show_image_dialog(ctx.app_handle.clone()).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "fs_search_files" => {
            let pattern = opts.req::<String>("pattern")?;
            let path = opts.req::<String>("path")?;
            let out = commands::fs_search_files(state, pattern, path).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "kanban_get_tasks" => {
            let out = commands::kanban_get_tasks(state).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "kanban_create_task" => {
            let title = opts.req::<String>("title")?;
            let description = opts.opt::<Option<String>>("description")?;
            let out = commands::kanban_create_task(state, title, description).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "kanban_update_task" => {
            let task_id = opts.req::<String>("task_id")?;
            let title = opts.opt::<Option<String>>("title")?;
            let description = opts.opt::<Option<String>>("description")?;
            let status = opts.opt::<Option<String>>("status")?;
            let out =
                commands::kanban_update_task(state, task_id, title, description, status).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "kanban_delete_task" => {
            let task_id = opts.req::<String>("task_id")?;
            commands::kanban_delete_task(state, task_id).await?;
            Ok(Value::Null)
        }
        "mcp_init" => {
            let _port = opts.req::<u16>("port")?;
            commands::mcp_init(state, _port).await?;
            Ok(Value::Null)
        }
        "mcp_shutdown" => {
            commands::mcp_shutdown(state).await?;
            Ok(Value::Null)
        }
        "mcp_handle_request" => {
            let request = opts.req::<String>("request")?;
            let out = commands::mcp_handle_request(state, request).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "mcp_broadcast" => {
            let method = opts.req::<String>("method")?;
            let params = opts.req::<String>("params")?;
            commands::mcp_broadcast(state, method, params).await?;
            Ok(Value::Null)
        }
        "mcp_tools" => {
            let out = commands::mcp_tools()?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_push" => {
            let title = opts.req::<String>("title")?;
            let message = opts.req::<String>("message")?;
            let level = opts.opt::<Option<String>>("level")?;
            let out = commands::notification_push(state, title, message, level)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_history" => {
            let limit = opts.opt::<Option<usize>>("limit")?;
            let out = commands::notification_history(state, limit)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_count" => {
            let out = commands::notification_count(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_mark_read" => {
            let notification_id = opts
                .req::<String>("id")
                .or_else(|_| opts.req::<String>("notification_id"))?;
            let out = commands::notification_mark_read(state, notification_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_mark_all_read" => {
            let out = commands::notification_mark_all_read(state);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_dismiss" => {
            let notification_id = opts
                .req::<String>("id")
                .or_else(|_| opts.req::<String>("notification_id"))?;
            let out = commands::notification_dismiss(state, notification_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_clear_all" => {
            let out = commands::notification_clear_all(state);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "notification_counts" => {
            let out = commands::notification_counts(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "output_buffer_get" => {
            let pane_id = opts.req::<String>("pane_id")?;
            let limit = opts.opt::<Option<usize>>("limit")?;
            let offset = opts.opt::<Option<usize>>("offset")?;
            let out = commands::output_buffer_get(state, pane_id, limit, offset)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "output_buffer_list" => {
            let out = commands::output_buffer_list(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "output_buffer_clear" => {
            let pane_id = opts.req::<String>("pane_id")?;
            let out = commands::output_buffer_clear(state, pane_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "get_pane_history" => {
            let pane_id = opts.req::<String>("pane_id")?;
            let out = commands::get_pane_history(state, pane_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plan_create" => {
            let goal = opts.req::<String>("goal")?;
            let reasoning = opts.req::<String>("reasoning")?;
            let steps = opts.req::<String>("steps")?;
            let out = commands::plan_create(state, goal, reasoning, steps)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plan_get" => {
            let out = commands::plan_get(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plan_update_step" => {
            let step_id = opts.req::<String>("step_id")?;
            let status = opts.req::<String>("status")?;
            let pane_id = opts.opt::<Option<String>>("pane_id")?;
            let out = commands::plan_update_step(state, step_id, status, pane_id)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_list" => {
            let out = commands::plugin_list(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_get" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            let out = commands::plugin_get(state, plugin_id).map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_register" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            let name = opts.req::<String>("name")?;
            let version = opts.req::<String>("version")?;
            let out = commands::plugin_register(state, plugin_id, name, version)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_unregister" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            commands::plugin_unregister(state, plugin_id)?;
            Ok(Value::Null)
        }
        "plugin_enable" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            commands::plugin_enable(state, plugin_id)?;
            Ok(Value::Null)
        }
        "plugin_disable" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            commands::plugin_disable(state, plugin_id)?;
            Ok(Value::Null)
        }
        "plugin_get_config" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            let out = commands::plugin_get_config(state, plugin_id).map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_set_config" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            let config = opts.req::<String>("config")?;
            commands::plugin_set_config(state, plugin_id, config)?;
            Ok(Value::Null)
        }
        "plugin_set_error" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            let error = opts.req::<String>("error")?;
            commands::plugin_set_error(state, plugin_id, error)?;
            Ok(Value::Null)
        }
        "plugin_host_list_sessions" => {
            let out = commands::plugin_host_list_sessions(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_host_get_session" => {
            let session_id = opts.req::<String>("session_id")?;
            let out = commands::plugin_host_get_session(state, session_id).map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_host_emit_event" => {
            let event_type = opts.req::<String>("event_type")?;
            let data = opts.req::<String>("data")?;
            let out = commands::plugin_host_emit_event(state, event_type, data)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_host_subscribe" => {
            let session_id = opts.req::<String>("session_id")?;
            let event_types = opts.req::<String>("event_types")?;
            commands::plugin_host_subscribe(state, session_id, event_types)?;
            Ok(Value::Null)
        }
        "plugin_host_update_status" => {
            let session_id = opts.req::<String>("session_id")?;
            let status = opts.req::<String>("status")?;
            commands::plugin_host_update_status(state, session_id, status)?;
            Ok(Value::Null)
        }
        "plugin_host_unregister_session" => {
            let session_id = opts.req::<String>("session_id")?;
            commands::plugin_host_unregister_session(state, session_id)?;
            Ok(Value::Null)
        }
        "plugin_host_discover_plugins" => {
            let dir = opts.req::<String>("dir")?;
            let out = commands::plugin_host_discover_plugins(state, dir)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_host_setup_plugin" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            let name = opts.req::<String>("name")?;
            let version = opts.req::<String>("version")?;
            let out = commands::plugin_host_setup_plugin(state, plugin_id, name, version)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "plugin_host_remove_plugin" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            commands::plugin_host_remove_plugin(state, plugin_id)?;
            Ok(Value::Null)
        }
        "pty_default_shell" => {
            let out = commands::pty_default_shell();
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_spawn" => {
            let id = opts.req::<String>("id")?;
            let cwd = opts.req::<String>("cwd")?;
            let shell = opts.req::<String>("shell")?;
            let cols = opts.opt::<Option<u16>>("cols")?;
            let rows = opts.opt::<Option<u16>>("rows")?;
            let start_paused = opts.opt::<Option<bool>>("start_paused")?;
            let listener_owner = opts.opt::<Option<String>>("listener_owner")?;
            commands::pty_spawn(
                state,
                id,
                cwd,
                shell,
                cols,
                rows,
                start_paused,
                listener_owner,
            )
            .await?;
            Ok(Value::Null)
        }
        "pty_write" => {
            let id = opts.req::<String>("id")?;
            let data = opts.req::<String>("data")?;
            commands::pty_write(state, id, data).await?;
            Ok(Value::Null)
        }
        "read_clipboard_text" => {
            let out = commands::read_clipboard_text(ctx.app_handle.clone()).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_kill" => {
            let id = opts.req::<String>("id")?;
            commands::pty_kill(state, id).await?;
            Ok(Value::Null)
        }
        "pty_resize" => {
            let id = opts.req::<String>("id")?;
            let cols = opts.req::<u16>("cols")?;
            let rows = opts.req::<u16>("rows")?;
            commands::pty_resize(state, id, cols, rows).await?;
            Ok(Value::Null)
        }
        "pty_get_history" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_get_history(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_has_session" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_has_session(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_is_ready" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_is_ready(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_get_cwd" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_get_cwd(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_agent_info" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_agent_info(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_foreground_process" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_foreground_process(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_spawn_agent" => {
            let id = opts.req::<String>("id")?;
            let cwd = opts.req::<String>("cwd")?;
            let shell = opts.req::<String>("shell")?;
            let agent_cmd = opts.req::<String>("agent_cmd")?;
            let cols = opts.opt::<Option<u16>>("cols")?;
            let rows = opts.opt::<Option<u16>>("rows")?;
            let start_paused = opts.opt::<Option<bool>>("start_paused")?;
            let listener_owner = opts.opt::<Option<String>>("listener_owner")?;
            commands::pty_spawn_agent(
                state,
                id,
                cwd,
                shell,
                agent_cmd,
                cols,
                rows,
                start_paused,
                listener_owner,
            )
            .await?;
            Ok(Value::Null)
        }
        "pty_set_xterm" => {
            let id = opts.req::<String>("id")?;
            let is_xterm = opts.req::<bool>("is_xterm")?;
            commands::pty_set_xterm(state, id, is_xterm).await?;
            Ok(Value::Null)
        }
        "pty_attach_listener" => {
            let id = opts.req::<String>("id")?;
            let owner = opts.req::<String>("owner")?;
            let replace_current = opts.opt::<Option<bool>>("replace_current")?;
            let out = commands::pty_attach_listener(state, id, owner, replace_current).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "pty_detach_listener" => {
            let id = opts.req::<String>("id")?;
            let owner = opts.req::<String>("owner")?;
            let generation = opts.req::<String>("generation")?;
            let out = commands::pty_detach_listener(state, id, owner, generation).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "search_code" => {
            let pattern = opts.req::<String>("pattern")?;
            let path = opts.req::<String>("path")?;
            let out = commands::search_code(state, pattern, path).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "search_ripgrep" => {
            let pattern = opts.req::<String>("pattern")?;
            let path = opts.req::<String>("path")?;
            let out = commands::search_ripgrep(state, pattern, path).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "session_create" => {
            let title = opts.opt::<Option<String>>("title")?;
            let out = commands::session_create(state, title).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "session_get" => {
            let id = opts.req::<String>("id")?;
            let out = commands::session_get(state, id).await.map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "session_list" => {
            let out = commands::session_list(state).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "session_delete" => {
            let id = opts.req::<String>("id")?;
            let out = commands::session_delete(state, id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "session_update" => {
            let id = opts.req::<String>("id")?;
            let title = opts.opt::<Option<String>>("title")?;
            let messages = opts.opt::<Option<String>>("messages")?;
            let out = commands::session_update(state, id, title, messages)
                .await
                .map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "session_add_message" => {
            let session_id = opts.req::<String>("session_id")?;
            let role = opts.req::<String>("role")?;
            let content = opts.req::<String>("content")?;
            let is_error = opts.opt::<Option<bool>>("is_error")?;
            let image_refs = opts.opt::<Option<String>>("image_refs")?;
            let out = commands::session_add_message(
                state, session_id, role, content, is_error, image_refs,
            )
            .await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "shell_integration_parse" => {
            let data = opts.req::<String>("data")?;
            let out = commands::shell_integration_parse(state, data).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "shell_integration_script" => {
            let shell = opts.req::<String>("shell")?;
            let out = commands::shell_integration_script(shell).map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "shell_integration_compatible" => {
            let shell = opts.req::<String>("shell")?;
            let out = commands::shell_integration_compatible(shell);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "shell_integration_strip" => {
            let data = opts.req::<String>("data")?;
            let out = commands::shell_integration_strip(data);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "store_get" => {
            let key = opts.req::<String>("key")?;
            if !mobile_store_key_allowed(&key) {
                return Err(format!("relay store key is not available: {key}"));
            }
            let out = commands::store_get(state, key).map_err(to_err)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "store_set" => {
            let key = opts.req::<String>("key")?;
            let value = opts.req::<String>("value")?;
            if !mobile_store_key_allowed(&key) {
                return Err(format!("relay store key is not available: {key}"));
            }
            commands::store_set(state, key, value)?;
            Ok(Value::Null)
        }
        "store_has" => {
            let key = opts.req::<String>("key")?;
            let out = commands::store_has(state, key);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "store_delete" => {
            let key = opts.req::<String>("key")?;
            if !mobile_store_key_allowed(&key) {
                return Err(format!("relay store key is not available: {key}"));
            }
            commands::store_delete(state, key)?;
            Ok(Value::Null)
        }
        "test_llm_api_key" => {
            let out = commands::test_llm_api_key(state)?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_create" => {
            let dir = opts.req::<String>("dir")?;
            let swarm_state = opts.req::<String>("swarm_state")?;
            let out = commands::swarm_create(state, dir, swarm_state).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_read_state" => {
            let dir = opts.req::<String>("dir")?;
            let out = commands::swarm_read_state(state, dir).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_start_watch" => {
            let dir = opts.req::<String>("dir")?;
            commands::swarm_start_watch(state, dir).await?;
            Ok(Value::Null)
        }
        "swarm_stop_watch" => {
            let dir = opts.req::<String>("dir")?;
            commands::swarm_stop_watch(state, dir)?;
            Ok(Value::Null)
        }
        "swarm_update_agent" => {
            let dir = opts.req::<String>("dir")?;
            let agent_id = opts.req::<String>("agent_id")?;
            let status = opts.opt::<Option<String>>("status")?;
            let last_action = opts.opt::<Option<String>>("last_action")?;
            let current_task = opts.opt::<Option<Option<String>>>("current_task")?;
            let out = commands::swarm_update_agent(
                state,
                dir,
                agent_id,
                status,
                last_action,
                current_task,
            )
            .await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_set_status" => {
            let dir = opts.req::<String>("dir")?;
            let status = opts.req::<String>("status")?;
            let out = commands::swarm_set_status(state, dir, status).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_create_task" => {
            let dir = opts.req::<String>("dir")?;
            let title = opts.req::<String>("title")?;
            let description = opts.req::<String>("description")?;
            let assigned_agent_id = opts.req::<String>("assigned_agent_id")?;
            let out =
                commands::swarm_create_task(state, dir, title, description, assigned_agent_id)
                    .await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_update_task" => {
            let dir = opts.req::<String>("dir")?;
            let task_id = opts.req::<String>("task_id")?;
            let status = opts.req::<String>("status")?;
            let out = commands::swarm_update_task(state, dir, task_id, status).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "swarm_send_message" => {
            let dir = opts.req::<String>("dir")?;
            let from = opts.req::<String>("from")?;
            let to = opts.req::<String>("to")?;
            let content = opts.req::<String>("content")?;
            commands::swarm_send_message(state, dir, from, to, content).await?;
            Ok(Value::Null)
        }
        "swarm_read_mailbox" => {
            let dir = opts.req::<String>("dir")?;
            let agent_id = opts.req::<String>("agent_id")?;
            let out = commands::swarm_read_mailbox(state, dir, agent_id).await?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "window_minimize" => {
            commands::window_minimize(ctx.app_handle.clone())?;
            Ok(Value::Null)
        }
        "window_maximize" => {
            commands::window_maximize(ctx.app_handle.clone())?;
            Ok(Value::Null)
        }
        "window_close" => {
            commands::window_close(ctx.app_handle.clone())?;
            Ok(Value::Null)
        }
        "window_is_maximized" => {
            let out = commands::window_is_maximized(ctx.app_handle.clone())?;
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "window_platform" => {
            let out = commands::window_platform();
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "workspace_add_trusted_root" => {
            let dir = opts.req::<String>("dir")?;
            commands::workspace_add_trusted_root(state, dir).await?;
            Ok(Value::Null)
        }
        "workspace_remove_trusted_root" => {
            let dir = opts.req::<String>("dir")?;
            commands::workspace_remove_trusted_root(state, dir).await?;
            Ok(Value::Null)
        }
        "workspace_list_trusted_roots" => {
            let out = commands::workspace_list_trusted_roots(state);
            Ok(serde_json::to_value(out).map_err(|e| e.to_string())?)
        }
        "output_buffer_append" => {
            let pane_id = opts.req::<String>("pane_id")?;
            let data = opts.req::<String>("data")?;
            let agent_type = opts.opt::<Option<String>>("agent_type")?;
            commands::output_buffer_append(state, pane_id, data, agent_type);
            Ok(Value::Null)
        }
        _ => Err(format!("unknown relay command: {cmd}")),
    }
}

/// Commands available to the mobile mirror. Mutating, secret-bearing, native
/// dialog, process, and window-control commands stay desktop-only even after
/// the relay token is presented.
pub fn command_allowed(cmd: &str) -> bool {
    matches!(
        cmd,
        "agents_list"
            | "agent_get_status"
            | "athena_chat"
            | "athena_chat_stream"
            | "athena_cancel_stream"
            | "fs_read_file"
            | "fs_write_file"
            | "pty_write"
            | "pty_spawn"
            | "pty_resize"
            | "pty_kill"
            | "pty_set_xterm"
            | "pty_attach_listener"
            | "pty_detach_listener"
            | "store_set"
            | "agent_comms_sessions"
            | "kanban_get_tasks"
            | "store_get"
            | "mcp_tools"
            | "notification_history"
            | "notification_count"
            | "notification_counts"
            | "notification_mark_read"
            | "notification_mark_all_read"
            | "notification_dismiss"
            | "output_buffer_get"
            | "output_buffer_list"
            | "get_pane_history"
            | "plan_get"
            | "plugin_list"
            | "plugin_get"
            | "plugin_host_list_sessions"
            | "pty_default_shell"
            | "pty_has_session"
            | "pty_is_ready"
            | "pty_agent_info"
            | "pty_foreground_process"
            | "session_list"
    )
}

/// Store values needed to boot/render the existing frontend. Keep this
/// explicit: allowing arbitrary `store_get` keys would expose secrets and
/// unrelated persisted state over the LAN.
fn mobile_store_key_allowed(key: &str) -> bool {
    matches!(
        key,
        "theme"
            | "font_family"
            | "font_size"
            | "custom_agents"
            | "smart_pane_titles"
            | "agent_notify_config"
            | "workspaces"
    )
}

/// Coerce any `Display` error (String, CommandError, …) to a plain `String`.
fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Helper wrapper around a `serde_json::Value::Object` for pulling named params.
pub struct Args {
    map: Map<String, Value>,
}

impl Args {
    fn new(value: Value) -> Self {
        match value {
            Value::Object(map) => Args { map },
            _ => Args { map: Map::new() },
        }
    }

    /// Required named parameter.
    pub fn req<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        let v = self
            .map
            .get(name)
            .ok_or_else(|| format!("missing required parameter '{name}'"))?;
        serde_json::from_value::<T>(v.clone())
            .map_err(|e| format!("invalid parameter '{name}': {e}"))
    }

    /// Optional named parameter, JSON null when omitted.
    pub fn opt<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        let v = self.map.get(name).cloned().unwrap_or(Value::Null);
        serde_json::from_value::<T>(v)
            .map_err(|e| format!("invalid optional parameter '{name}': {e}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mobile_allowlist_contains_required_companion_commands() {
        for command in [
            "store_get",
            "output_buffer_get",
            "pty_write",
            "pty_set_xterm",
            "pty_attach_listener",
            "athena_chat",
            "fs_read_file",
            "fs_write_file",
        ] {
            assert!(
                command_allowed(command),
                "expected mobile command: {command}"
            );
        }
    }

    #[test]
    fn mobile_allowlist_rejects_trust_root_commands() {
        // Trust-root authorization must originate on the desktop via the native
        // directory picker; exposing it over the relay would let any paired
        // phone bless arbitrary directories (e.g. ~/.ssh, ~/.aws) and then read
        // or write files there through the already-allowlisted fs_* commands.
        assert!(!command_allowed("workspace_add_trusted_root"));
        assert!(!command_allowed("workspace_remove_trusted_root"));
    }

    #[test]
    fn mobile_allowlist_rejects_store_delete() {
        // store_delete is not in command_allowed today, but the dispatch arm
        // also gates on mobile_store_key_allowed so the guard is defense-in-depth:
        // even if the command is ever allowlisted, arbitrary store keys remain
        // protected at the dispatch layer.
        assert!(!command_allowed("store_delete"));
    }

    #[test]
    fn mobile_allowlist_rejects_secrets_desktop_controls_and_browser() {
        // Browser child-WebView commands stay desktop-only. Exposing them over
        // the LAN would let a paired phone create arbitrary native webviews,
        // move them over the desktop window, or drive a different URL policy
        // than the local UI. The dispatch arms remain available for the desktop
        // relay plumbing, but this allowlist is the security gate.
        for command in [
            "store_api_key",
            "clear_api_key",
            "window_close",
            "fs_show_open_dialog",
            "plugin_set_config",
            "browser_show",
            "browser_hide",
            "browser_navigate",
            "browser_back",
            "browser_forward",
            "browser_reload",
            "browser_set_bounds",
        ] {
            assert!(
                !command_allowed(command),
                "unexpected mobile command: {command}"
            );
        }
    }

    #[test]
    fn store_allowlist_is_narrow() {
        for key in ["workspaces", "theme", "font_family"] {
            assert!(mobile_store_key_allowed(key));
        }
        for key in ["llm.api_key", "relay_token", "workspace.trusted_roots"] {
            assert!(!mobile_store_key_allowed(key));
        }
    }
}
