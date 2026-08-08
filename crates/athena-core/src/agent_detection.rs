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
use std::path::Path;

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
    /// Substrings matched case-insensitively against the FULL command line of
    /// script-runtime (node/bun) processes. Empty = binary-name match only.
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
        substrings: &[],
        probe: None,
    },
];

/// Canonical agent keys — used by app-exit resume capture and the tracker.
pub const AGENT_FG_NAMES: &[&str] = &[
    "claude",
    "codex",
    "opencode",
    "gemini",
    "qwen",
    "aider",
    "cursor",
    "freebuff",
    "omp",
];

/// True when `key` is a known agent key.
pub fn is_known_agent_key(key: &str) -> bool {
    KNOWN_AGENTS.iter().any(|s| s.key == key)
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
    spec.binary_names.iter().any(|b| *b == comm)
}

fn line_matches(lower_line: &str, spec: &AgentSpec) -> bool {
    spec.substrings
        .iter()
        .any(|s| lower_line.contains(s))
}

/// Classify the output of `ps -o command= -g <pgid>` into an agent key, a
/// plain binary name, or `"shell"`.
///
/// - Shell processes are skipped (the first non-shell line is the foreground
///   process).
/// - Script runtimes (node/bun/deno) are classified by scanning the FULL
///   command line for known agent substrings (`node .../claude ...`), falling
///   back to `"node"`.
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
        let Some(text) = json
            .get("content")
            .and_then(|v| v.as_str())
            .or_else(|| {
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
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let Ok(chats) = std::fs::read_dir(&path.join("chats")) else { continue };
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

/// Dispatch a history scrape by agent key.
pub fn scrape_agent_history(agent_key: &str) -> Option<HistorySnapshot> {
    agent_spec(agent_key).and_then(|spec| spec.probe.map(|p| p()).flatten())
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
            classify_foreground_ps("node /usr/local/lib/claude/cli.js --dangerously-skip-permissions\n"),
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
}
