//! Filesystem tool implementations for [`ToolExecutor`].

use super::{ToolCallResult, ToolExecutor, ToolExecutorError, ToolInput};
use athena_fs::path_validator::PathValidator;
use std::path::{Path, PathBuf};

impl ToolExecutor {
    pub(super) fn get_workspace_root(&self) -> Result<PathBuf, ToolExecutorError> {
        if let Some(ref root) = self.workspace_root_override {
            return Ok(root.clone());
        }
        std::env::current_dir()
            .and_then(|p| p.canonicalize())
            .map_err(|e| {
                ToolExecutorError::Io(std::io::Error::other(format!(
                    "Failed to get workspace root: {}",
                    e
                )))
            })
    }

    pub(super) fn validate_path(&self, path: &str) -> Result<PathBuf, ToolExecutorError> {
        let root = self.get_workspace_root()?;
        let path = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            root.join(path)
        };
        let validator = PathValidator::new(&root).map_err(|e| {
            ToolExecutorError::PathTraversal(format!("failed to initialize path validator: {}", e))
        })?;
        // TODO: opt-in allowlist for extra roots
        validator
            .validate(&path)
            .map_err(|e| ToolExecutorError::PathTraversal(e.to_string()))
    }

    pub(super) fn fs_read_file(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let path = args
            .path
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("path".to_string()))?;

        let validated = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Invalid path '{}': {}", path, e),
                    is_error: Some(true),
                })
            }
        };

        if !validated.exists() {
            return Ok(ToolCallResult {
                text: format!("File not found: {}", validated.display()),
                is_error: Some(true),
            });
        }

        match std::fs::read_to_string(&validated) {
            Ok(contents) => Ok(ToolCallResult {
                text: contents,
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Failed to read file '{}': {}", validated.display(), e),
                is_error: Some(true),
            }),
        }
    }

    pub(super) fn fs_list_dir(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let path = args
            .path
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("path".to_string()))?;

        let validated = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Invalid path '{}': {}", path, e),
                    is_error: Some(true),
                })
            }
        };

        if !validated.exists() {
            return Ok(ToolCallResult {
                text: format!("Directory not found: {}", validated.display()),
                is_error: Some(true),
            });
        }

        let entries = match std::fs::read_dir(&validated) {
            Ok(dir) => dir,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Failed to read directory '{}': {}", validated.display(), e),
                    is_error: Some(true),
                })
            }
        };

        let mut results: Vec<serde_json::Value> = Vec::new();
        for entry in entries {
            match entry {
                Ok(e) => {
                    let name = e.file_name().to_string_lossy().to_string();
                    let path = e.path().to_string_lossy().to_string();
                    let is_dir = match e.file_type() {
                        Ok(ft) => ft.is_dir(),
                        Err(_) => false,
                    };
                    results.push(serde_json::json!({
                        "name": name,
                        "path": path,
                        "is_dir": is_dir,
                    }));
                }
                Err(_) => continue,
            }
        }

        let json = match serde_json::to_string(&results) {
            Ok(j) => j,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error serializing directory entries: {}", e),
                    is_error: Some(true),
                })
            }
        };

        Ok(ToolCallResult {
            text: json,
            is_error: None,
        })
    }

    pub(super) fn fs_search(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError> {
        let pattern = args
            .pattern
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("pattern".to_string()))?;
        // `path` is optional in the tool schema; default to the workspace root.
        let path = args.path.as_deref().unwrap_or(".");

        let validated = match self.validate_path(path) {
            Ok(p) => p,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Invalid path '{}': {}", path, e),
                    is_error: Some(true),
                })
            }
        };

        let options = crate::types::SearchOptions {
            pattern: pattern.to_string(),
            path: validated.to_string_lossy().to_string(),
            glob: None,
            case_sensitive: false,
            max_results: Some(50),
            context_lines: Some(2),
        };

        // Drive the async `search_code` on the current Tokio runtime.
        // `fs_search` is sync because `execute_tool_call` dispatches it
        // without an `await` in the Tauri command handler (`spawn_blocking`
        // closure) and the orchestrator lock-guard chain. We must keep the
        // signature sync to avoid cascading `async`/`Send` changes, so we
        // bridge via `Handle::current().block_on`. The runtime is always
        // available in practice (Tauri main + MCP server are both async),
        // and this replaces a `std::process::Command` that would block
        // the worker thread.
        let search_result =
            tokio::runtime::Handle::current().block_on(crate::search::search_code(&options));

        match search_result {
            Ok(result) => {
                let json = match serde_json::to_string(&result) {
                    Ok(j) => j,
                    Err(e) => {
                        return Ok(ToolCallResult {
                            text: format!("Error serializing search results: {}", e),
                            is_error: Some(true),
                        })
                    }
                };
                Ok(ToolCallResult {
                    text: json,
                    is_error: None,
                })
            }
            Err(e) => Ok(ToolCallResult {
                text: format!("Search failed: {e}. Check the path and search pattern, then try again."),
                is_error: Some(true),
            }),
        }
    }
}
