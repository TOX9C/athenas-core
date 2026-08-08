//! Agent and terminal tool implementations for [`ToolExecutor`].

use super::{now_ms, ToolCallResult, ToolExecutor, ToolExecutorError, ToolInput};
use crate::tool_schema::build_agent_command;
use uuid::Uuid;

impl ToolExecutor {
    pub(super) fn launch_builtin_agent(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let agent_type = args.agent_type.as_deref().unwrap_or("claude");
        let agent_count = args.agent_count.unwrap_or(1);
        let agent_command = build_agent_command(agent_type, args.task_prompt.as_deref());

        for _ in 0..agent_count {
            let id = format!("agent-{}", Uuid::new_v4());
            self.event_sender
                .agent_spawned(&id, agent_type, &agent_command);
        }

        Ok(ToolCallResult {
            text: format!("Done, launched {} {} agents.", agent_count, agent_type),
            is_error: None,
        })
    }

    pub(super) fn launch_custom_agent(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let command = args
            .command
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("command".to_string()))?;
        // Strict allowlist: commands must be explicitly permitted
        let allowed: Vec<String> = std::env::var("ATHENA_COMMAND_ALLOWLIST")
            .ok()
            .map(|s| s.split(',').map(|c| c.trim().to_string()).collect())
            .unwrap_or_default();
        if !allowed.contains(&command.trim().to_string()) {
            return Err(ToolExecutorError::Notification(format!(
                "Command not in allowlist: '{}'",
                command
            )));
        }
        let agent_count = args.agent_count.unwrap_or(1);

        for _ in 0..agent_count {
            let id = format!("custom-agent-{}", Uuid::new_v4());
            self.event_sender.agent_spawned(&id, "custom", command);
        }

        Ok(ToolCallResult {
            text: format!("Done, launched {} custom agents.", agent_count),
            is_error: None,
        })
    }

    pub(super) fn close_terminals(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        if let Some(ref pane_ids) = args.pane_ids {
            self.event_sender.close_panes(pane_ids);
            Ok(ToolCallResult {
                text: format!("Closed {} terminal(s).", pane_ids.len()),
                is_error: None,
            })
        } else {
            Ok(ToolCallResult {
                text: "Closed 0 terminal(s).".to_string(),
                is_error: None,
            })
        }
    }

    pub(super) fn run_command_in_terminals(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let pane_ids = args.pane_ids.as_deref().unwrap_or(&[]);
        let command = args.command.as_deref().unwrap_or("");

        if !pane_ids.is_empty() && !command.is_empty() {
            for pane_id in pane_ids {
                self.event_sender.pty_write(pane_id, command);
                self.event_sender.pty_write(pane_id, "\r");
            }
        }

        Ok(ToolCallResult {
            text: format!("Sent command to {} terminal(s).", pane_ids.len()),
            is_error: None,
        })
    }

    pub(super) fn read_agent_output(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let pane_id = args
            .pane_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("pane_id".to_string()))?;

        let opts = crate::output_buffer::GetOutputOptions {
            limit: args.limit.or(Some(100)),
            since_line: args.since_line,
            since_time: args.since_time,
            offset: None,
            raw: None,
        };

        let lines = self.output_buffer.get_output(pane_id, Some(&opts));

        if lines.is_empty() {
            return Ok(ToolCallResult {
                text: format!(
                    "No output captured for pane '{}'. The pane may not exist or has not produced output yet.",
                    pane_id
                ),
                is_error: None,
            });
        }

        let formatted: String = lines
            .iter()
            .map(|l| format!("[{}] {}", l.line_num, l.text))
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolCallResult {
            text: formatted,
            is_error: None,
        })
    }

    pub(super) fn list_agents(&self) -> Result<ToolCallResult, ToolExecutorError> {
        let panes = self.output_buffer.get_agent_list();
        let sessions = self.agent_comms.get_agent_sessions();

        if panes.is_empty() && sessions.is_empty() {
            return Ok(ToolCallResult {
                text: "No agents currently running.".to_string(),
                is_error: None,
            });
        }

        let mut parts: Vec<String> = Vec::new();

        if !panes.is_empty() {
            parts.push("Terminal Panes:".to_string());
            for p in &panes {
                parts.push(format!(
                    "  {} ({}) — {} lines, last activity: {}",
                    p.pane_id,
                    p.agent_type,
                    p.line_count,
                    chrono::DateTime::from_timestamp_millis(p.last_activity_at as i64)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                ));
            }
        }

        if !sessions.is_empty() {
            parts.push("Agent Sessions:".to_string());
            for s in &sessions {
                parts.push(format!(
                    "  {} [{}] — plugin: {}, connected: {}",
                    s.agent_id,
                    s.status,
                    s.plugin_id,
                    chrono::DateTime::from_timestamp_millis(s.connected_at as i64)
                        .map(|dt| dt.to_rfc3339())
                        .unwrap_or_default()
                ));
            }
        }

        Ok(ToolCallResult {
            text: parts.join("\n"),
            is_error: None,
        })
    }

    pub(super) fn check_agent_status(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let agent_id = args
            .agent_id
            .as_deref()
            .ok_or_else(|| ToolExecutorError::MissingParam("agent_id".to_string()))?;

        let pane_info = self.output_buffer.get_pane_buffer_info(agent_id);
        let sessions = self.agent_comms.get_agent_sessions();
        let session = sessions
            .iter()
            .find(|s| s.agent_id == agent_id || s.id == agent_id);

        if pane_info.is_none() && session.is_none() {
            return Ok(ToolCallResult {
                text: format!("No agent found with ID '{}'.", agent_id),
                is_error: None,
            });
        }

        let mut parts: Vec<String> = Vec::new();

        if let Some(info) = &pane_info {
            parts.push(format!("Pane: {}", info.pane_id));
            parts.push(format!("Type: {}", info.agent_type));
            parts.push(format!(
                "Lines: {} ({} total)",
                info.line_count, info.total_lines
            ));
            parts.push(format!("Size: {} bytes", info.total_bytes));
            parts.push(format!(
                "Created: {}",
                chrono::DateTime::from_timestamp_millis(info.created_at as i64)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            ));
            parts.push(format!(
                "Last Activity: {}",
                chrono::DateTime::from_timestamp_millis(info.last_activity_at as i64)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            ));
            let is_active = now_ms().saturating_sub(info.last_activity_at) < 30_000;
            parts.push(format!(
                "Status: {}",
                if is_active { "active" } else { "idle" }
            ));
        }

        if let Some(s) = session {
            parts.push(format!("Session: {}", s.id));
            parts.push(format!("Agent ID: {}", s.agent_id));
            parts.push(format!("Connection Status: {}", s.status));
            parts.push(format!(
                "Connected: {}",
                chrono::DateTime::from_timestamp_millis(s.connected_at as i64)
                    .map(|dt| dt.to_rfc3339())
                    .unwrap_or_default()
            ));
        }

        let pty_connected = self.event_sender.has_session(agent_id);
        parts.push(format!("PTY Connected: {}", pty_connected));

        Ok(ToolCallResult {
            text: parts.join("\n"),
            is_error: None,
        })
    }

    pub(super) fn prompt_agent(
        &self,
        args: &ToolInput,
    ) -> Result<ToolCallResult, ToolExecutorError> {
        let pane_id = match args.pane_id {
            Some(ref id) => id,
            None => {
                return Ok(ToolCallResult {
                    text: "Missing pane_id or prompt.".to_string(),
                    is_error: None,
                })
            }
        };
        let prompt = match args.prompt {
            Some(ref p) => p,
            None => {
                return Ok(ToolCallResult {
                    text: "Missing pane_id or prompt.".to_string(),
                    is_error: None,
                })
            }
        };

        if !self.event_sender.has_session(pane_id) {
            return Ok(ToolCallResult {
                text: format!("No active PTY session for pane '{}'.", pane_id),
                is_error: None,
            });
        }

        self.event_sender.pty_write(pane_id, prompt);
        self.event_sender.pty_write(pane_id, "\r");

        Ok(ToolCallResult {
            text: format!("Prompt sent to {}.", pane_id),
            is_error: None,
        })
    }
}
