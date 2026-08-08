//! Workspace tool implementations for [`ToolExecutor`].

use super::{ToolCallResult, ToolExecutor, ToolExecutorError, ToolInput};

impl ToolExecutor {
    pub(super) fn workspace_list(&self) -> Result<ToolCallResult, ToolExecutorError> {
        match self.store.get::<String>("workspaces") {
            Ok(Some(json)) => match serde_json::from_str::<serde_json::Value>(&json) {
                Ok(val) => {
                    let spaces = val.get("spaces").and_then(|s| s.as_array());
                    let list: Vec<serde_json::Value> = match spaces {
                        Some(arr) => arr
                            .iter()
                            .filter_map(|s| {
                                let id = s.get("id")?.as_str()?;
                                let name = s.get("name")?.as_str().unwrap_or(id);
                                Some(serde_json::json!({"id": id, "name": name}))
                            })
                            .collect(),
                        None => Vec::new(),
                    };
                    Ok(ToolCallResult {
                        text: serde_json::to_string(&list).unwrap_or_else(|_| "[]".to_string()),
                        is_error: None,
                    })
                }
                Err(e) => Ok(ToolCallResult {
                    text: format!("Error parsing workspaces: {}", e),
                    is_error: Some(true),
                }),
            },
            Ok(None) => Ok(ToolCallResult {
                text: "[]".to_string(),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error reading workspaces: {}", e),
                is_error: Some(true),
            }),
        }
    }

    pub(super) fn workspace_get_active(&self) -> Result<ToolCallResult, ToolExecutorError> {
        // `active_space_id` lives INSIDE the `workspaces` JSON blob — the
        // same place orchestrator.rs / kanban.rs read it from. The orphan
        // `workspace.active` key is no longer consulted.
        match self.store.get::<String>("workspaces") {
            Ok(Some(json)) => match serde_json::from_str::<serde_json::Value>(&json) {
                Ok(val) => {
                    let active_id = match val.get("active_space_id").and_then(|v| v.as_str()) {
                        Some(id) => id.to_string(),
                        None => {
                            return Ok(ToolCallResult {
                                text: "No active workspace".to_string(),
                                is_error: Some(true),
                            });
                        }
                    };
                    if let Some(spaces) = val.get("spaces").and_then(|s| s.as_array()) {
                        for s in spaces {
                            if s.get("id") == Some(&serde_json::Value::String(active_id.clone())) {
                                return Ok(ToolCallResult {
                                    text: serde_json::to_string(s).unwrap_or_default(),
                                    is_error: None,
                                });
                            }
                        }
                    }
                    Ok(ToolCallResult {
                        text: serde_json::json!({"id": active_id}).to_string(),
                        is_error: None,
                    })
                }
                Err(e) => Ok(ToolCallResult {
                    text: format!("Error parsing workspaces: {}", e),
                    is_error: Some(true),
                }),
            },
            Ok(None) => Ok(ToolCallResult {
                text: "No workspaces configured".to_string(),
                is_error: Some(true),
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Error reading workspaces: {}", e),
                is_error: Some(true),
            }),
        }
    }

    pub(super) fn workspace_switch(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let space_id = args
            .space_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("space_id".to_string()))?;

        // Readers (orchestrator, kanban, frontend) pull `active_space_id`
        // from INSIDE the `workspaces` JSON blob — not from the orphan
        // `workspace.active` key. Update the blob so the switch is visible.
        let json = match self.store.get::<String>("workspaces") {
            Ok(Some(j)) => j,
            Ok(None) => {
                return Ok(ToolCallResult {
                    text: "No workspaces configured".to_string(),
                    is_error: Some(true),
                });
            }
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error reading workspaces: {}", e),
                    is_error: Some(true),
                });
            }
        };

        let mut val: serde_json::Value = match serde_json::from_str(&json) {
            Ok(v) => v,
            Err(e) => {
                return Ok(ToolCallResult {
                    text: format!("Error parsing workspaces: {}", e),
                    is_error: Some(true),
                });
            }
        };

        // Validate the target space exists before committing the switch.
        if let Some(spaces) = val.get("spaces").and_then(|s| s.as_array()) {
            let exists = spaces
                .iter()
                .any(|s| s.get("id").and_then(|v| v.as_str()) == Some(space_id));
            if !exists {
                return Ok(ToolCallResult {
                    text: format!("Workspace '{}' not found", space_id),
                    is_error: Some(true),
                });
            }
        }

        // Mutate `active_space_id` inside the blob, then persist the whole
        // blob back to the store.
        let obj = match val.as_object_mut() {
            Some(map) => map,
            None => {
                return Ok(ToolCallResult {
                    text: "Workspaces blob is not a JSON object".to_string(),
                    is_error: Some(true),
                });
            }
        };
        obj.insert(
            "active_space_id".to_string(),
            serde_json::Value::String(space_id.to_string()),
        );

        let new_json = serde_json::to_string(&val).unwrap_or_default();
        match self.store.set_sync("workspaces", &new_json) {
            Ok(()) => Ok(ToolCallResult {
                text: format!("Switched to workspace {}", space_id),
                is_error: None,
            }),
            Err(e) => Ok(ToolCallResult {
                text: format!("Failed to switch workspace: {}", e),
                is_error: Some(true),
            }),
        }
    }
}
