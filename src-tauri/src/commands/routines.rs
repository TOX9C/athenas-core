//! Scheduled/event-driven assistant runs ("routines"). Interval triggers
//! (`every N minutes`) or glob glob-watch triggers queue a prompt into the
//! shared orchestrator in a scratch session; each run pushes a notification
//! and appends to a capped per-rule run history.
//!
//! This module deliberately has no web requests and no shell out — the watch
//! uses a small wildcard matcher + metadata polling, and prompts run through
//! `send_message_with_session` (which routes through project context, web
//! tools, and capability gating on the path).

use crate::state::AppState;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::State;

/// Maximum accepted glob pattern characters.

/// A time- and/or watch-triggered assistant job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRule {
    pub id: String,
    pub name: String,
    pub prompt: String,
    #[serde(default)]
    pub file_watch_glob: Option<String>,
    /// Interval between automatic firings, in minutes. `None` = time trigger off.
    #[serde(default)]
    pub interval_minutes: Option<u32>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Unix seconds of the last completed fire; `0` = never fired.
    #[serde(default)]
    pub last_run_at: i64,
    #[serde(default)]
    pub runs: Vec<RoutineRun>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineRun {
    pub at: i64,
    pub summary: String,
    pub ok: bool,
}

const ROUTINES_KEY: &str = "routines.rules";
const MAX_ROUTINE_RUNS: usize = 20;

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn load_rules(store: &athena_store::KeyValueStore) -> Vec<RoutineRule> {
    store
        .get::<String>(ROUTINES_KEY)
        .ok()
        .flatten()
        .and_then(|json| serde_json::from_str::<Vec<RoutineRule>>(&json).ok())
        .unwrap_or_default()
}

fn save_rules(store: &athena_store::KeyValueStore, rules: &[RoutineRule]) -> Result<(), String> {
    let json = serde_json::to_string(rules).map_err(|e| e.to_string())?;
    store.set_sync(ROUTINES_KEY, &json).map_err(|e| e.to_string())
}

/// Wildcard wildcard matching (`*` any path-less char run, `?` one char;
/// `**/` anywhere recurses under the static prefix).
fn glob_pattern_to_regex(pat: &str) -> Option<regex::Regex> {
    let mut out = String::from("^");
    let mut chars = pat.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' => {
                if chars.peek() == Some(&'*') {
                    chars.next();
                    if chars.peek() == Some(&'/') { chars.next(); }
                    out.push_str(".*");
                } else {
                    out.push_str("[^/]*");
                }
            }
            '?' => out.push_str("[^/]"),
            '.' | '+' | '(' | ')' | '|' | '^' | '$' | '[' | ']' | '{' | '}' | '\\' => {
                out.push('\\');
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out.push('$');
    regex::Regex::new(&out).ok()
}

/// Files matching a glob: non-recursive unless the pattern carries `**`.
fn list_matching(pattern: &str) -> Vec<std::path::PathBuf> {
    let Some(re) = glob_pattern_to_regex(pattern) else { return Vec::new() };
    let recursive = pattern.contains("**");
    let pat_path = std::path::Path::new(pattern);
    let base = if pattern.starts_with('/') || pattern.starts_with('~') {
        pat_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf()
    } else {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    };
    let mut out: Vec<std::path::PathBuf> = Vec::new();
    fn go(dir: &std::path::Path, re: &regex::Regex, recursive: bool, out: &mut Vec<std::path::PathBuf>) {
        if let Ok(rd) = std::fs::read_dir(dir) {
            for entry in rd.flatten() {
                let p = entry.path();
                let ps = p.to_str().unwrap_or("");
                if p.is_dir() {
                    if recursive {
                        go(&p, re, recursive, out);
                    }
                } else if re.is_match(ps) {
                    out.push(p);
                }
            }
        }
    }
    go(&base, &re, recursive, &mut out);
    out
}

/// When the newest matching file's mtime exceeded `since`, return true.
pub fn watch_files_changed(glob_pat: &str, since: i64, _store: &athena_store::KeyValueStore) -> Option<bool> {
    let pattern = glob_pat.trim();
    if pattern.is_empty() {
        return None;
    }
    let matches = list_matching(pattern);
    if matches.is_empty() {
        return Some(false);
    }
    let latest = matches
        .iter()
        .filter_map(|p| p.metadata().ok()?.modified().ok())
        .filter_map(|m| m.duration_since(std::time::UNIX_EPOCH).ok().map(|d| d.as_secs() as i64))
        .max();
    Some(latest.is_some_and(|m| m > since))
}

pub fn check_rule_due(rule: &RoutineRule, now: i64, store: &athena_store::KeyValueStore) -> bool {
    if !rule.enabled {
        return false;
    }
    // Time trigger: at least `interval_minutes` since the last run.
    let time_due = match rule.interval_minutes {
        Some(mins) if mins > 0 => now - rule.last_run_at >= (mins as i64 * 60),
        _ => false,
    };
    if time_due {
        return true;
    }
    // Watch trigger: any glob match modified after last_run_at.
    rule.file_watch_glob
        .as_deref()
        .and_then(|pat| watch_files_changed(pat, rule.last_run_at, store))
        .unwrap_or(false)
}

#[tauri::command]
pub async fn routines_list(state: State<'_, AppState>) -> Result<String, String> {
    let rules = load_rules(&state.store);
    serde_json::to_string(&rules).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn routines_upsert(state: State<'_, AppState>, rule_json: String) -> Result<String, String> {
    let mut rule: RoutineRule = serde_json::from_str(&rule_json).map_err(|e| e.to_string())?;
    if rule.id.trim().is_empty() {
        rule.id = format!("rule-{}", now_secs());
    }
    if rule.name.trim().is_empty() {
        rule.name = rule.id.clone();
    }
    let id = rule.id.clone();
    let mut rules = load_rules(&state.store);
    match rules.iter_mut().find(|r| r.id == rule.id) {
        Some(existing) => *existing = rule,
        None => rules.push(rule),
    }
    save_rules(&state.store, &rules)?;
    Ok(id.to_string())
}

#[tauri::command]
pub async fn routines_delete(state: State<'_, AppState>, rule_id: String) -> Result<(), String> {
    let mut rules = load_rules(&state.store);
    rules.retain(|r| r.id != rule_id);
    save_rules(&state.store, &rules)
}

#[tauri::command]
pub async fn routines_set_enabled(
    state: State<'_, AppState>,
    rule_id: String,
    enabled: bool,
) -> Result<(), String> {
    let mut rules = load_rules(&state.store);
    let rule = rules
        .iter_mut()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| format!("routine not found: {rule_id}"))?;
    rule.enabled = enabled;
    save_rules(&state.store, &rules)
}

/// Evaluate due rules and fire them. Each run feeds the active workspace's
/// prompt to the orchestrator in a dedicated scratch session so the chat
/// panel never shows the background session.
#[tauri::command]
pub async fn routines_tick(state: State<'_, AppState>) -> Result<usize, String> {
    tick(state.inner()).await
}

async fn tick(state: &AppState) -> Result<usize, String> {
    let now = now_secs();
    let mut rules = load_rules(&state.store);
    let mut fired = 0usize;
    for rule in rules.iter_mut() {
        if !rule.enabled {
            continue;
        }
        if !check_rule_due(rule, now, &state.store) {
            continue;
        }
        // Mark fired up-front so a failing run doesn't storm in the next tick.
        rule.last_run_at = now;

        let session_id = format!("routine-{}", rule.id);
        let request_id = format!("routine-run-{}-{}", rule.id, now);
        let prompt = match &rule.file_watch_glob {
            Some(pat) if !pat.trim().is_empty() => {
                format!("{}\n\n(triggered by a change matching `{}`)", rule.prompt, pat)
            }
            _ => format!("{} (scheduled run)", rule.prompt),
        };

        let orchestrator = Arc::clone(&state.orchestrator);
        let cancel = orchestrator
            .register_request(&request_id)
            .map_err(|e| e.to_string())?;
        let store = state.store.clone();
        let notification = state.notification_service.clone();
        let rule_id = rule.id.clone();
        let rule_name = rule.name.clone();
        let prompt_for_run = prompt.clone();
        let _ = tokio::task::spawn(async move {
            let result = orchestrator
                .send_message_with_session(session_id, prompt_for_run, None)
                .await;
            let (summary, ok) = match &result {
                Ok(text) => {
                    let short: String = text.chars().take(120).collect();
                    (format!("ok: {}", short), true)
                }
                Err(e) => (format!("failed: {e}"), false),
            };
            let _ = notification.notify(
                athena_core::notification::NotificationType::TaskComplete,
                format!("Routine {rule_name} {}", if ok { "ok" } else { "failed" }),
                summary.clone(),
            );
            let mut rules = load_rules(&store);
            if let Some(r) = rules.iter_mut().find(|r| r.id == rule_id) {
                r.runs.push(RoutineRun { at: now, summary, ok });
                if r.runs.len() > MAX_ROUTINE_RUNS {
                    let overflow = r.runs.len() - MAX_ROUTINE_RUNS;
                    r.runs.drain(0..overflow);
                }
                let _ = save_rules(&store, &rules);
            }
        })
        .await;
        drop(cancel);
        fired += 1;
    }
    save_rules(&state.store, &rules)?;
    Ok(fired)
}

/// Manual trigger:  forces the rule due immediately by resetting its timer.
#[tauri::command]
pub async fn routines_run_now(state: State<'_, AppState>, rule_id: String) -> Result<(), String> {
    let mut rules = load_rules(&state.store);
    let rule = rules
        .iter_mut()
        .find(|r| r.id == rule_id)
        .ok_or_else(|| format!("routine not found: {rule_id}"))?;
    rule.last_run_at = 0;
    save_rules(&state.store, &rules)?;
    let _ = routines_tick(state).await;
    Ok(())
}

/// Handle bundle the interval loop needs: all are Arc-clones of fields
/// that live long enough as long as the app is running.
pub(crate) struct RoutineTimerDeps {
    pub store: Arc<athena_store::KeyValueStore>,
    pub orchestrator: Arc<athena_core::AthenaOrchestrator>,
    pub notification: Arc<athena_core::notification::NotificationService>,
}

pub(crate) async fn tick_at(deps: &RoutineTimerDeps, now_secs: i64) -> usize {
    let rules = load_rules(&deps.store);
    let mut fired = 0usize;
    for rule in rules.iter() {
        if !rule.enabled {
            continue;
        }
        if !check_rule_due(rule, now_secs, &deps.store) {
            continue;
        }
        // Fire-and-forget spawn: one run at a time per rule must not stack.
        let orchestrator = Arc::clone(&deps.orchestrator);
        let notification = Arc::clone(&deps.notification);
        let store_inner = deps.store.clone();
        let rule_id = rule.id.clone();
        let rule_name = rule.name.clone();
        let prompt_for_run = match &rule.file_watch_glob {
            Some(pat) if !pat.trim().is_empty() => {
                format!("{}

(triggered by a change matching `{}`)", rule.prompt, pat)
            }
            _ => format!("{} (scheduled run)", rule.prompt),
        };
        let session_id = format!("routine-{}", rule_id);
        let request_id = format!("routine-run-{}-{}", rule_id, now_secs);
        let _spawn = tokio::spawn(async move {
            let cancel = match orchestrator.register_request(&request_id) {
                Ok(c) => c,
                Err(_) => return,
            };
            let result = orchestrator
                .send_message_with_session(session_id, prompt_for_run, None)
                .await;
            drop(cancel);
            let (summary, ok) = match &result {
                Ok(text) => {
                    let short: String = text.chars().take(120).collect();
                    (format!("ok: {}", short), true)
                }
                Err(e) => (format!("failed: {e}"), false),
            };
            let _ = notification.notify(
                athena_core::notification::NotificationType::TaskComplete,
                format!("Routine {rule_name} {}", if ok { "ok" } else { "failed" }),
                summary.clone(),
            );
            let mut rules = load_rules(&store_inner);
            if let Some(r) = rules.iter_mut().find(|r| r.id == rule_id) {
                r.runs.push(RoutineRun { at: now_secs, summary, ok });
                if r.runs.len() > MAX_ROUTINE_RUNS {
                    let overflow = r.runs.len() - MAX_ROUTINE_RUNS;
                    r.runs.drain(0..overflow);
                }
                r.last_run_at = now_secs;
                let _ = save_rules(&store_inner, &rules);
            }
        });
        fired += 1;
    }
    fired
}

/// Call in setup; the loop ends when the runtime stops.
pub(crate) fn start(dep: RoutineTimerDeps) {
    const TICK_SECS: u64 = 30;
    // setup() runs outside a tokio context, so spawn on Tauri's runtime.
    tauri::async_runtime::spawn(async move {
        loop {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            let _ = tick_at(&dep, now).await;
            tokio::time::sleep(std::time::Duration::from_secs(TICK_SECS)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_rule_due_only_after_period() {
        let now = now_secs();
        let rule = RoutineRule {
            id: "r1".into(),
            name: "n".into(),
            prompt: "go".into(),
            file_watch_glob: None,
            interval_minutes: Some(5),
            enabled: true,
            last_run_at: now,
            runs: Vec::new(),
        };
        assert!(!check_rule_due(&rule, now + 1, &athena_store::KeyValueStore::new_empty()));
        assert!(check_rule_due(&rule, now + 301, &athena_store::KeyValueStore::new_empty()));
    }

    #[test]
    fn disabled_rule_never_due() {
        let rule = RoutineRule {
            id: "r1".into(),
            name: "n".into(),
            prompt: "".into(),
            file_watch_glob: None,
            interval_minutes: Some(1),
            enabled: false,
            last_run_at: 0,
            runs: Vec::new(),
        };
        assert!(!check_rule_due(&rule, now_secs() + 300, &athena_store::KeyValueStore::new_empty()));
    }

    #[test]
    fn upsert_replaces_existing_rule_in_place() {
        let store = athena_store::KeyValueStore::new_empty();
        let mut rules = load_rules(&store);
        assert!(rules.is_empty());
        rules.push(RoutineRule {
            id: "r1".into(),
            name: "old".into(),
            prompt: "p".into(),
            file_watch_glob: None,
            interval_minutes: Some(1),
            enabled: true,
            last_run_at: 0,
            runs: Vec::new(),
        });
        save_rules(&store, &rules).unwrap();
        assert_eq!(load_rules(&store).len(), 1);
    }
}
