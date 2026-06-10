# FIX SPEC: Workstream 3 — Resource Bounds & DoS Prevention

## Background
The audit identified multiple areas where unbounded resource growth can lead to OOM or DoS.
- AGENT_04 [Critical #2]: Unbounded `coalesce_buf` in `pty_read_loop`
- AGENT_08 [C1]: Unbounded agent output buffer growth
- AGENT_02 [2.1/3.1]: Unbounded TCP channel and writer thread leaks
- AGENT_03 [6/7]: Unbounded `context_lines` / `max_results`
- AGENT_06 [PL-04]: Plugin manifest read without file size limits
- AGENT_09 [C-1/C-3]: Event listener accumulation

## Key Changes
1. **Terminal `coalesce_buf`**: Add a 1MB hard cap in `src-tauri/src/commands/mod.rs`; flush when exceeded.
2. **Agent Output Buffers**: Prune `AgentOutputInfo` entries; cap `OutputLine.text` length in `frontend/src/stores/agent_output.rs`.
3. **Bounded Channels**: Replace `mpsc::channel` with `sync_channel(1024)` in `crates/athena-core/src/agent_comms.rs`; implement backpressure.
4. **Search Limits**: Cap `context_lines` at 100 and `max_results` at 5000 in `crates/athena-core/src/search.rs`.
5. **Plugin Limits**: Limit manifest file reads to 1MB in `crates/athena-plugins/src/lib.rs`.
6. **Event Listeners**: Ensure cleanup on unmount in `frontend/src/tauri_bridge.rs`.

## Files to Modify
- `src-tauri/src/commands/mod.rs`
- `frontend/src/stores/agent_output.rs`
- `crates/athena-core/src/agent_comms.rs`
- `crates/athena-core/src/search.rs`
- `crates/athena-plugins/src/lib.rs`
- `frontend/src/tauri_bridge.rs`

## Verification
- Unit tests for resource limits and overflow behavior.
- Stress tests for high-output scenarios.
