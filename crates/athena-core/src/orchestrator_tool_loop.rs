//! Tool-call dispatch adapter for the orchestrator.

use super::{json_to_tool_input, AthenaOrchestrator};

impl AthenaOrchestrator {
    pub(super) async fn execute_tool(
        &self,
        name: &str,
        input: &serde_json::Value,
    ) -> (String, bool) {
        let tool_input = match json_to_tool_input(input) {
            Ok(ti) => ti,
            Err(e) => {
                return (
                    format!("Failed to deserialize tool input for '{}': {}", name, e),
                    true,
                )
            }
        };

        let Some(executor_arc) = self.tool_executor.clone() else {
            return (
                format!(
                    "Tool '{}' was requested but no tool executor is configured. \
                     Pass an executor via AthenaOrchestrator::new_with_executor().",
                    name
                ),
                true,
            );
        };

        let name = name.to_string();
        match tokio::task::spawn_blocking(move || {
            let executor = executor_arc.lock();
            executor.execute_tool_call(&name, &tool_input)
        })
        .await
        {
            Ok(Ok(result)) => (result.text, result.is_error.unwrap_or(false)),
            Ok(Err(e)) => (format!("Tool execution error: {}", e), true),
            Err(join_err) => (format!("Tool execution task panicked: {}", join_err), true),
        }
    }
}
