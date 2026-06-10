# FIX SPEC: Workstream 2 — Async/Concurrency & Mutex Poisoning

## Background
Audit findings identified widespread issues with `std::sync::Mutex` usage in async contexts, leading to deadlocks and ungraceful handling of lock poisoning.
- AGENT_01 C4: Lock Poisoning Not Handled in Critical Paths (Orchestrator)
- AGENT_01 H7: `OutputBuffer` deadlock risk with `std::sync::Mutex` for event emitters
- AGENT_02 1.3: `RwLock` contention under `Mutex` in `NotificationService`
- AGENT_05 [2/3]: `has()` silently recovering from poisoned mutex / Redundant retry in `AppState`
- AGENT_10 [3/11]: `CircuitBreaker` `std::sync::Mutex` across `await`

## Key Changes
1. Replace `std::sync::Mutex` with `parking_lot::Mutex` or `tokio::sync::Mutex` in:
   - `crates/athena-core/src/orchestrator.rs`
   - `crates/athena-core/src/output_buffer.rs`
   - `crates/athena-store/src/store.rs`
   - `frontend/src/utils/circuit_breaker.rs`
2. Ensure `OutputBuffer` event emitters do not hold locks during callbacks.
3. Standardize lock recovery: use `parking_lot` (no poisoning) with explicit error handling.
4. Fix `AppState::new()` redundant retry.

## Files to Modify
- `crates/athena-core/src/orchestrator.rs`
- `crates/athena-core/src/output_buffer.rs`
- `crates/athena-store/src/store.rs`
- `frontend/src/utils/circuit_breaker.rs`
- `src-tauri/src/state.rs`

## Verification
- Unit tests for concurrent access and lock behavior.
- Test for graceful recovery from simulated panics.
