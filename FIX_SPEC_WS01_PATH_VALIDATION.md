# FIX SPEC: Workstream 1 — Path & Command Injection Security

## Background
Multiple audit findings identified path traversal vulnerabilities and command injection risks across the backend. This workstream addresses:
- AGENT_01 C1: Path Traversal in Tool Executor
- AGENT_06 FS-01/FS-02: `ensure_within_home` TOCTOU and Symlink Bypass weaknesses
- AGENT_07 Finding 1: `validate_path` Write Path Missing Canonicalization
- AGENT_03 Finding 1/2: Command Injection via Unsanitized Search Pattern/Path
- AGENT_07 Finding 3/5: Unvalidated path in search/swarm commands
- AGENT_06 PL-05: `validate_hook_script` `..` bypass

## Key Changes
1. Create `crates/athena-fs/src/path_validator.rs` with a robust `PathValidator` that:
   - Canonicalizes paths using `std::fs::canonicalize()`.
   - Verifies the canonical path `starts_with` the canonicalized workspace root.
   - Rejects `..` components before canonicalization.
   - Checks symlink depth to prevent escapes.
2. Update `crates/athena-fs/src/lib.rs` to use `PathValidator`.
3. Update `src-tauri/src/commands/mod.rs` to use `PathValidator`.
4. Update `crates/athena-core/src/tool_executor.rs` to use `PathValidator` for `fs_read_file`, `fs_list_dir`, `fs_search`, etc.
5. Sanitize command arguments passed to `Command::new` in `search.rs` and `tool_executor.rs`.
6. Fix `validate_hook_script` to reject `..` at end of path.

## Files to Modify
- `crates/athena-fs/src/lib.rs`
- `src-tauri/src/commands/mod.rs`
- `crates/athena-core/src/tool_executor.rs`
- NEW: `crates/athena-fs/src/path_validator.rs`
- `src-tauri/src/commands/search.rs`
- `src-tauri/src/commands/swarm.rs`
- `src-tauri/src/commands/plugin_host.rs`
- `crates/athena-plugins/src/lib.rs`

## Verification
- Unit tests for path traversal cases (.., symlinks, non-existent paths).
- Test that unsanitized commands are rejected.
