use crate::types::*;
use serde_json::Value;
use std::collections::HashSet;
use std::path::PathBuf;

/// Hard limits for search parameters to prevent DoS.
const MAX_CONTEXT_LINES: usize = 100;
const MAX_RESULTS: usize = 5000;

/// Locate the ripgrep binary, falling back to common system paths.
pub(crate) async fn find_rg_binary() -> Result<PathBuf, SearchError> {
    let candidates = if cfg!(windows) {
        vec!["rg.exe"]
    } else {
        vec![
            "rg",
            "/usr/local/bin/rg",
            "/opt/homebrew/bin/rg",
            "/usr/bin/rg",
        ]
    };

    for candidate in &candidates {
        let candidate_path = PathBuf::from(candidate);
        if candidate_path.exists() {
            return Ok(candidate_path);
        }
    }

    // Try to find via `which` as last resort
    let which_result = if cfg!(windows) {
        tokio::process::Command::new("cmd")
            .args(["/c", "where", "rg"])
            .output()
            .await
    } else {
        tokio::process::Command::new("which")
            .arg("rg")
            .output()
            .await
    };

    if let Ok(output) = which_result {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err(SearchError::RgNotFound)
}

/// Errors that can occur during search operations.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error(
        "ripgrep binary not found. Install it via your package manager (e.g. brew install ripgrep)"
    )]
    RgNotFound,
    #[error("Failed to spawn ripgrep: {0}")]
    SpawnError(#[from] std::io::Error),
    #[error("ripgrep process exited with code {code}: {stderr}")]
    RgExit { code: i32, stderr: String },
    #[error("JSON parse error: {0}")]
    JsonParseError(#[from] serde_json::Error),
}

/// Search code using ripgrep.
///
/// Spawns the `rg` binary with JSON output mode, parses the results,
/// and returns a structured `SearchResult`.
pub async fn search_code(options: &SearchOptions) -> Result<SearchResult, SearchError> {
    let rg_bin = find_rg_binary().await?;

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

    if let Some(max) = options.max_results {
        let capped = std::cmp::min(max, MAX_RESULTS);
        args.push("--max-count".into());
        args.push(capped.to_string());
    }

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

    let output = tokio::process::Command::new(&rg_bin)
        .args(&args)
        .env("LC_ALL", "en_US.UTF-8")
        .output()
        .await?;

    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if status != 0 && status != 1 {
        return Err(SearchError::RgExit {
            code: status,
            stderr: stderr.into_owned(),
        });
    }

    let mut matches: Vec<SearchMatch> = Vec::new();
    let mut files_matched: HashSet<String> = HashSet::new();
    let mut truncated = false;

    // Track context lines by a key of (file_path, line_number) for proper matching.
    let mut pending_context: Vec<(String, u32, String)> = Vec::new();

    for line in stdout.lines() {
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

                let context_lines_count = options.context_lines.unwrap_or(0) as u32;
                for (ctx_file, ctx_line, ctx_text) in &pending_context {
                    if ctx_file == &file_path
                        && *ctx_line < line_num
                        && *ctx_line >= line_num.saturating_sub(context_lines_count)
                    {
                        context_before.push(ctx_text.clone());
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

                if let Some(max) = options.max_results {
                    if matches.len() >= max {
                        truncated = true;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    // Assign remaining context lines to after the last match
    if !matches.is_empty() && !pending_context.is_empty() {
        if let Some(last_match) = matches.last_mut() {
            let context_lines_count = options.context_lines.unwrap_or(0) as u32;
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

/// Locate the ripgrep binary synchronously.
///
/// **Deprecated**: Spawns a blocking `std::process::Command`. Prefer the
/// async [`find_rg_binary`] which uses `tokio::process::Command`.
#[deprecated(
    since = "0.1.0",
    note = "Spawns a blocking std::process::Command; use the async `find_rg_binary` instead"
)]
pub fn find_rg_binary_sync() -> Result<PathBuf, SearchError> {
    let candidates = if cfg!(windows) {
        vec!["rg.exe"]
    } else {
        vec![
            "rg",
            "/usr/local/bin/rg",
            "/opt/homebrew/bin/rg",
            "/usr/bin/rg",
        ]
    };

    for candidate in &candidates {
        let candidate_path = PathBuf::from(candidate);
        if candidate_path.exists() {
            return Ok(candidate_path);
        }
    }

    // Try to find via `which` as last resort
    let which_result = if cfg!(windows) {
        std::process::Command::new("cmd")
            .args(["/c", "where", "rg"])
            .output()
    } else {
        std::process::Command::new("which").arg("rg").output()
    };

    if let Ok(output) = which_result {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }

    Err(SearchError::RgNotFound)
}

/// Synchronous version of `search_code` — runs ripgrep and parses results.
///
/// **Deprecated**: Spawns a blocking `std::process::Command` which can stall
/// the Tokio worker thread. Prefer the async [`search_code`], which uses
/// `tokio::process::Command` and integrates with the Tokio runtime.
#[deprecated(
    since = "0.1.0",
    note = "Spawns a blocking std::process::Command; use the async `search_code` instead"
)]
pub fn search_code_sync(options: &SearchOptions) -> Result<SearchResult, SearchError> {
    #[allow(deprecated)]
    let rg_bin = find_rg_binary_sync()?;

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

    if let Some(max) = options.max_results {
        let capped = std::cmp::min(max, MAX_RESULTS);
        args.push("--max-count".into());
        args.push(capped.to_string());
    }

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

    let output = std::process::Command::new(&rg_bin)
        .args(&args)
        .env("LC_ALL", "en_US.UTF-8")
        .output()?;

    let status = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if status != 0 && status != 1 {
        return Err(SearchError::RgExit {
            code: status,
            stderr: stderr.into_owned(),
        });
    }

    let mut matches: Vec<SearchMatch> = Vec::new();
    let mut files_matched: HashSet<String> = HashSet::new();
    let mut truncated = false;
    let mut pending_context: Vec<(String, u32, String)> = Vec::new();

    for line in stdout.lines() {
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

                let mut context_before: Vec<String> = Vec::new();
                let context_after: Vec<String> = Vec::new();

                let context_lines_count = options.context_lines.unwrap_or(0) as u32;
                for (ctx_file, ctx_line, ctx_text) in &pending_context {
                    if ctx_file == &file_path
                        && *ctx_line < line_num
                        && *ctx_line >= line_num.saturating_sub(context_lines_count)
                    {
                        context_before.push(ctx_text.clone());
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

                if let Some(max) = options.max_results {
                    if matches.len() >= max {
                        truncated = true;
                        break;
                    }
                }
            }
            _ => {}
        }
    }

    if !matches.is_empty() && !pending_context.is_empty() {
        if let Some(last_match) = matches.last_mut() {
            let context_lines_count = options.context_lines.unwrap_or(0) as u32;
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
pub async fn search_files(
    directory: &str,
    pattern: &str,
    glob: Option<&str>,
    max_results: Option<usize>,
) -> Result<Vec<String>, SearchError> {
    let rg_bin = find_rg_binary().await?;

    let mut args: Vec<String> = vec!["--files".into(), "--color=never".into()];

    if let Some(g) = glob {
        args.push("--glob".into());
        args.push(g.to_string());
    }

    if !pattern.is_empty() {
        args.push("--glob".into());
        args.push(format!("*{}*", pattern));
    }

    args.push(directory.to_string());

    let output = tokio::process::Command::new(&rg_bin)
        .args(&args)
        .env("LC_ALL", "en_US.UTF-8")
        .output()
        .await?;

    let status = output.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if status != 0 && status != 1 {
        return Err(SearchError::RgExit {
            code: status,
            stderr: stderr.into_owned(),
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let results: Vec<String> = stdout
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(max_results.unwrap_or(500))
        .map(|s| s.to_string())
        .collect();

    Ok(results)
}
