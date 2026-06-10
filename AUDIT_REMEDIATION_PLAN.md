# Audit Remediation Plan

## Phase 1: Critical Security & Stability Fixes (P0)
Goals: Fix all Critical findings, most High findings.

### Workstream 1: Backend Security (Path & Command Injection)
- Fix path validation in `tool_executor.rs`, `athena-fs/src/lib.rs`, and `src-tauri/src/commands/mod.rs` to prevent traversal.
- Sanitize command arguments passed to shell execution to prevent injection.
- Implement a unified `PathValidator` utility.

### Workstream 2: Async/Concurrency Fix (Mutex & Lock Poisoning)
- Replace `std::sync::Mutex` with `parking_lot::Mutex` or `tokio::sync::Mutex` in `orchestrator.rs`, `output_buffer.rs`, and `store.rs`.
- Fix lock poisoning recovery logic.

### Workstream 3: Resource Bounds & DoS Prevention
- Add hard caps on `coalesce_buf` in the PTY read loop.
- Prune `AgentOutputInfo` entries and cap `OutputLine` strings.
- Switch unbounded channels to bounded `sync_channel`.

### Workstream 4: Frontend Memory Leaks & Event Listeners
- Refactor `tauri_bridge.rs` to avoid `Closure.forget()`.
- Implement `use_drop` cleanup for all Tauri event listeners.
- Implement debounced/single-pending save for `WorkspaceState`.

### Workstream 5: API Key & Secret Handling
- Integrate `secrecy::SecretString` for API keys.
- Redact keys in error responses and logs.
- Ensure API keys are never passed to the frontend.

## Verification
- Unit tests for path validation and command sanitization.
- Integration tests for concurrency and resource bounds.
- Memory leak tests for frontend.
