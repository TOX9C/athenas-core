//! Tool-call dispatch adapter for the orchestrator.

use super::orchestrator_stream::MAX_CONCURRENT_TOOLS;
use super::{json_to_tool_input, AthenaOrchestrator, OrchestratorError};
use futures_util::StreamExt;

impl AthenaOrchestrator {
    pub(super) async fn execute_tool(
        &self,
        name: &str,
        input: &serde_json::Value,
    ) -> (String, bool) {
        self.execute_tool_core(name, input, None).await
    }

    /// Install the stream request context on the executor's event sender.
    /// Called once per turn (or once per batch of concurrent tool calls);
    /// the executor clears it when the last call in the batch finishes.
    pub(super) fn install_request_context(&self, request_id: &str, session_id: &str) {
        if let Some(executor) = self.tool_executor.as_ref() {
            executor.read().set_request_context(request_id, session_id);
        }
    }

    /// Clear the stream request context after a turn or tool batch ends.
    /// Mirrors the old per-call `_with_context` clear: pending ask_user
    /// questions for the request are resolved with an error so they cannot
    /// outlive the request.
    pub(super) fn clear_request_context(&self) {
        if let Some(executor) = self.tool_executor.as_ref() {
            executor.read().clear_request_context();
        }
    }

    /// Execute a batch of independent tool calls from one assistant turn
    /// concurrently, preserving input order in the output.
    ///
    /// All calls in a turn share the same `(request_id, session_id)`, so the
    /// request context is installed once for the whole batch instead of
    /// per-call — this is what makes concurrency safe: the executor mutex
    /// serializes the *dispatch*, but each call's blocking work runs on its
    /// own `spawn_blocking` thread. Cancellation is checked per call, so a
    /// cancel mid-batch fails the not-yet-started calls fast.
    ///
    /// Concurrency is bounded by `buffered(MAX_CONCURRENT_TOOLS)`; because
    /// `buffered` (not `buffer_unordered`) is used, results come back in
    /// input order and the `tool_use_id` pairing contract holds without a
    /// reassembly pass.
    #[allow(clippy::type_complexity)]
    pub(super) async fn execute_tool_batch(
        &self,
        calls: Vec<(String, serde_json::Value)>,
        request_id: &str,
        session_id: &str,
        cancel: &tokio_util::sync::CancellationToken,
    ) -> Vec<Result<(String, bool), OrchestratorError>> {
        // Safe under the conversation lock: whole turns never overlap, so
        // no other batch can install a different context between this
        // install and the clear below.
        self.install_request_context(request_id, session_id);
        let results: Vec<Result<(String, bool), OrchestratorError>> =
            futures_util::stream::iter(calls.into_iter().map(|(name, input)| {
                let cancel = cancel.clone();
                async move {
                    tokio::select! {
                        _ = cancel.cancelled() => Err(OrchestratorError::UserCancellation),
                        result = self.execute_tool_core(&name, &input, Some(request_id)) => {
                            Ok(result)
                        }
                    }
                }
            }))
            .buffered(MAX_CONCURRENT_TOOLS)
            .collect::<Vec<_>>()
            .await;
        self.clear_request_context();
        results
    }

    /// Shared body of [`Self::execute_tool_batch`]: deserialize,
    /// spawn_blocking, map errors.
    #[allow(clippy::type_complexity)]
    async fn execute_tool_core(
        &self,
        name: &str,
        input: &serde_json::Value,
        request_id: Option<&str>,
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
        // Owned so it can cross into `spawn_blocking`.
        let request_id = request_id.map(str::to_string);
        // Read guard: concurrent batch calls all hold read guards at once —
        // this is what makes F6 concurrency real. Context set/clear NEVER
        // happens here (it would let the first finisher wipe the slot for
        // siblings); it is owned by install_request_context/clear around the
        // whole turn or batch.
        match tokio::task::spawn_blocking(move || {
            let executor = executor_arc.read();
            if request_id
                .as_deref()
                .is_some_and(|rid| executor.request_cancelled(rid))
            {
                return Err(crate::tool_executor::ToolExecutorError::Cancelled);
            }
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
