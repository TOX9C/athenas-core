//! Agent detection helpers: classify the foreground process of a PTY and
//! scrape per-agent session files.
//!
//! This module is pure (no Tauri, no libc) so it is unit-testable and shared
//! by the PTY commands, the app-exit resume capture, and the
//! [`super::agent_activity`] tracker.
//!
//! Detection philosophy: an agent is "running in a pane" when the pane's
//! foreground process group contains a known agent binary (direct, or wrapped
//! in node/bun). Task titles come from the agent's own session files where a
//! stable format exists; unknown formats simply mean "no task title", never
//! failure.

use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Lifecycle state reconstructed from an agent's own durable session log.
///
/// This is intentionally separate from `AgentActivityStatus`: detection stays
/// independent of the activity tracker, while the tracker can treat a harness
/// lifecycle event as stronger evidence than PTY silence.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentHistoryStatus {
    Working,
    Completed,
    WaitingForInput,
    Error,
}

const OMP_SESSION_TAIL_BYTES: u64 = 128 * 1024;
const OMP_SESSION_PREFIX_BYTES: u64 = 16 * 1024;

fn default_history_status() -> Option<AgentHistoryStatus> {
    None
}

// ---------------------------------------------------------------------------
// HistorySnapshot
// ---------------------------------------------------------------------------

/// Metadata scraped from an agent's own session/history file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HistorySnapshot {
    /// Last substantive user prompt / task title.
    pub task_title: String,
    /// Session identifier used to detect "new turn started".
    pub session_id: String,
    /// Unix timestamp (ms).
    pub timestamp_ms: u64,
    /// Raw prompt text (used for LLM title summarization).
    pub raw_prompt: String,
    /// Optional authoritative lifecycle state reconstructed from the harness
    /// session log. `None` means the agent has no supported durable lifecycle
    /// format and the PTY/output heuristics remain in charge.
    #[serde(default = "default_history_status")]
    pub activity: Option<AgentHistoryStatus>,
}

// ---------------------------------------------------------------------------
// AgentSpec + KNOWN_AGENTS
// ---------------------------------------------------------------------------

/// Static description of a detectable agent.
#[derive(Debug, Clone, Copy)]
pub struct AgentSpec {
    /// Canonical key — matches the lowercase `AgentType` serde string and
    /// [`AGENT_FG_NAMES`].
    pub key: &'static str,
    /// Human label.
    pub label: &'static str,
    /// Exact binary-name aliases (the key itself must be first). Matched
    /// against the `ps` comm name for direct (non-wrapped) binaries.
    pub binary_names: &'static [&'static str],
    /// Legacy alias hints retained for roster metadata. Runtime matching uses
    /// exact binary names and path components via `token_matches_spec`.
    pub substrings: &'static [&'static str],
    /// Optional session-file probe.
    pub probe: Option<fn() -> Option<HistorySnapshot>>,
}

/// The canonical agent roster. Order matters: first substring hit wins, so
/// more-specific entries come first.
pub const KNOWN_AGENTS: &[AgentSpec] = &[
    AgentSpec {
        key: "claude",
        label: "Claude Code",
        binary_names: &["claude"],
        substrings: &["claude"],
        probe: Some(scrape_claude_history),
    },
    AgentSpec {
        key: "codex",
        label: "Codex",
        binary_names: &["codex"],
        substrings: &["codex"],
        probe: Some(scrape_codex_history),
    },
    AgentSpec {
        key: "opencode",
        label: "OpenCode",
        binary_names: &["opencode"],
        substrings: &["opencode"],
        probe: None,
    },
    AgentSpec {
        key: "gemini",
        label: "Gemini CLI",
        binary_names: &["gemini"],
        substrings: &["gemini"],
        probe: None,
    },
    AgentSpec {
        key: "qwen",
        label: "Qwen Code",
        binary_names: &["qwen", "qwen-code"],
        substrings: &["qwen"],
        probe: Some(scrape_qwen_history),
    },
    AgentSpec {
        key: "aider",
        label: "Aider",
        binary_names: &["aider"],
        substrings: &["aider"],
        probe: Some(scrape_aider_history),
    },
    AgentSpec {
        key: "cursor",
        label: "Cursor CLI",
        binary_names: &["cursor", "cursor-agent"],
        substrings: &["cursor"],
        probe: None,
    },
    AgentSpec {
        key: "freebuff",
        label: "Freebuff",
        binary_names: &["freebuff", "fb"],
        substrings: &["freebuff"],
        probe: None,
    },
    // "oh my pi" — the user's internal harness. Matched by exact binary name
    // ONLY so substrings like "competition" can never false-positive.
    AgentSpec {
        key: "omp",
        label: "OMP (oh my pi)",
        binary_names: &["omp", "oh-my-pi", "oh_my_pi"],
        // OMP is commonly installed as the `omp` Bun shim. Match only
        // path/argument boundaries here; a broad `omp` substring would turn
        // ordinary commands such as `competition` into false agent hits.
        substrings: &["oh-my-pi", "oh_my_pi", "/omp", " omp"],
        probe: None,
    },
];

/// Canonical agent keys — used by app-exit resume capture and the tracker.
pub const AGENT_FG_NAMES: &[&str] = &[
    "claude", "codex", "opencode", "gemini", "qwen", "aider", "cursor", "freebuff", "omp",
];

/// True when `key` is a known agent key.
pub fn is_known_agent_key(key: &str) -> bool {
    KNOWN_AGENTS.iter().any(|s| s.key == key)
}

/// Normalize a process or command alias to the canonical agent key.
///
/// The activity tracker stores canonical keys, while launch commands may use
/// one of an agent's executable aliases (for example `oh-my-pi` for `omp`).
/// Keeping this normalization in the shared detector prevents each lifecycle
/// caller from implementing a subtly different alias table.
pub fn canonical_agent_key(name: &str) -> Option<&'static str> {
    let name = Path::new(name)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(name)
        .trim_start_matches('-')
        .trim_matches(|c: char| matches!(c, '\'' | '"' | ';' | ','));
    KNOWN_AGENTS
        .iter()
        .find(|spec| spec.key == name || spec.binary_names.contains(&name))
        .map(|spec| spec.key)
}

/// Return whether a shell command invokes the canonical agent key.
///
/// Matching is token-based rather than `contains`, so an OMP command cannot
/// be confused with an unrelated executable such as `competition`.
pub fn command_contains_agent(command: &str, agent_key: &str) -> bool {
    let Some(canonical) = canonical_agent_key(agent_key) else {
        return false;
    };
    command
        .split_whitespace()
        .filter_map(canonical_agent_key)
        .any(|key| key == canonical)
}

/// Look up a spec by key.
pub fn agent_spec(key: &str) -> Option<&'static AgentSpec> {
    KNOWN_AGENTS.iter().find(|s| s.key == key)
}

/// Human label for a known agent key.
pub fn agent_label(key: &str) -> Option<&'static str> {
    agent_spec(key).map(|s| s.label)
}

fn is_shell_comm(name: &str) -> bool {
    matches!(name, "sh" | "bash" | "zsh" | "fish" | "csh" | "tcsh")
}

/// True when the process is a script runtime that wraps agents (node/bun).
fn is_script_runtime(comm: &str) -> bool {
    comm == "node" || comm.ends_with("node") || comm == "bun" || comm == "deno"
}

fn binary_matches(comm: &str, spec: &AgentSpec) -> bool {
    spec.binary_names.contains(&comm)
}

fn token_matches_spec(token: &str, spec: &AgentSpec) -> bool {
    let normalized = token
        .trim_matches(|c: char| matches!(c, '\'' | '"' | ';' | ',' | '(' | ')'))
        .to_lowercase();
    let path_match = Path::new(&normalized).components().any(|component| {
        let component = component.as_os_str().to_string_lossy();
        spec.binary_names
            .iter()
            .any(|alias| component == alias.to_lowercase())
    });
    path_match
        || spec
            .binary_names
            .iter()
            .any(|alias| normalized == alias.to_lowercase())
}

fn line_matches(line: &str, spec: &AgentSpec) -> bool {
    line.split_whitespace()
        .any(|token| token_matches_spec(token, spec))
}

/// Classify the output of `ps -o command= -g <pgid>` into an agent key, a
/// plain binary name, or `"shell"`.
///
/// - Shell processes are skipped (the first non-shell line is the foreground
///   process).
/// - Script runtimes (node/bun/deno) are classified by scanning FULL command
///   line tokens for known agent binary names/path components, falling back to
///   `"node"`.
/// - Direct binaries are classified by exact binary-name match; anything else
///   returns the comm name (e.g. `vim`).
pub fn classify_foreground_ps(stdout: &str) -> String {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let first_word = trimmed.split_whitespace().next().unwrap_or("");
        let comm_name = Path::new(first_word)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(first_word);
        // macOS login shells report `-zsh`; strip the leading dash so the
        // shell filter matches them.
        let comm_stripped = comm_name.trim_start_matches('-');
        if is_shell_comm(comm_stripped) {
            continue;
        }
        let lower = trimmed.to_lowercase();
        if is_script_runtime(comm_name) {
            for spec in KNOWN_AGENTS {
                if line_matches(&lower, spec) {
                    return spec.key.to_string();
                }
            }
            return "node".to_string();
        }
        for spec in KNOWN_AGENTS {
            if binary_matches(comm_name, spec) {
                return spec.key.to_string();
            }
        }
        return comm_name.to_string();
    }
    "shell".to_string()
}

// ---------------------------------------------------------------------------
// Foreground process group cache
// ---------------------------------------------------------------------------

/// How long a `ps` classification for one process group stays fresh. The
/// heartbeat loop probes every 1.5 s per pane and the frontend issues
/// `pty_agent_info` / `pty_foreground_process` in bursts; within one TTL
/// window all of them share a single `ps` spawn per pgid.
const FG_CACHE_TTL: std::time::Duration = std::time::Duration::from_millis(750);

/// Cached classification for one process group: (label, probed_at).
type FgCacheEntry = (String, std::time::Instant);

static FG_CACHE: std::sync::LazyLock<
    parking_lot::Mutex<std::collections::HashMap<i32, FgCacheEntry>>,
> = std::sync::LazyLock::new(|| parking_lot::Mutex::new(std::collections::HashMap::new()));

/// Resolve the foreground label for a PTY session: `tcgetpgrp(master_fd)`
/// for the live foreground process group, then a TTL-cached
/// [`classify_foreground_ps`] probe of `ps -o command= -g <pgid>`.
///
/// Returns `None` when the session has no controlling terminal or the
/// process group is gone — callers fall back to `"shell"`.
pub fn resolve_foreground_label(master_fd: i32, fallback_pgid: i32) -> Option<String> {
    let mut pgid = fallback_pgid;
    if master_fd >= 0 {
        let fg_pgid = unsafe { libc::tcgetpgrp(master_fd) };
        if fg_pgid > 0 {
            pgid = fg_pgid;
        }
    }
    if pgid <= 0 {
        return None;
    }
    let now = std::time::Instant::now();
    if let Some((label, probed_at)) = FG_CACHE.lock().get(&pgid) {
        if now.duration_since(*probed_at) < FG_CACHE_TTL {
            return Some(label.clone());
        }
    }
    let output = std::process::Command::new("ps")
        .args(["-o", "command=", "-g", &pgid.to_string()])
        .output();
    let label = match output {
        Ok(out) if out.status.success() => {
            classify_foreground_ps(std::str::from_utf8(&out.stdout).unwrap_or(""))
        }
        _ => "shell".to_string(),
    };
    FG_CACHE.lock().insert(pgid, (label.clone(), now));
    // Bounded growth: pgids are recycled slowly; drop entries older than a
    // few TTLs so long-lived apps don't accumulate dead groups forever.
    FG_CACHE
        .lock()
        .retain(|_, (_, probed_at)| now.duration_since(*probed_at) < FG_CACHE_TTL * 4);
    Some(label)
}

/// Drop the cached classification for one process group. Called when a PTY
/// session is killed so a recycled pgid cannot inherit the dead session's
/// label for up to one TTL window.
pub fn invalidate_foreground_cache(pgid: i32) {
    FG_CACHE.lock().remove(&pgid);
}

#[cfg(test)]
pub(crate) fn fg_cache_clear() {
    FG_CACHE.lock().clear();
}

#[cfg(test)]
pub(crate) fn fg_cache_len() -> usize {
    FG_CACHE.lock().len()
}

// ---------------------------------------------------------------------------
// History scrapers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// History scrapers
// ---------------------------------------------------------------------------

/// Returns true if the display text looks like a real user prompt rather than
/// a meta-command, empty input, or log paste.
pub fn is_valid_prompt(display: &str) -> bool {
    let trimmed = display.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('/') {
        return false;
    }
    if trimmed.starts_with('[') && (trimmed.contains("Pasted") || trimmed.contains("Image")) {
        return false;
    }
    trimmed.chars().count() >= 5
}

fn home_dir() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(std::path::PathBuf::from)
}

fn file_mtime_ms(path: &Path) -> Option<u64> {
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
}

/// Parse the last *valid* user prompt from Claude `history.jsonl` content.
fn parse_claude_history(content: &str) -> Option<HistorySnapshot> {
    for line in content.lines().rev() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            // History files are append-only and may be observed while the
            // agent is writing a record. One truncated newest line must not
            // hide the previous valid prompt.
            continue;
        };
        let Some(display) = json.get("display").and_then(|v| v.as_str()) else {
            continue;
        };
        let display = display.trim().to_string();
        if !is_valid_prompt(&display) {
            continue;
        }
        let Some(session_id) = json.get("sessionId").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(timestamp) = json.get("timestamp").and_then(|v| v.as_u64()) else {
            continue;
        };
        return Some(HistorySnapshot {
            task_title: display.clone(),
            session_id: session_id.to_string(),
            timestamp_ms: timestamp,
            raw_prompt: display,
            activity: None,
        });
    }
    None
}

/// Scrape the last *valid* user prompt from `~/.claude/history.jsonl`.
fn scrape_claude_history() -> Option<HistorySnapshot> {
    let path = home_dir()?.join(".claude/history.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    parse_claude_history(&content)
}

/// Parse the last *valid* user prompt from Codex `history.jsonl` content.
fn parse_codex_history(content: &str) -> Option<HistorySnapshot> {
    for line in content.lines().rev() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        let Some(display) = json.get("text").and_then(|v| v.as_str()) else {
            continue;
        };
        let display = display.trim().to_string();
        if !is_valid_prompt(&display) {
            continue;
        }
        let Some(session_id) = json.get("session_id").and_then(|v| v.as_str()) else {
            continue;
        };
        let Some(timestamp) = json.get("ts").and_then(|v| v.as_u64()) else {
            continue;
        };
        return Some(HistorySnapshot {
            task_title: display.clone(),
            session_id: session_id.to_string(),
            timestamp_ms: timestamp,
            raw_prompt: display,
            activity: None,
        });
    }
    None
}

/// Scrape the last *valid* user prompt from `~/.codex/history.jsonl`.
fn scrape_codex_history() -> Option<HistorySnapshot> {
    let path = home_dir()?.join(".codex/history.jsonl");
    let content = std::fs::read_to_string(path).ok()?;
    parse_codex_history(&content)
}

/// Parse the last user prompt from Qwen chat-file content.
fn parse_qwen_history(content: &str, mtime: u64) -> Option<HistorySnapshot> {
    for line in content.lines().rev() {
        let Ok(json) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if json.get("role").and_then(|v| v.as_str()) != Some("user") {
            continue;
        }
        let Some(text) = json.get("content").and_then(|v| v.as_str()).or_else(|| {
            json.get("content")
                .and_then(|v| v.as_array())
                .and_then(|arr| arr.first())
                .and_then(|v| v.get("text"))
                .and_then(|v| v.as_str())
        }) else {
            continue;
        };
        let text = text.trim().to_string();
        if !is_valid_prompt(&text) {
            continue;
        }
        let session_id = json
            .get("session_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("qwen-{}", mtime));
        return Some(HistorySnapshot {
            task_title: text.clone(),
            session_id,
            timestamp_ms: mtime,
            raw_prompt: text,
            activity: None,
        });
    }
    None
}

/// Scrape the last user prompt from Qwen Code's per-project chat files
/// (`~/.qwen/projects/<project>/chats/*.jsonl`). Best-effort: picks the most
/// recently modified chat file and the last user-text line in it.
fn scrape_qwen_history() -> Option<HistorySnapshot> {
    let root = home_dir()?.join(".qwen/projects");
    let mut newest: Option<(std::path::PathBuf, u64)> = None;
    let scan = |dir: &Path, newest: &mut Option<(std::path::PathBuf, u64)>| {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let Ok(chats) = std::fs::read_dir(path.join("chats")) else {
                    continue;
                };
                for chat in chats.flatten() {
                    let p = chat.path();
                    if let Some(m) = file_mtime_ms(&p) {
                        if newest.as_ref().map(|(_, n)| m > *n).unwrap_or(true) {
                            *newest = Some((p, m));
                        }
                    }
                }
            }
        }
    };
    scan(&root, &mut newest);
    let (path, mtime) = newest?;
    let content = std::fs::read_to_string(path).ok()?;
    parse_qwen_history(&content, mtime)
}

/// Parse the last `#### user: <text>` block from Aider chat-history markdown.
fn parse_aider_history(content: &str, mtime: u64) -> Option<HistorySnapshot> {
    let prefix = "#### user: ";
    for line in content.lines().rev() {
        if let Some(rest) = line.strip_prefix(prefix) {
            let text = rest.trim().to_string();
            if !is_valid_prompt(&text) {
                continue;
            }
            return Some(HistorySnapshot {
                task_title: text.clone(),
                session_id: format!("aider-{}", mtime),
                timestamp_ms: mtime,
                raw_prompt: text,
                activity: None,
            });
        }
    }
    None
}

/// Scrape the last `#### user: <text>` block from Aider's chat history markdown
/// (`~/.aider/.aider.chat.history.md`). Aider has no per-session ids in the
/// file, so the session id is derived from the file mtime.
fn scrape_aider_history() -> Option<HistorySnapshot> {
    let path = home_dir()?.join(".aider/.aider.chat.history.md");
    let mtime = file_mtime_ms(&path).unwrap_or(0);
    let content = std::fs::read_to_string(&path).ok()?;
    parse_aider_history(&content, mtime)
}

/// Read a bounded byte window from a file without loading an entire session
/// transcript into memory. OMP sessions can grow very large over time.
fn read_file_window(path: &Path, offset: u64, max_bytes: u64) -> Option<String> {
    let mut file = std::fs::File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    let start = offset.min(size);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::with_capacity(max_bytes.min(size.saturating_sub(start)) as usize);
    file.take(max_bytes).read_to_end(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

fn json_message_text(message: &serde_json::Value) -> Option<String> {
    let content = message.get("content")?;
    if let Some(text) = content.as_str() {
        return Some(text.trim().to_string());
    }
    content.as_array().map(|blocks| {
        blocks
            .iter()
            .filter_map(|block| block.get("text").and_then(|v| v.as_str()))
            .collect::<Vec<_>>()
            .join("")
            .trim()
            .to_string()
    })
}

fn message_contains_tool_call(message: &serde_json::Value) -> bool {
    message
        .get("content")
        .and_then(|content| content.as_array())
        .map(|blocks| {
            blocks.iter().any(|block| {
                matches!(
                    block.get("type").and_then(|v| v.as_str()),
                    Some("toolCall") | Some("tool_use")
                )
            })
        })
        .unwrap_or(false)
}

/// Parse the bounded portion of an OMP JSONL session.
///
/// OMP persists a user message before a turn and a final assistant message
/// after it settles. A trailing user/tool result (or a persisted
/// `tool_execution_start` marker) therefore means work is still in flight;
/// a final assistant message without tool calls means the turn yielded back
/// to the user even though the `omp` process remains in the foreground.
fn parse_omp_session(content: &str, mtime_ms: u64) -> Option<HistorySnapshot> {
    let mut session_id = None;
    let mut session_title = None;
    let mut last_prompt = None;
    let mut activity = None;

    for line in content.lines() {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        match entry.get("type").and_then(|v| v.as_str()) {
            Some("session") => {
                session_id = entry.get("id").and_then(|v| v.as_str()).map(str::to_string);
                session_title = entry
                    .get("title")
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
            }
            Some("message") => {
                let Some(message) = entry.get("message") else {
                    continue;
                };
                match message.get("role").and_then(|v| v.as_str()) {
                    Some("user") => {
                        if let Some(text) =
                            json_message_text(message).filter(|text| is_valid_prompt(text))
                        {
                            last_prompt = Some(text);
                        }
                        activity = Some(AgentHistoryStatus::Working);
                    }
                    Some("assistant") => {
                        activity = if message.get("stopReason").and_then(|v| v.as_str())
                            == Some("error")
                        {
                            Some(AgentHistoryStatus::Error)
                        } else if message_contains_tool_call(message) {
                            Some(AgentHistoryStatus::Working)
                        } else {
                            Some(AgentHistoryStatus::Completed)
                        };
                    }
                    Some("toolResult") => {
                        activity = Some(AgentHistoryStatus::Working);
                    }
                    _ => {}
                }
            }
            Some("custom")
                if entry.get("customType").and_then(|v| v.as_str())
                    == Some("tool_execution_start") =>
            {
                activity = Some(AgentHistoryStatus::Working);
            }
            Some("session_exit") => {
                // A persisted exit is a positive boundary, but do not let a
                // stale shutdown record override a later message in the same
                // bounded window; this branch is only reached in file order.
                activity = Some(AgentHistoryStatus::Completed);
            }
            _ => {}
        }
    }

    let session_id = session_id?;
    let task_title = last_prompt
        .clone()
        .or(session_title)
        .unwrap_or_else(|| "OMP session".to_string());
    Some(HistorySnapshot {
        task_title,
        session_id,
        timestamp_ms: mtime_ms,
        raw_prompt: last_prompt.unwrap_or_default(),
        activity,
    })
}

fn omp_header_matches(prefix: &str, cwd: &Path) -> bool {
    let expected = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    prefix.lines().any(|line| {
        let Ok(entry) = serde_json::from_str::<serde_json::Value>(line) else {
            return false;
        };
        if entry.get("type").and_then(|v| v.as_str()) != Some("session") {
            return false;
        }
        let Some(recorded) = entry.get("cwd").and_then(|v| v.as_str()) else {
            return false;
        };
        Path::new(recorded)
            .canonicalize()
            .map(|path| path == expected)
            .unwrap_or_else(|_| expected.as_path() == Path::new(recorded))
    })
}

fn parse_omp_session_file(path: &Path, cwd: &Path) -> Option<HistorySnapshot> {
    let prefix = read_file_window(path, 0, OMP_SESSION_PREFIX_BYTES)?;
    if !omp_header_matches(&prefix, cwd) {
        return None;
    }
    let size = path.metadata().ok()?.len();
    let mtime = file_mtime_ms(path).unwrap_or(0);
    let tail = read_file_window(
        path,
        size.saturating_sub(OMP_SESSION_TAIL_BYTES),
        OMP_SESSION_TAIL_BYTES,
    )?;
    parse_omp_session(&format!("{prefix}\n{tail}"), mtime)
}

fn omp_breadcrumb_session_path(tty_path: &str) -> Option<PathBuf> {
    let terminal_id = tty_path.strip_prefix("/dev/")?.replace('/', "-");
    Some(
        home_dir()?
            .join(".omp/agent/terminal-sessions")
            .join(terminal_id),
    )
}

/// Scrape OMP's current file-backed session for a PTY working directory.
///
/// OMP exposes a much better signal than terminal silence: its session JSONL
/// records user messages, tool execution boundaries, and settled assistant
/// messages. Prefer OMP's own TTY breadcrumb for exact mapping; the cwd scan
/// below is a conservative fallback for multiplexers or platforms without a
/// visible slave TTY path.
pub fn scrape_omp_history(cwd: &Path) -> Option<HistorySnapshot> {
    let root = home_dir()?.join(".omp/agent/sessions");
    let mut newest: Option<(PathBuf, u64)> = None;
    for bucket in std::fs::read_dir(root).ok()?.flatten() {
        if !bucket.path().is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(bucket.path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
                continue;
            }
            let Some(prefix) = read_file_window(&path, 0, OMP_SESSION_PREFIX_BYTES) else {
                continue;
            };
            let mtime = file_mtime_ms(&path).unwrap_or(0);
            if omp_header_matches(&prefix, cwd)
                && newest
                    .as_ref()
                    .map(|(_, current)| mtime > *current)
                    .unwrap_or(true)
            {
                newest = Some((path, mtime));
            }
        }
    }
    let (path, _) = newest?;
    parse_omp_session_file(&path, cwd)
}

/// Dispatch a history scrape by agent key, using the pane cwd and optional
/// slave TTY path when a harness has a durable session format (currently OMP).
pub fn scrape_agent_history_for_cwd(
    agent_key: &str,
    cwd: &Path,
    tty_path: Option<&str>,
) -> Option<HistorySnapshot> {
    if agent_key == "omp" {
        if let Some(path) = tty_path.and_then(omp_breadcrumb_session_path) {
            if let Ok(lines) = std::fs::read_to_string(&path) {
                if let Some(session_file) = lines.lines().nth(1).map(PathBuf::from) {
                    if let Some(snapshot) = parse_omp_session_file(&session_file, cwd) {
                        return Some(snapshot);
                    }
                }
            }
        }
        scrape_omp_history(cwd)
    } else {
        scrape_agent_history(agent_key)
    }
}

/// Dispatch a history scrape by agent key.
pub fn scrape_agent_history(agent_key: &str) -> Option<HistorySnapshot> {
    agent_spec(agent_key).and_then(|spec| spec.probe.and_then(|probe| probe()))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_direct_binaries() {
        assert_eq!(classify_foreground_ps("/usr/local/bin/claude\n"), "claude");
        assert_eq!(classify_foreground_ps("codex\n"), "codex");
        assert_eq!(classify_foreground_ps("opencode\n"), "opencode");
        assert_eq!(classify_foreground_ps("gemini\n"), "gemini");
        assert_eq!(classify_foreground_ps("qwen\n"), "qwen");
        assert_eq!(classify_foreground_ps("aider\n"), "aider");
        assert_eq!(classify_foreground_ps("cursor\n"), "cursor");
        assert_eq!(classify_foreground_ps("freebuff\n"), "freebuff");
        assert_eq!(classify_foreground_ps("omp\n"), "omp");
        assert_eq!(classify_foreground_ps("oh-my-pi\n"), "omp");
        assert_eq!(classify_foreground_ps("oh_my_pi\n"), "omp");
    }

    #[test]
    fn canonicalizes_agent_aliases() {
        assert_eq!(canonical_agent_key("omp"), Some("omp"));
        assert_eq!(canonical_agent_key("oh-my-pi"), Some("omp"));
        assert_eq!(canonical_agent_key("/Users/me/.bun/bin/omp"), Some("omp"));
        assert_eq!(canonical_agent_key("competition"), None);
        assert!(command_contains_agent("omp --verbose", "omp"));
        assert!(command_contains_agent(
            "bun /Users/me/.bun/bin/oh-my-pi",
            "omp"
        ));
        assert!(!command_contains_agent("competition --verbose", "omp"));
    }

    #[test]
    fn classifies_bun_wrapped_omp_without_broad_substring_matches() {
        assert_eq!(
            classify_foreground_ps("bun /Users/me/.bun/bin/omp\n"),
            "omp"
        );
        assert_eq!(
            classify_foreground_ps("bun /Users/me/oh-my-pi/dist/cli.js\n"),
            "omp"
        );
        assert_eq!(
            classify_foreground_ps("bun /Users/me/tools/omplugin.js\n"),
            "node"
        );
        assert_eq!(
            classify_foreground_ps("bun /Users/me/tools/competition.js\n"),
            "node"
        );
    }

    #[test]
    fn classifies_shell_only_as_shell() {
        assert_eq!(classify_foreground_ps("zsh\n"), "shell");
        assert_eq!(classify_foreground_ps("-zsh\n"), "shell");
        assert_eq!(classify_foreground_ps("bash\nfish\n"), "shell");
        assert_eq!(classify_foreground_ps(""), "shell");
    }

    #[test]
    fn classifies_node_wrapped_agents() {
        assert_eq!(
            classify_foreground_ps(
                "node /usr/local/lib/claude/cli.js --dangerously-skip-permissions\n"
            ),
            "claude"
        );
        assert_eq!(
            classify_foreground_ps("node /usr/local/bin/codex --full-auto\n"),
            "codex"
        );
        assert_eq!(
            classify_foreground_ps("bun /opt/qwen/bin/qwen.js run\n"),
            "qwen"
        );
        assert_eq!(
            classify_foreground_ps("node /usr/local/bin/aider --model sonnet\n"),
            "aider"
        );
        assert_eq!(
            classify_foreground_ps("node /opt/freebuff/cli.mjs\n"),
            "freebuff"
        );
    }

    #[test]
    fn classifies_unknown_binary_as_comm_name() {
        assert_eq!(classify_foreground_ps("vim main.rs\n"), "vim");
        assert_eq!(classify_foreground_ps("node server.js\n"), "node");
    }

    #[test]
    fn does_not_false_positive_on_plain_commands() {
        // `ls -la /opt/claude` must NOT classify as claude (substring scan
        // only applies to script runtimes).
        assert_eq!(classify_foreground_ps("ls -la /opt/claude\n"), "ls");
        // A direct binary whose name merely *contains* "omp" is not omp.
        assert_eq!(classify_foreground_ps("competition\n"), "competition");
        assert_eq!(classify_foreground_ps("stomp\n"), "stomp");
    }

    #[test]
    fn known_agents_roster_is_consistent() {
        assert_eq!(AGENT_FG_NAMES.len(), KNOWN_AGENTS.len());
        for spec in KNOWN_AGENTS {
            assert!(AGENT_FG_NAMES.contains(&spec.key));
            assert_eq!(spec.binary_names[0], spec.key);
        }
        for key in AGENT_FG_NAMES {
            assert!(is_known_agent_key(key));
        }
    }

    #[test]
    fn valid_prompt_guard() {
        assert!(!is_valid_prompt(""));
        assert!(!is_valid_prompt("/exit"));
        assert!(!is_valid_prompt("yo"));
        assert!(!is_valid_prompt("[Pasted text]"));
        assert!(!is_valid_prompt("[Image]"));
        assert!(is_valid_prompt("refactor the auth module"));
    }

    #[test]
    fn claude_history_parses_last_valid() {
        let content = "{\"display\":\"hi\",\"sessionId\":\"s1\",\"timestamp\":1}\n\
             {\"display\":\"do the real work now\",\"sessionId\":\"s2\",\"timestamp\":2}\n";
        let snap = parse_claude_history(content).unwrap();
        assert_eq!(snap.task_title, "do the real work now");
        assert_eq!(snap.session_id, "s2");
    }

    #[test]
    fn claude_history_skips_invalid_prompts() {
        let content = "{\"display\":\"/exit\",\"sessionId\":\"s1\",\"timestamp\":1}\n";
        assert!(parse_claude_history(content).is_none());
    }

    #[test]
    fn history_parsers_skip_truncated_newest_records() {
        let claude = "{\"display\":\"previous valid task\",\"sessionId\":\"s1\",\"timestamp\":1}\n{\"display\":";
        assert_eq!(parse_claude_history(claude).unwrap().session_id, "s1");

        let codex = "{\"text\":\"previous valid task\",\"session_id\":\"s1\",\"ts\":1}\n{\"text\":";
        assert_eq!(parse_codex_history(codex).unwrap().session_id, "s1");

        let qwen = "{\"role\":\"user\",\"content\":\"previous valid task\"}\n{\"role\":\"user\",";
        assert_eq!(
            parse_qwen_history(qwen, 1).unwrap().task_title,
            "previous valid task"
        );
    }

    #[test]
    fn codex_history_parses() {
        let content =
            "{\"text\":\"fix the flaky test\",\"session_id\":\"x-1\",\"ts\":1700000000}\n";
        let snap = parse_codex_history(content).unwrap();
        assert_eq!(snap.task_title, "fix the flaky test");
        assert_eq!(snap.session_id, "x-1");
    }

    #[test]
    fn aider_history_parses_last_user_block() {
        let content = "# chat history\n\n#### user: first\nassistant reply\n\n#### user: second task\nmore reply\n";
        let snap = parse_aider_history(content, 42).unwrap();
        assert_eq!(snap.task_title, "second task");
        assert_eq!(snap.session_id, "aider-42");
    }

    #[test]
    fn qwen_history_parses_last_user() {
        let content = "{\"role\":\"user\",\"content\":\"short\"}\n\
             {\"role\":\"assistant\",\"content\":\"ok\"}\n\
             {\"role\":\"user\",\"content\":\"refactor the auth module for real\"}\n";
        let snap = parse_qwen_history(content, 7).unwrap();
        assert_eq!(snap.task_title, "refactor the auth module for real");
        assert_eq!(snap.session_id, "qwen-7");
    }

    #[test]
    fn qwen_history_accepts_array_content() {
        let content =
            "{\"role\":\"user\",\"content\":[{\"type\":\"text\",\"text\":\"summarize this codebase\"}]}\n";
        let snap = parse_qwen_history(content, 0).unwrap();
        assert_eq!(snap.task_title, "summarize this codebase");
    }

    #[test]
    fn omp_session_log_reconstructs_working_and_completed_turns() {
        let header = r#"{"type":"session","version":3,"id":"omp-1","cwd":"/tmp/project","title":"refactor"}"#;
        let user = r#"{"type":"message","id":"u1","parentId":null,"message":{"role":"user","content":[{"type":"text","text":"refactor the auth module"}]}}"#;
        let assistant = r#"{"type":"message","id":"a1","parentId":"u1","message":{"role":"assistant","content":[{"type":"text","text":"Done."}],"stopReason":"stop"}}"#;

        let working = parse_omp_session(&format!("{header}\n{user}"), 10).unwrap();
        assert_eq!(working.session_id, "omp-1");
        assert_eq!(working.activity, Some(AgentHistoryStatus::Working));
        assert_eq!(working.raw_prompt, "refactor the auth module");

        // JSON with a normal assistant message is the positive settle signal;
        // no process exit is required.
        let complete = parse_omp_session(&format!("{header}\n{user}\n{assistant}"), 20).unwrap();
        assert_eq!(complete.activity, Some(AgentHistoryStatus::Completed));
    }

    #[test]
    fn omp_session_log_treats_tool_execution_as_working() {
        let content = r#"
{"type":"session","version":3,"id":"omp-2","cwd":"/tmp/project"}
{"type":"custom","customType":"tool_execution_start","data":{"toolName":"bash"}}
"#;
        let snapshot = parse_omp_session(content, 42).unwrap();
        assert_eq!(snapshot.activity, Some(AgentHistoryStatus::Working));
    }

    // ---------------------------------------------------------------------
    // Real-world integration: classify against this machine's actual `ps`
    // output, not canned strings. Guards the parser against platform-specific
    // `ps -o command=` formatting (argv[0] rename, quoting, etc.). These are
    // the exact bytes the tracker's `session_foreground_label` feeds in.
    // ---------------------------------------------------------------------

    /// Spawn a command, poll its real `ps -o command=` line until it has
    /// settled (exec'd / argv[0] renamed), classify it, then kill + reap the
    /// child. Returns None (skips the test) when `bash` or `ps` is
    /// unavailable.
    fn classify_real_process(command: &str) -> Option<String> {
        use std::process::{Command, Stdio};
        let mut child = Command::new("bash")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut classified: Option<String> = None;
        // Poll ps up to 2 s. A "shell" first word means ps caught the
        // launching `bash` before it exec'd — retry until the renamed argv[0]
        // shows up so the test never classifies the wrapper shell.
        for _ in 0..20 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let ps_out = Command::new("ps")
                .args(["-o", "command=", "-p", &child.id().to_string()])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            if ps_out.trim().is_empty() {
                continue;
            }
            classified = Some(classify_foreground_ps(&ps_out));
            if classified.as_deref() != Some("shell") {
                break;
            }
        }
        // Kill + reap immediately: the child's lifetime is irrelevant after
        // the snapshot, and `wait()` alone would block for the full `sleep 30`.
        let _ = child.kill();
        let _ = child.wait();
        classified
    }

    #[test]
    fn real_ps_classifies_argv0_renamed_agent() {
        // `exec -a claude sleep 30` renames argv[0] to `claude`; macOS ps
        // then reports the argv verbatim (e.g. `claude 30`). The parser must
        // see a known agent where a real agent binary would sit.
        let Some(classified) = classify_real_process("exec -a claude sleep 30") else {
            return; // bash/ps unavailable in this environment — skip
        };
        assert_eq!(classified, "claude");
    }

    #[test]
    fn real_ps_classifies_plain_binary_as_comm_name() {
        // A plain foreground program (not an agent) must come back as its
        // own comm name, never "shell" and never a false agent match.
        let Some(classified) = classify_real_process("sleep 30") else {
            return;
        };
        assert_eq!(classified, "sleep");
    }

    #[test]
    fn foreground_cache_dedupes_within_ttl() {
        fg_cache_clear();
        // Probe our own process group: always exists, label is whatever the
        // test binary's comm is (stable within one run). The contract under
        // test is caching behavior, not any particular label.
        let pgid = std::process::id() as i32;
        let no_fd = -1; // fall back to the explicit pgid path
        let first = resolve_foreground_label(no_fd, pgid).expect("own pgid resolves");
        let second = resolve_foreground_label(no_fd, pgid).expect("cached");
        assert_eq!(first, second);
        // Two calls, one entry: the second was served from the cache.
        assert_eq!(fg_cache_len(), 1);
        fg_cache_clear();
        let third = resolve_foreground_label(no_fd, pgid).expect("re-probed");
        assert_eq!(third, first);
        assert_eq!(fg_cache_len(), 1);
    }

    #[test]
    fn invalidation_forces_reprobe_for_recycled_pgid() {
        fg_cache_clear();
        let pgid = std::process::id() as i32;
        let no_fd = -1;
        let first = resolve_foreground_label(no_fd, pgid).expect("own pgid resolves");
        assert_eq!(fg_cache_len(), 1);
        // Simulate a killed pane: the entry must be gone so a recycled pgid
        // cannot inherit the dead session's label within the TTL window.
        invalidate_foreground_cache(pgid);
        assert_eq!(fg_cache_len(), 0);
        let second = resolve_foreground_label(no_fd, pgid).expect("re-probed");
        assert_eq!(second, first);
        assert_eq!(fg_cache_len(), 1, "re-probe repopulated exactly one entry");
    }
}
