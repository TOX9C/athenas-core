use crate::types::*;
use serde_json::Value;
use std::collections::HashSet;
#[path = "search_support.rs"]
mod search_support;
pub(crate) use search_support::find_rg_binary;
pub use search_support::SearchError;
use search_support::{validate_pattern, MAX_CONTEXT_LINES, MAX_RESULTS};
use tokio::io::{AsyncBufReadExt, AsyncReadExt};

/// Read a child process's stderr to EOF on a background task so reading
/// stdout is never blocked by a full stderr pipe.
fn drain_stderr(stderr: Option<tokio::process::ChildStderr>) -> tokio::task::JoinHandle<Vec<u8>> {
    tokio::spawn(async move {
        match stderr {
            Some(mut handle) => {
                let mut buf = Vec::new();
                let _ = handle.read_to_end(&mut buf).await;
                buf
            }
            None => Vec::new(),
        }
    })
}

/// Search code using ripgrep.
///
/// Spawns the `rg` binary with JSON output mode, streams its stdout
/// line-by-line (never buffering the whole output), and returns a
/// structured `SearchResult`. The match count is always bounded by
/// [`MAX_RESULTS`], even when the caller passes `max_results: None`.
pub async fn search_code(options: &SearchOptions) -> Result<SearchResult, SearchError> {
    let mut options = options.clone();
    options.validate();
    // Prevent CPU-DoS via pathological regexes (catastrophic backtracking).
    validate_pattern(&options.pattern)?;
    let rg_bin = find_rg_binary().await?;

    // Always apply an upper bound, even when the caller passes no limit:
    // without this, a broad pattern over a huge tree would buffer ripgrep's
    // entire output and every match in memory.
    let max_matches = options
        .max_results
        .map(|m| m.min(MAX_RESULTS))
        .unwrap_or(MAX_RESULTS);

    let mut args: Vec<String> = vec![
        "--json".into(),
        "--with-filename".into(),
        "--line-number".into(),
        "--column".into(),
        "--color=never".into(),
    ];

    if options.case_sensitive {
        args.push("--case-sensitive".into());
    } else {
        args.push("--ignore-case".into());
    }

    // Per-file hit cap: always set so rg itself stops emitting early.
    args.push("--max-count".into());
    args.push(max_matches.to_string());

    if let Some(ctx) = options.context_lines {
        let capped = std::cmp::min(ctx, MAX_CONTEXT_LINES);
        if capped > 0 {
            args.push("--context".into());
            args.push(capped.to_string());
        }
    }

    if let Some(glob) = &options.glob {
        args.push("--glob".into());
        args.push(glob.clone());
    }

    args.push("--".into());
    args.push(options.pattern.clone());
    args.push(options.path.clone());

    let mut child = tokio::process::Command::new(&rg_bin)
        .args(&args)
        .env("LC_ALL", "en_US.UTF-8")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr_task = drain_stderr(child.stderr.take());
    let mut lines = tokio::io::BufReader::new(stdout).lines();

    let mut matches: Vec<SearchMatch> = Vec::new();
    let mut files_matched: HashSet<String> = HashSet::new();
    let mut truncated = false;

    // Track context lines by a key of (file_path, line_number) for proper matching.
    let mut pending_context: Vec<(String, u32, String)> = Vec::new();

    let context_lines_count = options.context_lines.unwrap_or(0) as u32;

    // Stream-parse rg's JSON lines as they arrive; stop reading (and kill
    // rg) as soon as the cap is reached so peak memory stays bounded.
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let data_type = parsed["type"].as_str().unwrap_or_default();

        match data_type {
            "context" => {
                let data = &parsed["data"];
                let file_path = data["path"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let line_num = data["line_number"].as_u64().unwrap_or(0) as u32;
                let text = match data["lines"]["text"].as_str() {
                    Some(t) => t.trim_end().to_string(),
                    None => continue,
                };
                pending_context.push((file_path, line_num, text));
            }
            "match" => {
                let data = &parsed["data"];
                let file_path = data["path"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                let line_num = data["line_number"].as_u64().unwrap_or(0) as u32;
                let submatch = match data["submatches"].as_array() {
                    Some(arr) if !arr.is_empty() => &arr[0],
                    _ => continue,
                };
                let col = submatch["start"].as_u64().unwrap_or(1) as u32;
                let line_text = match data["lines"]["text"].as_str() {
                    Some(t) => t.trim_end().to_string(),
                    None => continue,
                };
                let match_text = submatch["match"]["text"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();

                files_matched.insert(file_path.clone());

                // Associate context lines that appear before this match
                let mut context_before: Vec<String> = Vec::new();
                let context_after: Vec<String> = Vec::new();

                for (ctx_file, ctx_line, ctx_text) in &pending_context {
                    if ctx_file == &file_path
                        && *ctx_line < line_num
                        && *ctx_line >= line_num.saturating_sub(context_lines_count)
                    {
                        context_before.push(ctx_text.clone());
                    }
                }

                // Retrospectively populate after-context for the previous match
                // in this file. Ripgrep streams context lines after the match
                // they belong to; those lines only arrive once the next match
                // (or end-of-stream) is seen, so the previous match's
                // context_after must be filled in here. Lines that also fall
                // within this new match's before-window are left for the
                // before-window scan above (matches ripgrep's merged-window
                // single emission of overlapping context).
                let before_window_start = line_num.saturating_sub(context_lines_count);
                if let Some(prev) = matches.iter_mut().rev().find(|m| m.file_path == file_path) {
                    for (ctx_file, ctx_line, ctx_text) in &pending_context {
                        if ctx_file == &prev.file_path
                            && *ctx_line > prev.line_number
                            && *ctx_line <= prev.line_number + context_lines_count
                            && *ctx_line < before_window_start
                        {
                            prev.context_after.push(ctx_text.clone());
                        }
                    }
                }

                matches.push(SearchMatch {
                    file_path,
                    line_number: line_num,
                    column: col,
                    line_text,
                    match_text,
                    context_before,
                    context_after,
                });

                if matches.len() >= max_matches {
                    truncated = true;
                    // Stop pulling output; rg exits promptly once killed.
                    let _ = child.kill().await;
                    break;
                }
            }
            _ => {}
        }
    }

    let status = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr_bytes);

    // Skip the exit-status check when we killed rg ourselves on truncation:
    // the status then reflects the kill, not a search error.
    if !truncated && status != 0 && status != 1 {
        return Err(SearchError::RgExit {
            code: status,
            stderr: stderr.into_owned(),
        });
    }

    // Assign remaining context lines to after the last match
    if !matches.is_empty() && !pending_context.is_empty() {
        if let Some(last_match) = matches.last_mut() {
            for (ctx_file, ctx_line, ctx_text) in &pending_context {
                if ctx_file == &last_match.file_path
                    && *ctx_line > last_match.line_number
                    && *ctx_line <= last_match.line_number + context_lines_count
                {
                    last_match.context_after.push(ctx_text.clone());
                }
            }
        }
    }

    let total_matches = matches.len();
    Ok(SearchResult {
        matches,
        truncated,
        stats: SearchStats {
            files_matched: files_matched.len(),
            total_matches,
        },
    })
}

/// List files matching a pattern in a directory.
///
/// `max_results` is capped at [`SearchOptions::MAX_RESULTS`] (5000) to
/// bound memory usage and prevent DoS via multi-million file paths, and
/// ripgrep's output is streamed line-by-line (never fully buffered),
/// killing the process once the cap is reached.
pub async fn search_files(
    directory: &str,
    pattern: &str,
    glob: Option<&str>,
    max_results: Option<usize>,
) -> Result<Vec<String>, SearchError> {
    // Cap caller-supplied `max_results` to the same upper bound used for
    // `search_code`. Without this, an attacker could pass `usize::MAX` and
    // force the process to buffer an arbitrarily large result set.
    let max_results = max_results
        .map(|m| m.min(SearchOptions::MAX_RESULTS))
        .unwrap_or(500);

    let rg_bin = find_rg_binary().await?;

    let mut args: Vec<String> = vec!["--files".into(), "--color=never".into()];

    if let Some(g) = glob {
        args.push("--glob".into());
        args.push(g.to_string());
    }

    if !pattern.is_empty() {
        // Pattern is embedded inside a `--glob` value (`*{}*`), not passed
        // as a positional arg, so a leading dash can't be misinterpreted
        // as a flag. We still defang defensively to keep behavior
        // consistent with `search_code` for callers that share the same
        // pattern field.
        let safe_pattern = if pattern.starts_with('-') {
            format!("\\{}", pattern)
        } else {
            pattern.to_string()
        };
        args.push("--glob".into());
        args.push(format!("*{}*", safe_pattern));
    }

    args.push(directory.to_string());

    let mut child = tokio::process::Command::new(&rg_bin)
        .args(&args)
        .env("LC_ALL", "en_US.UTF-8")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;

    let stdout = child.stdout.take().expect("stdout piped");
    let stderr_task = drain_stderr(child.stderr.take());
    let mut lines = tokio::io::BufReader::new(stdout).lines();

    // Stream file paths and stop collecting (killing rg) at the cap so the
    // buffer never exceeds the cap, even on huge trees.
    let mut results: Vec<String> = Vec::new();
    let mut killed = false;
    while let Ok(Some(line)) = lines.next_line().await {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        results.push(line.to_string());
        if results.len() >= max_results {
            let _ = child.kill().await;
            killed = true;
            break;
        }
    }

    let status = child.wait().await.ok().and_then(|s| s.code()).unwrap_or(-1);
    let stderr_bytes = stderr_task.await.unwrap_or_default();
    let stderr = String::from_utf8_lossy(&stderr_bytes);

    if !killed && status != 0 && status != 1 {
        return Err(SearchError::RgExit {
            code: status,
            stderr: stderr.into_owned(),
        });
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts() -> SearchOptions {
        SearchOptions {
            pattern: "TODO".into(),
            path: ".".into(),
            glob: None,
            case_sensitive: false,
            max_results: None,
            context_lines: None,
        }
    }

    #[test]
    fn validate_caps_context_lines() {
        let mut o = opts();
        o.context_lines = Some(usize::MAX);
        o.validate();
        assert_eq!(o.context_lines, Some(SearchOptions::MAX_CONTEXT_LINES));
    }

    #[test]
    fn validate_caps_max_results() {
        let mut o = opts();
        o.max_results = Some(usize::MAX);
        o.validate();
        assert_eq!(o.max_results, Some(SearchOptions::MAX_RESULTS));
    }

    #[test]
    fn validate_strips_leading_dash() {
        let mut o = opts();
        o.pattern = "--help".into();
        o.validate();
        // Pattern must no longer start with a raw dash that would parse
        // as a flag if the `--` end-of-options separator were ever dropped.
        assert!(
            !o.pattern.starts_with('-'),
            "pattern still starts with dash: {}",
            o.pattern
        );
        assert!(o.pattern.starts_with('\\'));
    }

    #[test]
    fn validate_preserves_safe_values() {
        let mut o = opts();
        o.pattern = "fn main".into();
        o.context_lines = Some(5);
        o.max_results = Some(100);
        o.validate();
        assert_eq!(o.pattern, "fn main");
        assert_eq!(o.context_lines, Some(5));
        assert_eq!(o.max_results, Some(100));
    }

    #[test]
    fn validate_leaves_none_options_untouched() {
        let mut o = opts();
        o.validate();
        assert_eq!(o.context_lines, None);
        assert_eq!(o.max_results, None);
        assert_eq!(o.pattern, "TODO");
    }
}
