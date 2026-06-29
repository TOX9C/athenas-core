# Deep Dive Agent Missions

## Agent 1: Core Logic & Orchestration
**Scope:** `crates/athena-core/src/`
**Files:** orchestrator.rs, types.rs, tool_executor.rs, output_capture.rs, output_buffer.rs, kanban.rs
**Focus:** Logic bugs, async flow, state management, race conditions, resource leaks
**Deliverable:** AGENT_01_CORE_ORCHESTRATION.md

## Agent 2: Agent Comms & Swarm
**Scope:** `crates/athena-core/src/`
**Files:** agent_comms.rs, swarm.rs, notification.rs
**Focus:** Message passing, concurrency, deadlocks, protocol issues, memory leaks
**Deliverable:** AGENT_02_AGENT_COMMS.md

## Agent 3: MCP & Search
**Scope:** `crates/athena-core/src/`
**Files:** mcp.rs, search.rs, shell_hooks.rs, shell_integration.rs
**Focus:** External API integration, error handling, security, command injection
**Deliverable:** AGENT_03_MCP_SEARCH.md

## Agent 4: Terminal & Input
**Scope:** `crates/athena-terminal/src/`
**Files:** session.rs, input/*.rs, lib.rs
**Focus:** PTY handling, input parsing, escape sequences, resource management
**Deliverable:** AGENT_04_TERMINAL.md

## Agent 5: Store & State
**Scope:** `crates/athena-store/src/` and `src-tauri/src/state.rs`
**Files:** store.rs, session.rs, types.rs, tests.rs, state.rs
**Focus:** SQLite operations, session lifecycle, data integrity, locking
**Deliverable:** AGENT_05_STORE_STATE.md

## Agent 6: Filesystem & Browser
**Scope:** `crates/athena-fs/src/` and `crates/athena-browser/src/`
**Files:** lib.rs (fs and browser)
**Focus:** Path traversal, file operations, sandboxing, browser security
**Deliverable:** AGENT_06_FS_BROWSER.md

## Agent 7: Tauri Commands
**Scope:** `src-tauri/src/commands/`
**Files:** mod.rs, athena.rs, fs.rs, store.rs, session.rs, swarm.rs, agent.rs, etc.
**Focus:** Command handler bugs, input validation, error propagation, IPC security
**Deliverable:** AGENT_07_TAURI_COMMANDS.md

## Agent 8: Frontend Core & Stores
**Scope:** `frontend/src/`
**Files:** lib.rs, stores/*.rs, tauri_bridge.rs
**Focus:** State management, signal bugs, async handling, memory leaks in Dioxus
**Deliverable:** AGENT_08_FRONTEND_CORE.md

## Agent 9: UI Components
**Scope:** `frontend/src/components/`
**Files:** All component files excluding right_sidebar/
**Focus:** Component lifecycle, event handling, potential panics, WASM edge cases
**Deliverable:** AGENT_09_UI_COMPONENTS.md

## Agent 10: Utils & Cross-Cutting
**Scope:** `frontend/src/utils/`, `frontend/src/types/`, config files
**Files:** All utility files, type definitions, Cargo.toml, tauri.conf.json
**Focus:** Helper function bugs, type safety, configuration issues, dependency security
**Deliverable:** AGENT_10_UTILS_AND_CONFIG.md
