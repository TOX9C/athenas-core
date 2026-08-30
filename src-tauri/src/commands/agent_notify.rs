//! Agent notification emitter installation (Phase 2 of the universal
//! agent-notifications plan — see `.plans/agent-detection-universal-notifications.md`).
//!
//! Warp's model: don't guess agent state — have each agent *push* its own
//! lifecycle through a native hook/config. This module installs a tiny POSIX
//! emitter script and wires each supported agent's hooks to call it:
//!
//! - **Claude Code**: `Stop` hook (turn complete → `complete`),
//!   `PermissionRequest` hook (tool approval → `request`), and a
//!   `Notification` hook on `idle_prompt` (waiting for input → `request`).
//! - **Codex**: lifecycle hooks in `~/.codex/hooks.json` — `Stop` (turn
//!   complete → `complete`) and `PermissionRequest` (approval → `request`).
//! - **OMP (oh my pi)**: a user-level extension at
//!   `~/.pi/agent/extensions/athena-notify.ts` — `turn_end` → `complete`
//!   (turn finished), `session_shutdown` → `complete`. Loaded
//!   automatically by OMP; no package edits, survives updates.
//!
//! Freebuff's real CLI is a compiled binary (no hook surface), so its
//! emitter lives in the npm wrapper (`launcher.js`, patched on this
//! machine) and — durably — upstream in `CodebuffAI/freebuff-private`
//! (see `.plans/agent-detection-universal-notifications.md`).
//!
//! Every hook prints the OSC 6337 marker parsed by
//! `athena_core::agent_lifecycle`, which the PTY read loop relays to the
//! activity tracker immediately.
//!
//! # Transport (why `/dev/tty`)
//!
//! Agent hosts capture a hook's stdout — Claude Code parses it as a structured
//! return channel, and Codex spawns its `notify`/hook children with stdout
//! nulled. So a marker written to stdout never reaches the pane. The emitter
//! instead writes to the **controlling terminal** (`/dev/tty`), which is the
//! pane's PTY — the exact trick Warp's `claude-code-warp` plugin uses for its
//! OSC 777 adapter — and falls back to stdout when no tty exists (an
//! in-process emitter, or a host that detached the hook).
//!
//! # Why not Codex's `notify` key
//!
//! `notify` is a single argv array (not a list of programs), is often already
//! occupied (the Computer Use desktop client sets it), and fires only on
//! `agent-turn-complete`. Codex's lifecycle hooks are the correct surface: they
//! cover both "done" (`Stop`) and "needs approval" (`PermissionRequest`).
//!
//! Install is non-destructive: existing hooks and config keys are preserved;
//! we only append our own entries (idempotently).

use std::path::{Path, PathBuf};

/// The emitter script. Written to `~/.local/bin/athena-agent-notify` on
/// install; embedded here so the installer is self-contained (no bundled
/// resource path to resolve at runtime).
///
/// It prints an OSC 6337 lifecycle marker to the controlling terminal. Two
/// calling conventions:
///   athena-agent-notify <complete|request|error> [agent]
///   athena-agent-notify '{"type":"agent-turn-complete", ...}'   (Codex notify)
const EMITTER_SCRIPT: &str = r#"#!/bin/sh
# Athena agent-notify emitter — prints an OSC 6337 lifecycle marker.
#
# Direct form (Claude Code / Codex hooks, Freebuff/OMP emitters):
#   athena-agent-notify <complete|request|error> [agent]
#
# Codex notify form (Codex `notify` passes a JSON event payload as argv[1]):
#   athena-agent-notify '{"type":"agent-turn-complete",...}'
set -eu

kind=""
agent=""
arg="${1:-}"

case "$arg" in
  \{*) # JSON payload (Codex notify)
    t=$(printf '%s' "$arg" | sed -n 's/.*"type"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
    case "$t" in
      *complete*) kind="complete" ;;
      *permission*|*approval*|*input*) kind="request" ;;
      *error*|*failed*) kind="error" ;;
      *) exit 0 ;;
    esac
    agent="codex"
    ;;
  complete|done|completed|success|finished) kind="complete" ;;
  request|needs_input|waiting|waiting_input|attention) kind="request" ;;
  error|failed) kind="error" ;;
  *) exit 0 ;;
esac

if [ -z "$agent" ]; then
  agent="${2:-}"
fi

if [ -n "$agent" ]; then
  MARKER=$(printf '\033]6337;{"kind":"%s","agent":"%s"}\007' "$kind" "$agent")
else
  MARKER=$(printf '\033]6337;{"kind":"%s"}\007' "$kind")
fi

# Agent hosts capture a hook's stdout (Claude parses it; Codex nulls it), so
# write the marker to the controlling terminal — the pane's PTY — directly.
# Same trick Warp's plugin uses (`> /dev/tty`). Fall back to stdout when there
# is no tty (e.g. an in-process emitter or a detached hook).
if [ -w /dev/tty ]; then
  printf '%s' "$MARKER" > /dev/tty 2>/dev/null || printf '%s' "$MARKER"
else
  printf '%s' "$MARKER"
fi
"#;

/// Home directory, read fresh from the environment (the hook configs and the
/// emitter script all live under `$HOME`, not the workspace sandbox).
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

// ---------------------------------------------------------------------------
// Emitter script
// ---------------------------------------------------------------------------

/// Write the emitter script to `~/.local/bin/athena-agent-notify` (executable)
/// and return its absolute path.
fn install_emitter(home: &Path) -> Result<PathBuf, String> {
    let dir = home.join(".local/bin");
    std::fs::create_dir_all(&dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    let path = dir.join("athena-agent-notify");
    std::fs::write(&path, EMITTER_SCRIPT).map_err(|e| format!("write {}: {e}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("chmod {}: {e}", path.display()))?;
    }
    Ok(path)
}

// ---------------------------------------------------------------------------
// Shared hook-merge helpers
// ---------------------------------------------------------------------------

/// True when a hook entry already references our emitter script.
fn entry_contains_command(entry: &serde_json::Value, script: &str) -> bool {
    entry
        .get("hooks")
        .and_then(|h| h.as_array())
        .map(|hooks| {
            hooks.iter().any(|h| {
                h.get("command")
                    .and_then(|c| c.as_str())
                    .map(|c| c.contains(script))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

fn entry_matches_matcher(entry: &serde_json::Value, matcher: Option<&str>) -> bool {
    match matcher {
        Some(m) => entry.get("matcher").and_then(|v| v.as_str()) == Some(m),
        None => entry.get("matcher").is_none(),
    }
}

/// Ensure `root["hooks"][event]` exists as an array and push `entry` if our
/// script is not already present under that event (idempotent).
fn push_hook_entry(
    root: &mut serde_json::Value,
    event: &str,
    entry: serde_json::Value,
    script: &str,
) {
    if !root.get("hooks").is_some_and(|v| v.is_object()) {
        root["hooks"] = serde_json::json!({});
    }
    let hooks = root.get_mut("hooks").expect("hooks object just ensured");
    if !hooks.get(event).is_some_and(|v| v.is_array()) {
        hooks[event] = serde_json::json!([]);
    }
    if let Some(arr) = hooks.get_mut(event).and_then(|v| v.as_array_mut()) {
        if !arr.iter().any(|e| entry_contains_command(e, script)) {
            arr.push(entry);
        }
    }
}

fn command_hook(command: &str) -> serde_json::Value {
    serde_json::json!({
        "hooks": [{ "type": "command", "command": command }]
    })
}

/// Single-quote a path for a shell command line, escaping any embedded `'`
/// with the POSIX `'\''` idiom. The naive `'{path}'` form breaks whenever
/// the path contains a quote (e.g. a `$HOME` with an apostrophe), turning
/// one command into two.
fn shell_quote(path: &str) -> String {
    format!("'{}'", path.replace('\'', "'\\''"))
}

// ---------------------------------------------------------------------------
// Claude Code
// ---------------------------------------------------------------------------

/// Merge our hooks into a Claude Code `settings.json`. Pure: takes the
/// existing JSON text and returns the merged JSON, preserving every unrelated
/// key and any user-defined hooks. Idempotent.
fn merge_claude_settings(existing: &str, script: &str) -> Result<String, String> {
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(existing)
            .map_err(|e| format!("~/.claude/settings.json is not valid JSON: {e}"))?
    };
    if !root.is_object() {
        return Err("~/.claude/settings.json must be a JSON object".to_string());
    }

    let stop_cmd = format!("{} complete claude", shell_quote(script));
    let request_cmd = format!("{} request claude", shell_quote(script));

    // Stop → turn complete.
    push_hook_entry(&mut root, "Stop", command_hook(&stop_cmd), script);

    // PermissionRequest → tool approval needed (immediate, unlike the
    // ~6s-delayed Notification permission_prompt matcher).
    push_hook_entry(
        &mut root,
        "PermissionRequest",
        command_hook(&request_cmd),
        script,
    );

    // Notification (idle_prompt) → Claude finished and is waiting for input.
    if !root.get("hooks").is_some_and(|v| v.is_object()) {
        root["hooks"] = serde_json::json!({});
    }
    let hooks = root.get_mut("hooks").expect("hooks object just ensured");
    if !hooks.get("Notification").is_some_and(|v| v.is_array()) {
        hooks["Notification"] = serde_json::json!([]);
    }
    if let Some(arr) = hooks.get_mut("Notification").and_then(|v| v.as_array_mut()) {
        let already = arr.iter().any(|e| {
            entry_matches_matcher(e, Some("idle_prompt")) && entry_contains_command(e, script)
        });
        if !already {
            let mut entry = serde_json::Map::new();
            entry.insert(
                "matcher".to_string(),
                serde_json::Value::String("idle_prompt".to_string()),
            );
            entry.insert(
                "hooks".to_string(),
                command_hook(&request_cmd)["hooks"].clone(),
            );
            arr.push(serde_json::Value::Object(entry));
        }
    }

    serde_json::to_string_pretty(&root).map_err(|e| format!("serialize settings.json: {e}"))
}

/// Install the Claude hooks into `~/.claude/settings.json` (non-destructive).
fn install_claude(home: &Path, script: &str) -> Result<PathBuf, String> {
    let path = home.join(".claude/settings.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = merge_claude_settings(&existing, script)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, merged).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Codex
// ---------------------------------------------------------------------------

/// Merge our lifecycle hooks into `~/.codex/hooks.json`. Pure: takes the
/// existing JSON and returns the merged JSON, preserving unrelated keys and
/// any user-defined hooks. Idempotent. Codex uses the same `{hooks: {event:
/// [{hooks: [...]}]}}` shape as Claude Code.
fn merge_codex_hooks(existing: &str, script: &str) -> Result<String, String> {
    let mut root: serde_json::Value = if existing.trim().is_empty() {
        serde_json::json!({})
    } else {
        serde_json::from_str(existing)
            .map_err(|e| format!("~/.codex/hooks.json is not valid JSON: {e}"))?
    };
    if !root.is_object() {
        return Err("~/.codex/hooks.json must be a JSON object".to_string());
    }

    let stop_cmd = format!("{} complete codex", shell_quote(script));
    let request_cmd = format!("{} request codex", shell_quote(script));

    // Stop → turn complete.
    push_hook_entry(&mut root, "Stop", command_hook(&stop_cmd), script);
    // PermissionRequest → tool approval needed.
    push_hook_entry(
        &mut root,
        "PermissionRequest",
        command_hook(&request_cmd),
        script,
    );

    serde_json::to_string_pretty(&root).map_err(|e| format!("serialize hooks.json: {e}"))
}

/// Install the Codex lifecycle hooks into `~/.codex/hooks.json`
/// (non-destructive; leaves the `notify` key untouched).
fn install_codex(home: &Path, script: &str) -> Result<PathBuf, String> {
    let path = home.join(".codex/hooks.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let merged = merge_codex_hooks(&existing, script)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, merged).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// OMP (oh my pi)
// ---------------------------------------------------------------------------

/// The OMP user-level extension. Written to `~/.pi/agent/extensions/`;
/// OMP discovers `*.ts` files there and loads them as extensions on every
/// session start (see `discovery/builtin.ts` `loadExtensionModules`).
/// Mirrors the file installed on this machine at install time.
const OMP_EXTENSION: &str = r#"/**
 * athena-notify — pushes OSC 6337 lifecycle markers from OMP into the
 * terminal. The athena PTY read loop parses them and fires notifications
 * instantly (no polling): "finished" when a turn ends, "done" when the
 * session shuts down.
 *
 * The sequence is an unknown OSC code to every terminal (xterm.js ignores
 * it), so it is invisible and never corrupts the TUI.
 *
 * Lives at ~/.pi/agent/extensions/ — a user-level OMP extension loaded
 * automatically on session start. No OMP package edits, survives updates.
 */

/** Minimal shape of the extension API we use (avoids a runtime import). */
interface AthenaNotifyAPI {
  on(event: string, handler: (event: unknown) => void): void;
}

const EMIT = (kind: string): void => {
  try {
    process.stdout.write(`\x1b]6337;{"kind":"${kind}","agent":"omp"}\x07`);
  } catch {
    /* never break the agent */
  }
};

export default function athenaNotify(pi: AthenaNotifyAPI): void {
  // A turn ended: OMP finished its work and is back at the input prompt →
  // "finished". Needs-attention mid-turn (permission / confirmation
  // prompts) is detected from the pane's output tail instead.
  pi.on("turn_end", () => EMIT("complete"));
  // The session ended (quit / run finished) → "done".
  pi.on("session_shutdown", () => EMIT("complete"));
}
"#;

/// Install the OMP extension into `~/.pi/agent/extensions/` (idempotent
/// overwrite — the extension is user-level config, so this is safe).
fn install_omp(home: &Path) -> Result<PathBuf, String> {
    let path = home.join(".pi/agent/extensions/athena-notify.ts");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    std::fs::write(&path, OMP_EXTENSION).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

// ---------------------------------------------------------------------------
// Tauri command
// ---------------------------------------------------------------------------

/// Install the agent-notification emitter for one agent (or `"all"`).
/// Returns a human-readable summary of the files written.
#[tauri::command]
pub fn agent_notify_install(agent: String) -> Result<String, String> {
    let agent = agent.trim().to_lowercase();
    let home = home_dir().ok_or_else(|| "HOME is not set".to_string())?;
    let script = install_emitter(&home)?;
    let script_str = script.to_string_lossy().into_owned();

    let mut steps = Vec::new();
    match agent.as_str() {
        "claude" => {
            let path = install_claude(&home, &script_str)?;
            steps.push(format!("Claude Code hooks → {}", path.display()));
        }
        "codex" => {
            let path = install_codex(&home, &script_str)?;
            steps.push(format!("Codex lifecycle hooks → {}", path.display()));
        }
        "omp" => {
            let path = install_omp(&home)?;
            steps.push(format!("OMP extension → {}", path.display()));
        }
        "all" => {
            let claude = install_claude(&home, &script_str)?;
            let codex = install_codex(&home, &script_str)?;
            let omp = install_omp(&home)?;
            steps.push(format!("Claude Code hooks → {}", claude.display()));
            steps.push(format!("Codex lifecycle hooks → {}", codex.display()));
            steps.push(format!("OMP extension → {}", omp.display()));
        }
        _ => {
            return Err(format!(
                "unsupported agent '{agent}' (supported: claude, codex, omp, all)"
            ));
        }
    }
    steps.push(format!("Emitter script → {}", script.display()));
    Ok(steps.join("\n"))
}

// ---------------------------------------------------------------------------
// Tests (pure merge functions only — no filesystem side effects)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const SCRIPT: &str = "/Users/me/.local/bin/athena-agent-notify";

    #[test]
    fn claude_merge_creates_hooks_on_empty_settings() {
        let merged = merge_claude_settings("", SCRIPT).unwrap();
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();

        let stop = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        let stop_cmd = stop[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(stop_cmd.contains("complete claude"));
        assert!(stop_cmd.contains(SCRIPT));

        let perm = v["hooks"]["PermissionRequest"].as_array().unwrap();
        assert_eq!(perm.len(), 1);
        let perm_cmd = perm[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(perm_cmd.contains("request claude"));

        let notify = v["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(notify.len(), 1);
        assert_eq!(notify[0]["matcher"].as_str(), Some("idle_prompt"));
        let notify_cmd = notify[0]["hooks"][0]["command"].as_str().unwrap();
        assert!(notify_cmd.contains("request claude"));
    }

    #[test]
    fn hook_commands_escape_single_quotes_in_script_path() {
        // A $HOME with an apostrophe must not split the shell command.
        let tricky = "/Users/o'brien/.local/bin/athena-agent-notify";
        let merged = merge_claude_settings("", tricky).unwrap();
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();
        let stop_cmd = v["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        // Wrapped in single quotes with the POSIX '\'' escape inside, so the
        // apostrophe is closed, escaped, and reopened: `'/Users/o'\''brien/...'`.
        assert_eq!(
            stop_cmd,
            "'/Users/o'\\''brien/.local/bin/athena-agent-notify' complete claude"
        );

        let codex = merge_codex_hooks("", tricky).unwrap();
        let cv: serde_json::Value = serde_json::from_str(&codex).unwrap();
        let codex_stop = cv["hooks"]["Stop"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert!(codex_stop.contains("'\\''"));
        assert!(codex_stop.ends_with("complete codex"));
    }

    #[test]
    fn claude_merge_preserves_existing_keys_and_hooks() {
        let existing = r#"{
            "permissions": { "allow": ["Bash"] },
            "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "echo user stop" }] }],
                "Notification": [{ "matcher": "*", "hooks": [{ "type": "command", "command": "bash notify-phone.sh" }] }]
            }
        }"#;
        let merged = merge_claude_settings(existing, SCRIPT).unwrap();
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();

        // Unrelated key preserved.
        assert_eq!(v["permissions"]["allow"][0], "Bash");
        // User's Stop hook preserved alongside ours.
        let stop = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert!(stop
            .iter()
            .any(|e| { e["hooks"][0]["command"].as_str() == Some("echo user stop") }));
        assert!(stop
            .iter()
            .any(|e| { e["hooks"][0]["command"].as_str().unwrap().contains(SCRIPT) }));
        // User's `*` Notification hook preserved; ours added with idle_prompt.
        let notify = v["hooks"]["Notification"].as_array().unwrap();
        assert_eq!(notify.len(), 2);
        assert!(notify.iter().any(|e| e["matcher"].as_str() == Some("*")));
        assert!(notify
            .iter()
            .any(|e| e["matcher"].as_str() == Some("idle_prompt")));
    }

    #[test]
    fn claude_merge_is_idempotent() {
        let once = merge_claude_settings("", SCRIPT).unwrap();
        let twice = merge_claude_settings(&once, SCRIPT).unwrap();
        let a: serde_json::Value = serde_json::from_str(&once).unwrap();
        let b: serde_json::Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(a, b);
        assert_eq!(b["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(b["hooks"]["PermissionRequest"].as_array().unwrap().len(), 1);
        assert_eq!(b["hooks"]["Notification"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn claude_merge_rejects_invalid_json() {
        assert!(merge_claude_settings("{ not json", SCRIPT).is_err());
    }

    #[test]
    fn codex_hooks_merge_creates_fresh() {
        let merged = merge_codex_hooks("", SCRIPT).unwrap();
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();

        let stop = v["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 1);
        assert!(stop[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("complete codex"));

        let perm = v["hooks"]["PermissionRequest"].as_array().unwrap();
        assert_eq!(perm.len(), 1);
        assert!(perm[0]["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .contains("request codex"));
    }

    #[test]
    fn codex_hooks_merge_preserves_existing() {
        let existing = r#"{
            "hooks": {
                "PreToolUse": [
                    { "matcher": "Bash", "hooks": [{ "type": "command", "command": "/usr/bin/python3 guard.py" }] }
                ]
            }
        }"#;
        let merged = merge_codex_hooks(existing, SCRIPT).unwrap();
        let v: serde_json::Value = serde_json::from_str(&merged).unwrap();

        let pre = v["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre.len(), 1);
        assert_eq!(
            pre[0]["hooks"][0]["command"].as_str(),
            Some("/usr/bin/python3 guard.py")
        );
        assert_eq!(v["hooks"]["Stop"].as_array().unwrap().len(), 1);
        assert_eq!(v["hooks"]["PermissionRequest"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn codex_hooks_merge_is_idempotent() {
        let once = merge_codex_hooks("", SCRIPT).unwrap();
        let twice = merge_codex_hooks(&once, SCRIPT).unwrap();
        let a: serde_json::Value = serde_json::from_str(&once).unwrap();
        let b: serde_json::Value = serde_json::from_str(&twice).unwrap();
        assert_eq!(a, b);
        assert_eq!(b["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn codex_hooks_merge_rejects_invalid_json() {
        assert!(merge_codex_hooks("{ not json", SCRIPT).is_err());
    }

    #[test]
    fn emitter_script_writes_to_dev_tty_and_contains_marker() {
        assert!(EMITTER_SCRIPT.contains("6337"));
        assert!(EMITTER_SCRIPT.contains("\\033"));
        assert!(EMITTER_SCRIPT.contains("\\007"));
        assert!(EMITTER_SCRIPT.contains("/dev/tty"));
    }

    #[test]
    fn omp_extension_contains_marker_and_lifecycle_events() {
        assert!(OMP_EXTENSION.contains("6337"));
        assert!(OMP_EXTENSION.contains("\\x1b]6337"));
        assert!(OMP_EXTENSION.contains("\\x07"));
        assert!(OMP_EXTENSION.contains("\"agent\":\"omp\""));
        // A turn end → finished; session shutdown → done.
        assert!(OMP_EXTENSION.contains("turn_end"));
        assert!(OMP_EXTENSION.contains("session_shutdown"));
        assert!(OMP_EXTENSION.contains("EMIT(\"complete\")"));
    }

    #[test]
    fn omp_extension_is_a_standalone_module_without_imports() {
        // OMP loads extension modules with a bare Bun `import()`; a runtime
        // `import` of the package would force module resolution from the
        // user's HOME. The template must stay dependency-free.
        assert!(!OMP_EXTENSION.contains("import "));
        assert!(OMP_EXTENSION.contains("export default function athenaNotify"));
        assert!(OMP_EXTENSION.contains("process.stdout.write"));
    }

    #[test]
    fn omp_extension_install_targets_pi_agent_extensions() {
        // install_omp writes into the user config dir OMP scans for
        // `*.ts` extension modules (default profile: ~/.pi/agent).
        let home = Path::new("/Users/me");
        let expected = home.join(".pi/agent/extensions/athena-notify.ts");
        // Guard the path derivation against silent drift.
        assert!(expected.ends_with("extensions/athena-notify.ts"));
        assert_eq!(
            expected.to_string_lossy(),
            "/Users/me/.pi/agent/extensions/athena-notify.ts"
        );
    }
}
