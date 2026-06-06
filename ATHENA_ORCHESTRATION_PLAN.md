# Athena Orchestrator: Production Roadmap v2

> **For the agent executing this plan:** Read the entire document before touching any code.
> Each phase has explicit entry conditions. Do not start a phase until its prerequisites are met.
> When a decision is marked **DECIDED**, do not re-open it — implement as written.

---

## Vision

Athena becomes a true orchestration layer — a standalone LLM client that understands the full
state of the Athena's Core desktop environment and can act on it through tools. Athena never
assumes, always confirms, and operates with full workspace context awareness.

---

## Core Principles

1. **Athena is its own brain** — No harness, no wrapper. Direct HTTP calls to any OpenAI-compatible
   or Anthropic API with a user-provided API key + base URL.
2. **Workspace-aware** — Athena always knows which workspace the user is on. All agent operations
   are scoped to the active workspace unless explicitly told otherwise.
3. **Never implicit** — Athena never launches agents, creates tasks, or modifies state without
   first checking the current situation and asking for confirmation.
4. **Tool loop first** — The LLM prompt includes the full app state snapshot and available tools.
   The LLM decides what to do via tool calls.
5. **External MCP** — The MCP server is exposed externally so Claude Code, OpenCode, Cursor, and
   other tools can connect to and control Athena.

---

## What Athena Can Do (End State)

### Conversational Capabilities

| User Says | What Athena Does |
|-----------|-----------------|
| "Is the builder agent done?" | Checks active workspace, reads builder output, reports status + last N lines. |
| "Prompt these four agents to analyze the codebase" | Checks active workspace. Sees 3 agents running. Asks: "You have 3 agents (claude-1, codex-2, gemini-3). What should the 4th be?" After confirmation, launches 4th, dispatches prompt to all 4. |
| "Add a Kanban task to refactor auth" | Creates task in active workspace's board. Asks if an agent should be assigned. |
| "Read lib.rs and summarize what the App component does" | Reads via `fs_read`, sends to LLM, returns summary. |
| "Run cargo test in all terminals" | Lists active terminals, asks confirmation, dispatches command to each. |
| "Switch to the backend workspace" | Changes active workspace. |
| "What agents are running in this workspace?" | Lists all agents in active workspace: status, last activity, output line count. |
| "Create an execution plan to add dark mode, then dispatch it" | Creates plan, breaks into steps, dispatches to agents, monitors, evaluates. |

### System Access Matrix

| System | Read | Write | Scope |
|--------|------|-------|-------|
| **Agents** | Status, output, activity | Spawn, kill, write command, prompt | Active workspace |
| **Kanban** | Full board state | Create, move, update, delete | Active workspace |
| **Files** | Read, list, search | Write, create, delete | Workspace directory |
| **Workspaces** | List, active space | Switch, create, delete | Global |
| **Browser** | URL, content | Navigate, back/forward | Global |
| **Notifications** | History, unread count | Push new notification | Global |
| **Plans** | Active plan + step status | Create, dispatch, evaluate | Global |

> **Note on Swarm:** Swarm is intentionally excluded from this plan. It will be addressed in a
> separate phase after the core orchestration loop is stable. Do not implement swarm tools now.

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                         Athena's Core Desktop                        │
│  ┌─────────────────┐  ┌──────────────────┐  ┌──────────────────────┐ │
│  │  Athena Panel   │  │ Workspace (Term   │  │   Kanban Board       │ │
│  │  (LLM Chat)     │  │  Panes, Agents)  │  │   (Tasks, Columns)   │ │
│  └───────┬─────────┘  └──────────────────┘  └──────────────────────┘ │
│          │                                                            │
│          └──────────────────────┐                                     │
│                                 │                                     │
│          ┌──────────────────────▼────────────┐                        │
│          │        AthenaOrchestrator          │                        │
│          │  • Generic LLM (key + base_url)   │                        │
│          │  • System prompt with app snapshot │                        │
│          │  • Tool loop (request → execute)   │                        │
│          └──────────────────────┬─────────────┘                       │
│                                 │                                     │
│          ┌──────────────────────▼────────────┐                        │
│          │         SnapshotBuilder            │                        │
│          │  • Token-budgeted state assembly   │                        │
│          │  • Per-subsystem allocations       │                        │
│          │  • Truncation + recency priority   │                        │
│          └──────────────────────┬─────────────┘                       │
│                                 │                                     │
│          ┌──────────────────────▼────────────┐                        │
│          │         ToolExecutor              │                        │
│          │  • launch/kill agents             │                        │
│          │  • read/write files               │                        │
│          │  • kanban CRUD                    │                        │
│          │  • run commands in terminals      │                        │
│          │  • create/dispatch plans          │                        │
│          └──────────────────────┬─────────────┘                       │
│                                 │                                     │
│          ┌──────────────────────▼────────────┐                        │
│          │     MCP Server (stdio transport)  │ ◄── External tools    │
│          │  • JSON-RPC 2.0                   │     (Claude Code,     │
│          │  • Session-isolated connections   │      OpenCode, etc.)  │
│          │  • Shared ToolExecutor reference  │                        │
│          └───────────────────────────────────┘                        │
└─────────────────────────────────────────────────────────────────────┘
```

---

## Decisions (Locked — Do Not Re-open)

These were the open questions in the original plan. They are resolved here so the agent
does not make ad-hoc choices mid-implementation.

### Decision 1: Tool Execution Location
All tool execution goes through the backend `ToolExecutor` in `athena-core`.
The frontend only renders results. No tool logic in the frontend stores.

### Decision 2: Workspace State Source of Truth
`get_active_workspace()` reads from the backend `KeyValueStore`. The frontend store
mirrors this but is not authoritative. Tools always read from the backend store.

### Decision 3: Session vs. Agent Terminology
- **"agent"** = a PTY session running an AI tool (lives in `session_manager`)
- **"session"** = a chat conversation with Athena's LLM (lives in `session_store`)
- **"session_manager"** is never referred to as "sessions" in user-facing copy

### Decision 4: Confirmation Flow
State-modifying tools follow this exact sequence:
1. LLM decides it wants to call a destructive/spawning tool
2. Tool returns a `PendingConfirmation` struct (not a result)
3. Orchestrator surfaces this to the user as an `ask_user` block
4. User confirms or cancels
5. Only on confirm: tool executes for real and returns result

Implement via a `requires_confirmation: bool` field on `ToolDefinition`.
Tools with this flag true NEVER execute on first call — they always return
`ToolCallResult::PendingConfirmation { summary: String }`.

### Decision 5: Kanban Persistence
**Backend-first via SQLite** using the existing `athena-store` pattern.
No Tauri event bridge for Kanban. The frontend Kanban store syncs from the backend
on load and after any write tool call. Do not implement a dual-write path.

### Decision 6: MCP Transport
Use **stdio transport**, not raw TCP. The `nc localhost 4545` approach in the original
plan has no auth, no session isolation, and no reconnection handling. Stdio gives
process isolation for free. Claude Code and OpenCode both support stdio MCP.
The sample config is updated in Phase 3.

### Decision 7: State Snapshot Token Budget
The state snapshot injected before each LLM call is capped at **800 tokens total**.
Allocation per subsystem:
- Active workspace name + metadata: 50 tokens
- Agent list (name, status, last activity, line count — NO raw output): 200 tokens
- Kanban tasks (title + status only): 150 tokens
- Recently modified files (paths only, last 10): 100 tokens
- Reserved for overflow / edge cases: 300 tokens

Raw agent output is NEVER injected into the snapshot. It is only fetched on demand
via the `read_agent_output` tool when Athena or the user explicitly asks for it.

---

## Error Taxonomy

Every tool and orchestrator path must handle these five classes. Define them as an enum
in `athena-core` and use them consistently across all error surfaces.

| Class | Trigger | Athena Response |
|-------|---------|-----------------|
| `ToolFailure` | Tool executed but the operation failed (file not found, agent already dead) | Report clearly: "I couldn't read `lib.rs` — it doesn't exist in this workspace." |
| `LLMApiFailure` | API returned non-200, rate limit, invalid key, malformed JSON | Show user-friendly message + raw status code. Do not retry silently. |
| `StateConflict` | Two operations race on same workspace/agent state | Abort the second operation, report which one was aborted and why. |
| `UserCancellation` | User cancels a pending confirmation | Acknowledge cleanly. Do not retry. Do not log as error. |
| `ToolTimeout` | Tool call exceeds 30s (configurable) | Report timeout, surface partial result if available. |

---

## Phase Gate Rules

**The agent must not start a phase until all items in its entry conditions are checked off.**
If a blocker is found, stop and report it — do not work around it silently.

---

## Phase 1: Foundation — LLM + Working Tool Executor

**Goal:** Athena can receive a message, call a tool, and produce a real side effect
(spawning an agent in a terminal pane). Nothing in phases 2–5 is testable until this works.

### ⛔ Entry Condition for Phase 1
- [ ] You have read this entire document
- [ ] No other phase work has started

---

### 1.1 Generic LLM Settings

**Files:** `frontend/src/components/settings/`, `crates/athena-core/src/orchestrator.rs`
**Effort:** Low

**Current state:** `ProviderConfig` exists in the backend. The frontend still shows a
provider dropdown (Anthropic, OpenAI, etc.) which conflicts with the generic approach.

**Changes:**

Backend — `crates/athena-core/src/orchestrator.rs`:
- Add `LLMProvider::Generic` variant
- `send_openai()` already handles `base_url` — wire it through from `ProviderConfig`
- All provider variants should funnel to `send_openai()` when a custom `base_url` is set

Backend — `src-tauri/src/commands/mod.rs`:
```rust
// In build_provider_config_from_store:
let base_url = state.store.get::<String>("llm.base_url").ok().flatten();
let model    = state.store.get::<String>("llm.model").ok().flatten();
let api_key  = state.store.get::<String>("llm.api_key").ok().flatten();
// Pass all three to ProviderConfig
```

Frontend — replace the provider dropdown with three plain fields:
- `API Key` — password input, saves to `llm.api_key`
- `Base URL` — text input (e.g. `https://api.openai.com/v1`), saves to `llm.base_url`
- `Model` — text input (e.g. `gpt-4o`), saves to `llm.model`

Do not add a provider dropdown. The user types their own base URL. Done.

---

### 1.2 Fix `TauriEventSender` ⚠️ PHASE GATE

**Files:** `src-tauri/src/state.rs`, `src-tauri/src/commands/mod.rs`
**Effort:** High
**Status: CRITICAL BLOCKER — nothing else in Phase 1 can be tested until this is done.**

**Current state:** `agent_spawned`, `close_panes`, `pty_write`, and `has_session` are all
no-ops that log a warning. The tool loop cannot produce any real side effects.

**The core problem:** `TauriEventSender` needs access to `SessionManager`, but `AppState`
holds it without sharing the reference.

**Solution:**

Step 1 — Make `TauriEventSender` hold a shared reference:
```rust
pub struct TauriEventSender {
    pub app_handle: AppHandle,
    pub session_manager: Arc<tokio::sync::Mutex<SessionManager>>,
}
```

Step 2 — When `AppState::new()` builds `TauriEventSender`, pass the same `Arc` that
`AppState` itself stores. Both hold a clone of the same `Arc` — there is only one
`SessionManager`.

Step 3 — Implement the no-ops for real:
```rust
impl ToolEventSender for TauriEventSender {
    fn agent_spawned(&self, id: &str, agent_type: &str, agent_cmd: &str) {
        // Lock session_manager, call spawn() with the given agent type and command
        // Emit "athena:agentSpawned" Tauri event so the frontend creates the pane
    }

    fn pty_write(&self, pane_id: &str, data: &str) {
        // Lock session_manager, write data to the PTY for pane_id
    }

    fn has_session(&self, pane_id: &str) -> bool {
        // Lock session_manager, check if pane_id exists
    }

    fn close_panes(&self, pane_ids: &[&str]) {
        // Lock session_manager, kill each pane in pane_ids
    }
}
```

**Verification:** After this step, write a test command that calls `agent_spawned` directly
and confirm a real terminal pane appears. If it doesn't, do not move to 1.3.

---

### 1.3 Write the Orchestrator System Prompt

**Files:** `crates/athena-core/src/orchestrator.rs` (as a `const` or loaded from file)
**Effort:** Low
**Dependency:** Can be written in parallel with 1.2, but cannot be tested until 1.2 is done.

Store the system prompt as a `const &str` in `orchestrator.rs`. Do not load from an
external file — it should ship with the binary.

```
You are Athena, the orchestrator of a developer desktop environment called Athena's Core.

## Your role
You help the user manage their development environment: launching and monitoring AI agents,
managing Kanban tasks, reading files, and running commands in terminals.

## Available tools
- launch_builtin_agent    — Spawn a new AI agent in a terminal pane
- kill_agent              — Stop a running agent
- read_agent_output       — Read the last N lines of an agent's terminal output
- list_agents             — List all running agents in the active workspace
- run_command_in_terminal — Run a shell command in a terminal pane
- fs_read_file            — Read a file from the workspace
- fs_list_dir             — List directory contents
- fs_search               — Search files using ripgrep
- kanban_list_tasks       — List all Kanban tasks in the active workspace
- kanban_create_task      — Create a new Kanban task
- kanban_update_task      — Move or update a Kanban task
- kanban_delete_task      — Delete a Kanban task
- workspace_list          — List all workspaces
- workspace_get_active    — Get the currently active workspace
- workspace_switch        — Switch to a different workspace

## Rules (never violate these)
1. You are workspace-scoped. Every agent and Kanban operation targets the active workspace
   unless the user explicitly names another.
2. Never launch an agent, run a command, or modify state without first calling
   workspace_get_active and checking the current state.
3. If the user says "prompt 4 agents" but only 3 exist, ask what the 4th should be.
   Never invent an agent type.
4. Every state-modifying action requires explicit user confirmation before executing.
5. Always include the active workspace name in your response when reporting agent or task status.
6. When a tool fails, report the failure clearly. Do not silently retry.

## Response style
Concise and technical. Examples:
- Confirming: "I'll launch a Claude agent in 'backend-refactor'. Confirm?"
- Reporting: "3 agents running in 'backend-refactor': claude-1 (idle), codex-2 (working), gemini-3 (idle)."
- Error: "I couldn't read `lib.rs` — it doesn't exist in the active workspace."
```

---

### 1.4 Add Workspace-Aware Tools to ToolExecutor

**Files:** `crates/athena-core/src/tool_executor.rs`
**Effort:** Medium
**Dependency:** 1.2 must be complete (tools need real state to query)

Add these tool implementations:

```rust
fn workspace_list(&self, _args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError>;
fn workspace_get_active(&self, _args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError>;
fn workspace_switch(&self, args: &ToolInput) -> Result<ToolCallResult, ToolExecutorError>;
```

All three read from / write to the backend `KeyValueStore` under the `workspace.*` namespace.
`workspace_switch` has `requires_confirmation: true`.

---

### 1.5 Add SnapshotBuilder with Token Budget

**Files:** New file `crates/athena-core/src/snapshot.rs`, update `orchestrator.rs`
**Effort:** Medium
**Dependency:** 1.4 must be complete (snapshot pulls from workspace tools)

Create a `SnapshotBuilder` struct that assembles the state context injected before each
LLM call. Hard limits (see Decision 7):

```rust
pub struct SnapshotBudget {
    pub workspace_meta_tokens: usize,   // 50
    pub agents_tokens: usize,           // 200
    pub kanban_tokens: usize,           // 150
    pub recent_files_tokens: usize,     // 100
    pub overflow_tokens: usize,         // 300
}
```

Builder rules:
- Agent entries include: name, status, last activity timestamp, output line count.
  **Never include raw agent output in the snapshot.** Output is fetched on demand only.
- Kanban entries include: title, status column. No description, no comments.
- Recent files: last 10 modified paths, no content.
- If any section exceeds its allocation, truncate oldest entries first.
- Append `[truncated]` marker when truncation occurs so the LLM knows.

The snapshot is injected as a system context message (not as part of the user turn) on
every call to the orchestrator.

Output format:
```
[Current State — backend-refactor]
Agents (3 running):
  pane-1: claude  | idle    | last active 2m ago | 142 lines
  pane-2: codex   | working | last active 0m ago |  89 lines
  pane-3: gemini  | idle    | last active 8m ago |   0 lines
Kanban (2 tasks):
  "Refactor auth module" — In Progress
  "Add dark mode"        — To Do
Recent files (last 10 modified):
  src/auth/mod.rs, src/ui/theme.rs
```

---

### Phase 1 Completion Checklist

Before moving to Phase 2, verify all of the following manually:

- [ ] User can enter an API key, base URL, and model in settings and save them
- [ ] A message to Athena triggers a real HTTP call to the configured LLM
- [ ] The LLM can call `list_agents` and get a real list from the active workspace
- [ ] The LLM can call `launch_builtin_agent` and a real terminal pane appears
- [ ] The state snapshot is injected before each LLM call and is under 800 tokens
- [ ] A failed tool call reports the error class correctly (see Error Taxonomy)

---

## Phase 2: Kanban + File System Integration

**Goal:** Athena can create Kanban tasks, read/search files, and report on them.

### ⛔ Entry Conditions for Phase 2
- [ ] All Phase 1 checklist items are verified
- [ ] `TauriEventSender` is confirmed working with a real agent spawn test

---

### 2.1 Kanban Backend Persistence

**Files:** New `crates/athena-core/src/kanban.rs`, update `tool_executor.rs`
**Effort:** Medium

**DECIDED (see Decision 5):** Backend SQLite via `athena-store`. No Tauri event bridge.

Schema (SQLite via existing `athena-store`):
```sql
CREATE TABLE kanban_tasks (
    id          TEXT PRIMARY KEY,
    workspace   TEXT NOT NULL,
    title       TEXT NOT NULL,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'todo',  -- todo | in_progress | done
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);
```

Tool implementations in `tool_executor.rs`:
- `kanban_list_tasks`   — Query by active workspace, return JSON array
- `kanban_create_task`  — Insert row, return new task ID. `requires_confirmation: false`
  (creating a task is low-risk; user can delete it)
- `kanban_update_task`  — Update status/position/description. `requires_confirmation: false`
- `kanban_delete_task`  — Delete by ID. `requires_confirmation: true`

After any write operation, emit a `athena:kanbanUpdated` Tauri event so the frontend
Kanban component re-fetches from the backend store.

---

### 2.2 File System Tools

**Files:** `crates/athena-core/src/tool_executor.rs`
**Effort:** Low

These Tauri commands already exist — map them to `ToolExecutor`:
- `fs_read_file`    → maps to existing `fs_read` command
- `fs_write_file`   → maps to existing `fs_write`. `requires_confirmation: true`
- `fs_list_dir`     → maps to existing `fs_list_dir` command
- `fs_search`       → maps to existing `search_code` command (ripgrep-backed)

`fs_write_file` and any delete operation require confirmation. Read operations do not.

---

### Phase 2 Completion Checklist

- [ ] User can ask Athena "create a Kanban task: refactor auth" and see it appear on the board
- [ ] User can ask Athena "what tasks are in this workspace" and get an accurate list
- [ ] User can ask Athena "read src/main.rs" and get the file contents summarized
- [ ] User can ask Athena "search for all uses of `spawn_agent`" and get file:line results
- [ ] Kanban state survives an app restart

---

## Phase 3: MCP Server

**Goal:** External tools (Claude Code, OpenCode) can connect to Athena's MCP server.

### ⛔ Entry Conditions for Phase 3
- [ ] All Phase 2 checklist items are verified

---

### 3.1 MCP Transport: Stdio (not TCP)

**DECIDED (see Decision 6):** Use stdio transport. Drop the TCP `nc` approach.

The MCP server runs as a subprocess spawned by the external tool (Claude Code, OpenCode).
It communicates over stdin/stdout using JSON-RPC 2.0.

In `crates/athena-core/src/mcp.rs`:
- Remove or gate the TCP listener behind a `--tcp` flag for debugging only
- Implement stdio handler that reads JSON-RPC from stdin, writes responses to stdout
- Each connection (subprocess instance) gets its own session context
- All tool calls route to the shared `ToolExecutor` (same instance as the Athena chat panel)

The subprocess approach means Claude Code and OpenCode each get isolated sessions.
No session collision by design.

---

### 3.2 Wire MCP Handlers to Real ToolExecutor

**Files:** `crates/athena-core/src/mcp.rs`
**Effort:** Medium

Wire each MCP tool name to the real `ToolExecutor` method. The MCP server exposes the
same tool surface as the internal chat loop — no duplicate implementations:

```rust
match tool_name {
    "launch_builtin_agent"    => tool_executor.launch_builtin_agent(&args),
    "kill_agent"              => tool_executor.kill_agent(&args),
    "list_agents"             => tool_executor.list_agents(&args),
    "read_agent_output"       => tool_executor.read_agent_output(&args),
    "kanban_create_task"      => tool_executor.kanban_create_task(&args),
    "kanban_list_tasks"       => tool_executor.kanban_list_tasks(&args),
    "kanban_update_task"      => tool_executor.kanban_update_task(&args),
    "kanban_delete_task"      => tool_executor.kanban_delete_task(&args),
    "fs_read_file"            => tool_executor.fs_read_file(&args),
    "fs_search"               => tool_executor.fs_search(&args),
    "workspace_list"          => tool_executor.workspace_list(&args),
    "workspace_get_active"    => tool_executor.workspace_get_active(&args),
    _ => Err(McpError::UnknownTool(tool_name.to_string())),
}
```

Note: `workspace_switch`, `fs_write_file`, and `kanban_delete_task` are **not exposed**
through MCP. External tools should not be able to switch workspaces or delete files
without the user's hand on the Athena panel. This is a security boundary.

---

### 3.3 MCP Configuration Docs

Add `docs/mcp-setup.md` to the repo. Sample config for Claude Code:

```json
{
  "mcpServers": {
    "athena-core": {
      "command": "/path/to/athena-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

Document clearly:
- `athena-mcp` is a separate binary (or `athena-core --mcp-mode`)
- It requires the Athena desktop app to be running first
- It connects to the running app's shared state via IPC
- The external tool gets read + safe-write access only (see restricted tool list above)

---

### Phase 3 Completion Checklist

- [ ] Claude Code can connect to Athena via the MCP stdio config
- [ ] From Claude Code: "list agents in Athena" returns the current workspace's agents
- [ ] From Claude Code: "create a Kanban task" creates a task visible in the UI
- [ ] From Claude Code: workspace_switch is not available (returns UnknownTool error)

---

## Phase 4: Polish + Conversation Memory

**Goal:** Athena feels alive, remembers past sessions, and handles failures gracefully.

### ⛔ Entry Conditions for Phase 4
- [ ] All Phase 3 checklist items are verified

---

### 4.1 Conversation Memory

- Persist Athena chat sessions to `session_store` (SQLite-backed, already exists)
- Each session: ID, workspace at time of creation, timestamp, message history (JSON)
- On panel open: show the last session for the current workspace, offer "New session" button
- Do not auto-inject old sessions into new LLM calls — memory is for the user to browse,
  not for the LLM to accumulate indefinitely

---

### 4.2 Error Handling Polish

Implement the full Error Taxonomy (defined above) with user-facing messages:

| Error Class | User-facing message template |
|-------------|------------------------------|
| `ToolFailure` | "I couldn't [action] — [reason]." |
| `LLMApiFailure` | "The LLM API returned an error ([status code]). Check your API key and base URL in settings." |
| `StateConflict` | "Two operations conflicted on [resource]. I cancelled [operation]. Try again." |
| `UserCancellation` | (no message — just dismiss the confirmation block) |
| `ToolTimeout` | "The [tool_name] call timed out after 30s. [partial result if any]" |

---

### 4.3 Tone and Response Style

Athena's responses are concise, technical, and action-oriented. No filler.

| Situation | Template |
|-----------|----------|
| Confirming an action | "I'll [action] in '[workspace]'. Confirm?" |
| Reporting status | "[N] agents running in '[workspace]': [name] ([status]), ..." |
| Reporting an error | "I couldn't [action] — [reason]." |
| Asking for missing info | "You asked for 4 agents but only 3 exist. What should the 4th be?" |

---

### Phase 4 Completion Checklist

- [ ] Past sessions are accessible from the Athena panel
- [ ] Each error class produces the correct user-facing message
- [ ] API key errors show the status code, not a generic "something went wrong"

---

## Phase 5: Drag-and-Drop Visual Context

**Goal:** Users can drag agents, files, or Kanban cards onto the Athena panel to pin them
as context for the current conversation.

### ⛔ Entry Conditions for Phase 5
- [ ] All Phase 4 checklist items are verified

---

### 5.0 Context Injection API (Prerequisite for 5.1 and 5.2)

Before implementing drag-and-drop, define the generic mechanism that all context types
will use. This prevents 5.1 and 5.2 from each inventing their own approach.

Add a `PinnedContext` type in `orchestrator.rs`:

```rust
pub enum PinnedContext {
    Agent { pane_id: String, agent_type: String, status: String, line_count: usize },
    KanbanTask { task_id: String, title: String, status: String },
    File { path: String, preview_lines: Vec<String> },  // first 20 lines only
}
```

Pinned context is injected into the system prompt after the main snapshot, in a separate
`[Pinned Context]` block. The user can pin multiple items. Pinned context persists for
the current session (cleared on "New session").

---

### 5.1 Agent Drag-to-Context

- User drags an agent card from the sidebar onto the Athena panel
- The `PinnedContext::Agent` entry is added to the current session
- Athena confirms: "I've pinned pane-1 (claude, idle) as context. I'll refer to it as
  'this agent' in this session."
- Subsequent prompts about "this agent" resolve to the pinned agent's pane_id

---

### 5.2 Kanban Drag-to-Assign

- User drags a Kanban card onto an agent card
- Athena asks: "Assign 'Refactor auth module' to claude-1?"
- If yes: updates the Kanban task's `assigned_agent` field and emits `kanbanUpdated`
- Does not auto-spawn or auto-prompt the agent — that is a separate explicit user action

---

## Implementation Order

| Phase | Step | What | Files | Effort | Depends On |
|-------|------|------|-------|--------|------------|
| 1 | 1.1 | Generic LLM settings | `settings/`, `orchestrator.rs` | Low | — |
| 1 | **1.2** | **Fix TauriEventSender** | `state.rs`, `commands/mod.rs` | **HIGH** | **Nothing — do this first** |
| 1 | 1.3 | Orchestrator system prompt | `orchestrator.rs` | Low | Can parallel with 1.2 |
| 1 | 1.4 | Workspace-aware tools | `tool_executor.rs` | Medium | 1.2 complete |
| 1 | 1.5 | SnapshotBuilder with token budget | New `snapshot.rs`, `orchestrator.rs` | Medium | 1.4 complete |
| 2 | 2.1 | Kanban SQLite backend + tools | New `kanban.rs`, `tool_executor.rs` | Medium | Phase 1 ✓ |
| 2 | 2.2 | File system tools | `tool_executor.rs` | Low | Phase 1 ✓ |
| 3 | 3.1 | MCP stdio transport | `mcp.rs` | Medium | Phase 2 ✓ |
| 3 | 3.2 | Wire MCP to ToolExecutor | `mcp.rs` | Medium | 3.1 complete |
| 3 | 3.3 | MCP config docs | `docs/mcp-setup.md` | Low | 3.2 complete |
| 4 | 4.1 | Conversation memory | `session_store`, `orchestrator.rs` | Medium | Phase 3 ✓ |
| 4 | 4.2 | Error handling polish | All error sites | Medium | Phase 3 ✓ |
| 5 | 5.0 | PinnedContext API | `orchestrator.rs` | Low | Phase 4 ✓ |
| 5 | 5.1 | Agent drag-to-context | Frontend + `orchestrator.rs` | Medium | 5.0 complete |
| 5 | 5.2 | Kanban drag-to-assign | Frontend + `tool_executor.rs` | Medium | 5.0 complete |

---

## Files Reference

| File | Role |
|------|------|
| `crates/athena-core/src/orchestrator.rs` | LLM client, tool loop, system prompt, snapshot injection |
| `crates/athena-core/src/snapshot.rs` | SnapshotBuilder with token budget (new) |
| `crates/athena-core/src/tool_executor.rs` | All tool implementations |
| `crates/athena-core/src/kanban.rs` | Kanban SQLite persistence (new) |
| `crates/athena-core/src/mcp.rs` | MCP stdio server, wired to ToolExecutor |
| `src-tauri/src/state.rs` | AppState, TauriEventSender (fix Arc sharing) |
| `src-tauri/src/commands/mod.rs` | Tauri command handlers, build_provider_config |
| `frontend/src/components/settings/` | Generic LLM config fields (key, base_url, model) |
| `frontend/src/stores/athena.rs` | Athena chat state, session history, pinned context |
| `frontend/src/stores/workspace.rs` | Workspace UI state (mirrors backend KV store) |
| `frontend/src/stores/task.rs` | Kanban UI state (syncs from backend after writes) |
| `frontend/src/tauri_bridge.rs` | IPC invoke() wrappers |
| `docs/mcp-setup.md` | MCP configuration guide for Claude Code / OpenCode |

---

## Success Criteria

After all phases are complete, the following must work end-to-end without manual
intervention:

1. Open Athena settings → enter any OpenAI-compatible API key, base URL, model → save
2. Ask: "What agents are running in my current workspace?"
   → Response: "You're in 'backend-refactor'. 3 agents: claude-1 (idle), codex-2 (working), gemini-3 (idle)."
3. Ask: "Launch a 4th agent, make it Claude"
   → Athena asks for confirmation → user confirms → new terminal pane opens with Claude
   → Response: "Launched claude-4 in 'backend-refactor'."
4. Ask: "Create a Kanban task: refactor auth module"
   → Task appears on the board
   → Response: "Created 'refactor auth module' in 'backend-refactor'."
5. Configure Claude Code MCP: `{ "command": "/path/to/athena-mcp" }`
6. In Claude Code: "List agents in Athena"
   → Returns the same agent list as step 2
7. In Claude Code: "Create a task in Athena: add dark mode"
   → Task appears in the Kanban board
8. In Claude Code: attempt `workspace_switch`
   → Returns `UnknownTool` error (blocked at MCP boundary)
