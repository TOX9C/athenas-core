//! Dispatch: map `{cmd, args}` WS messages to the real `#[tauri::command]`
//! implementations.
//!
//! The relay is a transparent L7 proxy. Each `invoke(cmd, args)` over the WS
//! deserialises `args` into the command's typed parameters, borrows
//! `State<'_, AppState>` from the app handle for the duration of the call, and
//! invokes the underlying function directly — the exact same code path the
//! desktop Tauri handler uses.
//!
//! The dispatch table intentionally contains one arm per allowlisted command.
//! `authorize_command` is the only security boundary; anything outside the
//! mirror contract falls through to `unknown relay command` instead of keeping
//! latent, untested privileged handlers alive.

use std::collections::HashSet;

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
        "agents_list" => {
            let out = commands::agents_list(state)?;
            json(out)
        }
        "agent_get_status" => {
            let agent_id = opts.req::<String>("agent_id")?;
            let out = commands::agent_get_status(state, agent_id).map_err(to_err)?;
            json(out)
        }
        "athena_chat" => {
            let message = opts.req::<String>("message")?;
            let out = commands::athena_chat(state, message).await?;
            json(out)
        }
        "athena_chat_stream" => {
            // camelCase wire keys — the bridge sends `sessionId`/`requestId`
            // for Tauri's desktop rename; the relay reads raw JSON.
            let message = opts.req::<String>("message")?;
            let session_id = opts
                .req::<String>("sessionId")
                .or_else(|_| opts.req::<String>("session_id"))?;
            let request_id = opts
                .req::<String>("requestId")
                .or_else(|_| opts.req::<String>("request_id"))?;
            let out = commands::athena_chat_stream(state, message, session_id, request_id).await?;
            json(out)
        }
        "athena_cancel_stream" => {
            let request_id = opts
                .req::<String>("requestId")
                .or_else(|_| opts.req::<String>("request_id"))?;
            let out = commands::athena_cancel_stream(state, request_id)?;
            json(out)
        }
        "fs_read_file" => {
            let path = opts.req::<String>("path")?;
            let out = commands::fs_read_file(state, path).await.map_err(to_err)?;
            json(out)
        }
        "pty_default_shell" => json(commands::pty_default_shell()),
        "pty_spawn" => {
            let id = opts.req::<String>("id")?;
            let cwd = opts.req::<String>("cwd")?;
            let shell = opts.req::<String>("shell")?;
            let cols = opts.opt::<Option<u16>>("cols")?;
            let rows = opts.opt::<Option<u16>>("rows")?;
            let start_paused = opts.opt::<Option<bool>>("startPaused")?;
            let listener_owner = opts.opt::<Option<String>>("listenerOwner")?;
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
        "pty_kill" => {
            let id = opts.req::<String>("id")?;
            commands::pty_kill(state, id).await?;
            Ok(Value::Null)
        }
        "pty_resize" => {
            let id = opts.req::<String>("id")?;
            let cols = opts.req::<u16>("cols")?;
            let rows = opts.req::<u16>("rows")?;
            let owner = opts.opt::<Option<String>>("owner")?;
            commands::pty_resize(state, id, cols, rows, owner).await?;
            Ok(Value::Null)
        }
        "pty_set_xterm" => {
            let id = opts.req::<String>("id")?;
            // The bridge sends camelCase `isXterm`; Tauri's rename only
            // applies on the desktop invoke path, while relay reads raw JSON.
            let is_xterm = opts.req::<bool>("isXterm")?;
            commands::pty_set_xterm(state, id, is_xterm).await?;
            Ok(Value::Null)
        }
        "pty_attach_listener" => {
            let id = opts.req::<String>("id")?;
            let owner = opts.req::<String>("owner")?;
            let replace_current = opts.opt::<Option<bool>>("replaceCurrent")?;
            let out =
                commands::pty_attach_listener_relay(state, id, owner, replace_current).await?;
            json(out)
        }
        "pty_detach_listener" => {
            let id = opts.req::<String>("id")?;
            let owner = opts.req::<String>("owner")?;
            let generation = opts.req::<String>("generation")?;
            let out = commands::pty_detach_listener(state, id, owner, generation).await?;
            json(out)
        }
        "pty_has_session" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_has_session(state, id).await?;
            json(out)
        }
        "pty_is_ready" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_is_ready(state, id).await?;
            json(out)
        }
        "pty_agent_info" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_agent_info(state, id).await?;
            json(out)
        }
        "pty_foreground_process" => {
            let id = opts.req::<String>("id")?;
            let out = commands::pty_foreground_process(state, id).await?;
            json(out)
        }
        "pty_raw_replay" => {
            let pane_id = opts.req::<String>("paneId")?;
            let out = commands::pty_raw_replay(state, pane_id).await?;
            json(out)
        }
        "store_get" => {
            let key = opts.req::<String>("key")?;
            if !mobile_store_key_allowed(&key) {
                return Err(format!("relay store key is not available: {key}"));
            }
            let out = commands::store_get(state, key).await.map_err(to_err)?;
            json(out)
        }
        "store_set" => {
            let key = opts.req::<String>("key")?;
            let value = opts.req::<String>("value")?;
            if !mobile_store_key_allowed(&key) {
                return Err(format!("relay store key is not available: {key}"));
            }
            commands::store_set(app.clone(), state, key, value).await?;
            Ok(Value::Null)
        }
        "agent_comms_sessions" => {
            let out = commands::agent_comms_sessions(state)?;
            json(out)
        }
        "kanban_get_tasks" => {
            let out = commands::kanban_get_tasks(state).await?;
            json(out)
        }
        "mcp_tools" => {
            let out = commands::mcp_tools()?;
            json(out)
        }
        "notification_history" => {
            let limit = opts.opt::<Option<usize>>("limit")?;
            let out = commands::notification_history(state, limit)?;
            json(out)
        }
        "notification_count" => json(commands::notification_count(state)?),
        "notification_counts" => json(commands::notification_counts(state)?),
        "notification_mark_read" => {
            let notification_id = opts
                .req::<String>("notificationId")
                .or_else(|_| opts.req::<String>("id"))
                .or_else(|_| opts.req::<String>("notification_id"))?;
            json(commands::notification_mark_read(state, notification_id)?)
        }
        "notification_mark_all_read" => json(commands::notification_mark_all_read(state)),
        "notification_dismiss" => {
            let notification_id = opts
                .req::<String>("notificationId")
                .or_else(|_| opts.req::<String>("id"))
                .or_else(|_| opts.req::<String>("notification_id"))?;
            json(commands::notification_dismiss(state, notification_id)?)
        }
        "notification_resolve" => {
            let notification_id = opts
                .req::<String>("notificationId")
                .or_else(|_| opts.req::<String>("id"))
                .or_else(|_| opts.req::<String>("notification_id"))?;
            json(commands::notification_resolve(state, notification_id)?)
        }
        "output_buffer_get" => {
            let pane_id = opts.req::<String>("paneId")?;
            let limit = opts.opt::<Option<usize>>("limit")?;
            let offset = opts.opt::<Option<usize>>("offset")?;
            json(commands::output_buffer_get(state, pane_id, limit, offset)?)
        }
        "output_buffer_list" => json(commands::output_buffer_list(state)?),
        // Formerly `get_pane_history` (unbounded full-buffer read); the
        // relay surface now routes through the paginated command.
        "get_pane_history" => {
            let pane_id = opts.req::<String>("paneId")?;
            let limit = opts.opt::<Option<usize>>("limit")?;
            json(commands::output_buffer_get(state, pane_id, limit, None)?)
        }
        "plan_get" => json(commands::plan_get(state)?),
        "plugin_list" => json(commands::plugin_list(state)?),
        "plugin_get" => {
            let plugin_id = opts.req::<String>("plugin_id")?;
            json(commands::plugin_get(state, plugin_id).map_err(to_err)?)
        }
        "plugin_host_list_sessions" => json(commands::plugin_host_list_sessions(state)?),
        "session_list" => {
            let out = commands::session_list(state).await?;
            json(out)
        }
        "session_create" => {
            let title = opts.opt::<Option<String>>("title")?;
            let out = commands::session_create(state, title).await?;
            json(out)
        }
        "session_get" => {
            let id = opts.req::<String>("id")?;
            let out = commands::session_get(state, id).await.map_err(to_err)?;
            json(out)
        }
        "relay_request_pane_share" => {
            let pane_id = opts.req::<String>("paneId")?;
            commands::relay_request_pane_share(state, pane_id)?;
            Ok(Value::Null)
        }
        _ => Err(format!("unknown relay command: {cmd}")),
    }
}

/// Commands whose responses take a full model turn (or otherwise run long).
/// The relay runs these as background tasks so a stalled provider cannot
/// freeze every later invoke on the phone's socket (see ws.rs session_loop).
pub const BACKGROUND_COMMANDS: &[&str] = &["athena_chat", "athena_chat_stream"];

/// Commands available to the mobile mirror.
///
/// Threat model: the mirror is an experimental plaintext LAN feature behind a
/// per-process token plus desktop pairing approval, so this gate is kept
/// read-oriented with the minimum interactive-terminal mutations the phone UI
/// needs. In particular, arbitrary filesystem writes and agent launch are not
/// relay commands; terminal content access additionally requires pane
/// ownership or an explicit desktop pane share.
pub fn command_allowed(cmd: &str) -> bool {
    matches!(
        cmd,
        "agents_list"
            | "agent_get_status"
            | "athena_chat"
            | "athena_chat_stream"
            | "athena_cancel_stream"
            | "fs_read_file"
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
            | "notification_resolve"
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
            | "pty_raw_replay"
            | "session_list"
            | "session_create"
            | "session_get"
            | "relay_request_pane_share"
    )
}

/// Pane-scoped commands the relay gates to panes the desktop has shared (or
/// the phone spawned itself). These read or mutate a specific pane's terminal
/// content/stream, so the per-pane share toggle is their authorization
/// boundary — the token alone is not sufficient for them.
///
/// Status-only queries (`pty_has_session`, `pty_is_ready`, `pty_agent_info`,
/// `pty_foreground_process`) remain ungated: they expose existence/process
/// metadata but no terminal content, and the mobile attach flow prechecks them.
pub fn pane_scoped_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "pty_write"
            | "pty_resize"
            | "pty_kill"
            | "pty_set_xterm"
            | "pty_attach_listener"
            | "pty_detach_listener"
            | "output_buffer_get"
            | "get_pane_history"
            | "pty_raw_replay"
    )
}

/// Extract the pane id a command operates on, using the wire key that command
/// expects. Most PTY commands use `id`; output/replay readers use `paneId`.
pub fn pane_id_of(cmd: &str, args: &serde_json::Value) -> Option<String> {
    let key = match cmd {
        "output_buffer_get" | "get_pane_history" | "pty_raw_replay" => "paneId",
        _ => "id",
    };
    args.get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
}

/// Decide whether a command + args may proceed on the relay, given the panes
/// this connection spawned (`owned`) and a live check against the panes the
/// desktop shared (`shared`). This is the single authorization boundary
/// applied to every relay invoke frame; there is deliberately no `mobile-`
/// prefix exemption. `shared` is a predicate rather than a set so callers on
/// the hot path can test membership under the state lock without cloning the
/// whole set per invoke.
pub fn authorize_command(
    cmd: &str,
    args: &serde_json::Value,
    owned: &HashSet<String>,
    shared: impl Fn(&str) -> bool,
) -> Result<(), String> {
    if !command_allowed(cmd) {
        return Err(format!(
            "relay command not available in read-oriented mirror mode: {cmd}"
        ));
    }
    if pane_scoped_command(cmd) {
        let pane_id = pane_id_of(cmd, args);
        let authorized = pane_id
            .as_deref()
            .is_some_and(|pane| owned.contains(pane) || shared(pane));
        if !authorized {
            return Err(format!(
                "relay pane is not shared: {}",
                pane_id.as_deref().unwrap_or("<missing pane id>")
            ));
        }
    }
    Ok(())
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

/// JSON-serialize a command result.
fn json<T: serde::Serialize>(value: T) -> Result<Value, String> {
    serde_json::to_value(value).map_err(|e| e.to_string())
}

/// Coerce any `Display` error (String, CommandError, …) to a plain `String`.
fn to_err<E: std::fmt::Display>(e: E) -> String {
    e.to_string()
}

/// Helper wrapper around a `serde_json::Value::Object` for pulling named params.
struct Args {
    map: Map<String, Value>,
}

impl Args {
    fn new(value: Value) -> Self {
        match value {
            Value::Object(map) => Args { map },
            _ => Args { map: Map::new() },
        }
    }

    /// Required named parameter. Deserializes through `&Value` (which
    /// implements `Deserializer`) so strings/numbers are borrowed from the
    /// already-parsed frame instead of cloned per parameter.
    fn req<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        let v = self
            .map
            .get(name)
            .ok_or_else(|| format!("missing required parameter '{name}'"))?;
        serde::Deserialize::deserialize(v)
            .map_err(|e: serde_json::Error| format!("invalid parameter '{name}': {e}"))
    }

    /// Optional named parameter, JSON null when omitted.
    fn opt<T: serde::de::DeserializeOwned>(&self, name: &str) -> Result<T, String> {
        static NULL: Value = Value::Null;
        let v = self.map.get(name).unwrap_or(&NULL);
        serde::Deserialize::deserialize(v)
            .map_err(|e: serde_json::Error| format!("invalid optional parameter '{name}': {e}"))
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
            "athena_chat_stream",
            "fs_read_file",
            "relay_request_pane_share",
        ] {
            assert!(
                command_allowed(command),
                "expected mobile command: {command}"
            );
        }
    }

    #[test]
    fn mobile_allowlist_rejects_writes_and_process_launch_beyond_owner_terminal() {
        // The mirror intentionally remains read-oriented outside the phone's
        // own explicitly created terminal. Arbitrary fs writes and historical
        // PTY agent launch paths are the remote-code-execution surface dropped
        // from the plaintext LAN gate.
        for command in [
            "fs_write_file",
            "pty_spawn_agent",
            "pty_get_history",
            "workspace_add_trusted_root",
            "workspace_remove_trusted_root",
        ] {
            assert!(
                !command_allowed(command),
                "unexpected relay command: {command}"
            );
        }
    }

    #[test]
    fn mobile_allowlist_rejects_secrets_desktop_controls_and_browser() {
        for command in [
            "store_api_key",
            "clear_api_key",
            "window_close",
            "fs_show_open_dialog",
            "plugin_set_config",
            "browser_show",
            "browser_hide",
            "browser_navigate",
            "read_clipboard_text",
        ] {
            assert!(
                !command_allowed(command),
                "unexpected mobile command: {command}"
            );
        }
    }

    #[test]
    fn mobile_allowlist_rejects_arbitrary_store_reads_and_deletes() {
        assert!(!command_allowed("store_delete"));
        assert!(!command_allowed("store_has"));
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

    fn owned(pane: &str) -> HashSet<String> {
        HashSet::from([pane.to_string()])
    }

    #[test]
    fn authorize_rejects_non_allowlisted_commands() {
        let owned: HashSet<String> = HashSet::new();
        let shared: HashSet<String> = HashSet::new();
        assert!(
            authorize_command("window_close", &serde_json::json!({}), &owned, |p| shared.contains(p)).is_err()
        );
        assert!(authorize_command(
            "store_api_key",
            &serde_json::json!({ "key": "x" }),
            &owned,
            |p| shared.contains(p)
            )
        .is_err());
    }

    #[test]
    fn authorize_rejects_pane_scoped_command_on_unowned_unshared_pane() {
        let owned = owned("pane-a");
        let shared = HashSet::from(["pane-b".to_string()]);
        let args = serde_json::json!({ "id": "pane-c", "data": "ls\n" });
        assert!(authorize_command("pty_write", &args, &owned, |p| shared.contains(p)).is_err());
    }

    #[test]
    fn authorize_rejects_pane_scoped_command_with_missing_pane_id() {
        let owned: HashSet<String> = HashSet::new();
        let shared: HashSet<String> = HashSet::new();
        assert!(authorize_command(
            "pty_write",
            &serde_json::json!({ "data": "ls\n" }),
            &owned,
            |p| shared.contains(p)
            )
        .is_err());
    }

    #[test]
    fn authorize_allows_pane_scoped_command_on_owned_pane() {
        let owned = owned("pane-a");
        let shared: HashSet<String> = HashSet::new();
        let args = serde_json::json!({ "id": "pane-a", "data": "ls\n" });
        assert!(authorize_command("pty_write", &args, &owned, |p| shared.contains(p)).is_ok());
    }

    #[test]
    fn authorize_allows_pane_scoped_command_on_shared_pane() {
        let owned: HashSet<String> = HashSet::new();
        let shared = HashSet::from(["pane-b".to_string()]);
        let args = serde_json::json!({ "id": "pane-b", "data": "ls\n" });
        assert!(authorize_command("pty_write", &args, &owned, |p| shared.contains(p)).is_ok());
    }

    #[test]
    fn mobile_prefix_has_no_authorization_meaning() {
        // Ownership is per connection. A phone that reconnects must own the
        // pane again or ask for a desktop share; the predictable `mobile-`
        // naming convention must not let another paired phone claim it.
        let owned: HashSet<String> = HashSet::new();
        let shared: HashSet<String> = HashSet::new();
        let args = serde_json::json!({ "id": "mobile-abc123", "data": "ls\n" });
        assert!(authorize_command("pty_write", &args, &owned, |p| shared.contains(p)).is_err());
        let owned = HashSet::from(["mobile-abc123".to_string()]);
        assert!(authorize_command("pty_write", &args, &owned, |p| shared.contains(p)).is_ok());
    }

    #[test]
    fn authorize_allows_non_pane_scoped_allowlisted_commands() {
        let owned: HashSet<String> = HashSet::new();
        let shared: HashSet<String> = HashSet::new();
        assert!(
            authorize_command("pty_default_shell", &serde_json::json!({}), &owned, |p| shared.contains(p)).is_ok()
        );
        assert!(authorize_command(
            "athena_chat",
            &serde_json::json!({ "message": "hi" }),
            &owned,
            |p| shared.contains(p)
            )
        .is_ok());
    }

    #[test]
    fn pane_id_of_uses_the_command_wire_key() {
        assert_eq!(
            pane_id_of("pty_write", &serde_json::json!({ "id": "  pane-a  " })),
            Some("pane-a".to_string())
        );
        assert_eq!(
            pane_id_of(
                "output_buffer_get",
                &serde_json::json!({ "paneId": "  pane-b  " })
            ),
            Some("pane-b".to_string())
        );
        assert_eq!(
            pane_id_of("pty_raw_replay", &serde_json::json!({ "paneId": "pane-c" })),
            Some("pane-c".to_string())
        );
        assert_eq!(pane_id_of("pty_write", &serde_json::json!({})), None);
    }

    #[test]
    fn authorize_allows_raw_replay_only_for_owned_or_shared_panes() {
        let owned = owned("pane-a");
        let shared = HashSet::from(["pane-b".to_string()]);
        assert!(authorize_command(
            "pty_raw_replay",
            &serde_json::json!({ "paneId": "pane-a" }),
            &owned,
            |p| shared.contains(p)
            )
        .is_ok());
        assert!(authorize_command(
            "pty_raw_replay",
            &serde_json::json!({ "paneId": "pane-b" }),
            &owned,
            |p| shared.contains(p)
            )
        .is_ok());
        assert!(authorize_command(
            "pty_raw_replay",
            &serde_json::json!({ "paneId": "pane-c" }),
            &owned,
            |p| shared.contains(p)
            )
        .is_err());
        assert!(
            authorize_command("pty_raw_replay", &serde_json::json!({}), &owned, |p| shared.contains(p)).is_err()
        );
    }
}
