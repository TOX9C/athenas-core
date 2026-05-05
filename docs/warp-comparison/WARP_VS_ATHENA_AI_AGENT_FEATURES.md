# Warp vs. Athena's Core — AI & Agent Feature Comparison

> **Warp** (https://github.com/warpdotdev/warp): GPU-accelerated Rust terminal with built-in AI assistant and agent mode
> **Athena's Core**: Electron-based AI agent orchestration IDE — spawns and coordinates external coding agents

---

## 1. AI Integration Approach

| Dimension               | Warp                                                                                                                                  | Athena's Core                                                                                                                                |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| **AI architecture**     | In-process AI crate (`crates/ai/`) — agent loop, skills, indexing, diff validation all run inside the app                             | External AI orchestrator (`electron/athenaOrchestrator.ts`) — LLM calls happen in main process, but agents run as **external PTY processes** |
| **LLM providers**       | OpenAI GPT models (founding sponsor); BYOK via API keys; AWS credentials for custom endpoints                                         | Anthropic (Claude) and OpenAI-compatible (NVIDIA NIM, LM Studio) via SDK; provider selected in settings                                      |
| **LLM SDK**             | Direct HTTP calls via `reqwest` + `hyper`; protobuf for multi-agent API (`warp_multi_agent_api`)                                      | `@anthropic-ai/sdk` + `openai` npm packages                                                                                                  |
| **Agent loop location** | Inside the Rust process (`crates/ai/src/agent/`)                                                                                      | Inside the Electron main process (`athenaOrchestrator.ts`)                                                                                   |
| **Context building**    | Project context indexing (`crates/ai/src/project_context/`); full-text search via `tantivy`; `arborium` AST parsing for 25+ languages | Dynamic system prompt built from workspace state, tasks, custom agents, and active panes — no codebase indexing                              |
| **Diff validation**     | Dedicated `crates/ai/src/diff_validation/` — validates AI-proposed code changes before applying                                       | No diff validation — tool results fed back to LLM as-is                                                                                      |

**Key difference**: Warp's AI is an **in-process assistant** with deep codebase understanding (AST parsing, full-text search, diff validation). Athena's AI is an **orchestrator of external agents** — it doesn't understand the codebase itself but coordinates agents that do.

---

## 2. Agent Management

| Dimension           | Warp                                                                                                                              | Athena's Core                                                                                                                                                 |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Agent types**     | Built-in AI agent (in-process); "Oz" cloud agent (proprietary); external CLI agents (Claude Code, Codex, etc.) via `cli_agent.rs` | 6 agent types: `claude`, `codex`, `opencode`, `gemini`, `custom`, `shell` — all run as external PTY processes                                                 |
| **Agent execution** | In-process (built-in) or server-side (Oz); external agents are a feature, not the core model                                      | All agents are **external CLI processes** spawned via `node-pty` (`ptyManager.ts`)                                                                            |
| **Agent lifecycle** | WarpUI view lifecycle; agents exist as UI elements within the terminal                                                            | PTY process lifecycle: spawn → ready (prompt detection) → running → exit; `ptyManager.ts` tracks state in module-level Maps                                   |
| **Agent output**    | In-process output capture; block model isolates per-command output                                                                | `output-buffer-service` — in-memory ring buffer per pane (5000 lines / 2MB cap), ANSI-stripped, line-numbered, timestamped                                    |
| **Agent status**    | WarpUI model updates via `AppContext`; integrated with view system                                                                | `agentStatusStore.ts` (renderer) + `agent-comms.ts` status updates (TCP:4546); stall detection (90s timeout)                                                  |
| **Custom agents**   | No custom agent registration UI                                                                                                   | Settings modal: register custom agents with name + CLI command; stored in `electron-store` as `athena-customAgents`                                           |
| **Agent input**     | WarpUI text editor for commands; AI agent has its own input UI                                                                    | `ptyManager.write()` sends keystrokes to PTY; `toolExecutor.prompt_agent()` writes command + delayed Enter; `askUser` tool for orchestrator-to-user questions |

**Key difference**: Warp treats its AI agent as a first-class UI citizen that lives inside the app. Athena treats agents as external processes it spawns, monitors, and communicates with — the app is fundamentally an **orchestrator and observer**, not an agent runtime.

---

## 3. Swarm / Multi-Agent Coordination

| Dimension                 | Warp                                                                                                                         | Athena's Core                                                                                                                           |
| ------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| **Multi-agent API**       | `warp_multi_agent_api` — protobuf-based protocol for multi-agent orchestration (separate repo: `warpdotdev/warp-proto-apis`) | No formal multi-agent protocol; coordination via filesystem (`swarmCoordinator.ts`) and TCP messaging (`agent-comms.ts`)                |
| **Orchestration model**   | Cloud-mediated (Oz platform); `orchestration` and `orchestration_v2` feature flags; `agent_harness` for agent execution      | Local-only; `swarmCoordinator.ts` manages file-based state (`.ade/swarm-state.json`) and mailbox system (`.ade/mailbox/{agentId}.json`) |
| **Swarm state**           | Server-side (Warp Drive); protobuf definitions; real-time sync via GraphQL                                                   | Local filesystem; JSON files in project `.ade/` directory; 5-second polling for state changes                                           |
| **Inter-agent messaging** | Protobuf messages via multi-agent API; `ambient_agents_rtc` for real-time communication                                      | File-based mailbox: `swarm:sendMessage` appends to `.ade/mailbox/{to}.json`; `swarm:readMailbox` reads messages                         |
| **Agent roles**           | Coordinator, builder, scout, reviewer (defined in `warp_multi_agent_api` protobuf)                                           | Same roles defined in `src/types/swarm.ts` but only used in UI; no enforced role logic in backend                                       |
| **Stall detection**       | Server-side (Oz platform)                                                                                                    | 90-second timeout in `swarmCoordinator.ts` (polling-based); 90-second timeout in `pluginHost.ts` (interval-based)                       |
| **Swarm UI**              | Feature-flagged swarm board (not in OSS repo — part of proprietary UI)                                                       | `SwarmBoard.tsx` — shows goal, status badge, pause/resume/abort controls, agent cards with nudge capability, activity feed              |
| **Launch experience**     | Cloud-based swarm launch via Oz                                                                                              | `SwarmModal.tsx` + `SwarmLauncher.tsx` — local launch with goal, agent count, and role configuration                                    |

**Key difference**: Warp's multi-agent coordination is cloud-mediated via protobuf APIs and the Oz platform. Athena's is entirely local, using file-based state and mailbox messaging — simpler but limited to a single machine.

---

## 4. MCP (Model Context Protocol) Tools

| Dimension                             | Warp                                                                                                                         | Athena's Core                                                                                                                                                                                                         |
| ------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **MCP role**                          | Primarily a **MCP client** — connects to external MCP tool servers; also an **MCP server** when `mcp_server` feature enabled | Primarily a **MCP server** — exposes tools for external agents to call                                                                                                                                                |
| **MCP library**                       | `rmcp` — Rust MCP client (forked at `warpdotdev/rmcp`)                                                                       | Custom `mcpServer.ts` — hand-rolled JSON-RPC 2.0 over TCP                                                                                                                                                             |
| **MCP transport**                     | 3 transports: Streamable HTTP (via reqwest), SSE (via reqwest), child process (stdio)                                        | 1 transport: TCP (port 4545) with token-based auth                                                                                                                                                                    |
| **MCP server tools** (Warp as server) | Not documented in OSS repo; `mcp_server` feature flag exists                                                                 | 11 tools: `create_tasks`, `get_next_task`, `update_task_status`, `spawn_agents`, `notify`, `status_update`, `get_output`, `list_agent_panes`, `athena_forward_output`, `send_message_to_agent`, `read_agent_messages` |
| **MCP client usage**                  | AI agent calls MCP tool servers during execution; `rmcp` handles connection lifecycle, auth (`mcp_oauth`), and transport     | No MCP client — Athena does not connect to external MCP servers                                                                                                                                                       |
| **MCP persistence**                   | SQLite: `add_mcp_pane`, `add_mcp_environment_variable_table`, `add_running_mcp_servers_table` migrations                     | No persistence — MCP server is transient, starts/stops with the app                                                                                                                                                   |
| **MCP config**                        | `.mcp.json` in project root; `file_based_mcp` feature flag                                                                   | `agentMcpConfig.ts` — builds spawn prefix with MCP config for agent PTY processes                                                                                                                                     |
| **MCP debugging**                     | `mcp_debugging_ids` feature flag for protocol tracing                                                                        | No MCP debugging tools                                                                                                                                                                                                |

**Key difference**: Warp is a **MCP consumer** — its AI agent connects to external MCP tool servers to extend its capabilities. Athena is a **MCP provider** — it exposes its internal capabilities (task management, agent spawning, output reading) as MCP tools for external agents to consume. They use MCP from opposite directions.

---

## 5. Plugin System

| Dimension                  | Warp                                                                                                  | Athena's Core                                                                                                                                                                     |
| -------------------------- | ----------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Plugin runtime**         | QuickJS sandbox via `rquickjs` + `warp_js` crate; `plugin_host` feature flag                          | No sandboxed plugin runtime                                                                                                                                                       |
| **Plugin API**             | JS bridge with bincode serialization; completion signature definitions run in sandbox                 | Plugin registry (`plugin-manager.ts`) with enable/disable/configure; plugin sessions with capability scoping (`pluginHost.ts`)                                                    |
| **Primary use case**       | Shell command completions v2 — JS-based completion specs loaded and executed in QuickJS               | Agent integration — plugins register sessions with capabilities (notifications, status, tasks, agent_control, user_input, file_access, swarm)                                     |
| **Plugin manifest**        | Completion signature JS files (Fig-derived format)                                                    | `PluginManifest` type: id, name, version, description, author, capabilities, tools, config schema, install method (builtin/mcp_server/hook)                                       |
| **Plugin installation**    | Bundled with app or loaded dynamically; `node_runtime` crate manages Node.js for npm-based extensions | `pluginHost:setupPlugin` / `pluginHost:removePlugin` IPC — writes MCP config entries for agent types (opencode, claude-code)                                                      |
| **Plugin event system**    | Completions return suggestions to the terminal input system                                           | `PluginEventBus.tsx` (renderer) + `pluginHost.emitPluginEvent()` (main) — 18 event types with typed payloads; auto-notifications for `needs_input`, `task_complete`, `task_error` |
| **Plugin isolation**       | QuickJS sandbox — plugins cannot access Rust memory or system APIs                                    | No isolation — plugin sessions are tracked in main process Maps; capabilities are scoped per agent type via `DEFAULT_CAPABILITIES` map                                            |
| **Third-party extensions** | JS completion specs; MCP server connections (via `rmcp`)                                              | Custom agent registration; MCP tool server connections                                                                                                                            |

**Key difference**: Warp's plugin system is a **sandboxed JS runtime** for shell completion intelligence. Athena's plugin system is an **agent session manager** with capability scoping and event routing. They solve fundamentally different problems.

---

## 6. Orchestrator Tools (Athena-specific)

Athena's `toolExecutor.ts` defines 12 orchestrator tools that the LLM can invoke during the agentic loop:

| Tool                       | Purpose                                                           | Warp Equivalent                                         |
| -------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------- |
| `launch_builtin_agent`     | Spawn a Claude/Codex/OpenCode/Gemini agent in a new terminal pane | No direct equivalent — Warp's agent runs in-process     |
| `launch_custom_agent`      | Spawn a user-defined custom CLI agent                             | No direct equivalent                                    |
| `close_terminals`          | Close terminal panes by ID                                        | No tool-based equivalent                                |
| `run_command_in_terminals` | Write a command to specific PTY sessions                          | No tool-based equivalent — Warp's agent writes directly |
| `read_agent_output`        | Read captured output from the output-buffer-service               | Warp reads in-process; no separate buffer needed        |
| `list_agents`              | List all active agent panes and sessions                          | Warp's agent list is part of the view hierarchy         |
| `check_agent_status`       | Get detailed status of a specific agent                           | WarpUI model queries                                    |
| `create_execution_plan`    | Create a structured plan with dependent steps                     | Warp's AI agent has its own planning (not tool-based)   |
| `dispatch_plan_step`       | Execute a plan step (spawn agent, check dependencies)             | No direct equivalent                                    |
| `prompt_agent`             | Send a text prompt to a running agent's PTY                       | Warp's agent communicates via in-process API            |
| `ask_user`                 | Ask the user a question with options, block on response           | Warp's AI agent has its own input UI                    |
| `evaluate_results`         | Review plan step results and update statuses                      | No direct equivalent                                    |

**Key difference**: Athena needs 12 explicit tools because the orchestrator and agents are **separate processes** — every interaction requires IPC. Warp's in-process agent can call internal APIs directly, eliminating the need for a tool bridge.

---

## 7. AI-Powered Features

| Feature                    | Warp                                                                                                                                                   | Athena's Core                                                                                                                 |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------- |
| **Code completion**        | Shell command completions via JS specs (Fig-derived)                                                                                                   | No shell completion system                                                                                                    |
| **Natural language input** | `input_classifier` crate + `natural_language_detection` crate (fasttext, ONNX models) — classifies input as natural language vs. command               | No input classification; Athena chat is explicitly NL                                                                         |
| **AI agent mode**          | Full agent mode with plan-execute-review loop; skills system; computer use (screen control); `agent_mode_debug`/`agent_mode_primary_xml` feature flags | Agentic loop in `athenaOrchestrator.ts` (up to 50 iterations); 12 tools; stall detection (5 identical consecutive tool calls) |
| **Codebase indexing**      | `tantivy` full-text search + `arborium` AST parsing (25+ languages) + `warp_ripgrep` for code search                                                   | No codebase indexing; system prompt includes workspace/pane metadata only                                                     |
| **AI workflows**           | `warp-workflows` crate — user-defined reusable command sequences; `am_workflows` for agent mode workflows; `team_workflows`                            | No workflow system                                                                                                            |
| **AI skills**              | `.agents/skills/` — 17+ structured skill definitions (add-feature-flag, fix-errors, implement-specs, rust-unit-tests, etc.)                            | No skills system                                                                                                              |
| **Diff validation**        | `crates/ai/src/diff_validation/` — validates AI-proposed code changes                                                                                  | No diff validation                                                                                                            |
| **Computer use**           | `crates/computer_use/` + `agent_mode_computer_use` feature — agent can interact with screen elements                                                   | No computer use / screen control                                                                                              |
| **LSP as tool**            | `lsp_as_a_tool` feature flag — AI agent can use Language Server Protocol for code intelligence                                                         | No LSP integration                                                                                                            |
| **Web search/fetch**       | `web_search_ui` and `web_fetch_ui` feature flags                                                                                                       | No web search/fetch tools                                                                                                     |
| **Voice input**            | `crates/voice_input/`                                                                                                                                  | No voice input                                                                                                                |
| **Cloud AI**               | `cloud_conversations` feature; `cloud_mode` for remote AI execution; `drive_objects_as_context` for Warp Drive context                                 | No cloud AI; fully local                                                                                                      |
| **AI auto-title**          | `shared_block_title_generation` — AI generates titles for shared command blocks                                                                        | No auto-title generation                                                                                                      |

---

## 8. Agent Communication Architecture

```
WARP:
┌─────────────────────────────────────────────┐
│  Warp App (single Rust process)             │
│  ┌──────────┐  ┌──────────┐  ┌───────────┐ │
│  │ AI Agent │→ │ rmcp     │→ │ External  │ │
│  │ (in-proc)│  │ MCP Ctx  │  │ MCP Svrs  │ │
│  └────┬─────┘  └──────────┘  └───────────┘ │
│       │ direct calls                        │
│  ┌────▼─────┐                               │
│  │ Terminal │  ┌──────────┐                │
│  │ Model    │  │ Protobuf │→ Oz Cloud      │
│  │ (blocks) │  │ Multi-Ag │  Agent Svc     │
│  └──────────┘  └──────────┘                │
└─────────────────────────────────────────────┘

ATHENA'S CORE:
┌───────────────────────┐     ┌──────────────────────┐
│  Main Process (Node)  │     │  Renderer (Chromium) │
│  ┌──────────────────┐ │     │  ┌────────────────┐  │
│  │ AthenaOrchestr.  │ │ IPC │  │ AthenaPanel    │  │
│  │ (LLM agentic loop│◄─────►│  │ (chat UI)      │  │
│  └───────┬──────────┘ │     │  └────────────────┘  │
│          │            │     │  ┌────────────────┐  │
│  ┌───────▼──────────┐ │     │  │ TerminalGrid   │  │
│  │ toolExecutor     │ │ IPC │  │ (xterm.js)     │  │
│  │ (12 tools)       │◄─────►│  └────────────────┘  │
│  └───────┬──────────┘ │     └──────────────────────┘
│          │            │
│  ┌───────▼──────────┐ │
│  │ ptyManager       │─┼──→  External Agent PTYs
│  │ (node-pty)       │ │    (claude, codex, opencode, gemini, custom)
│  └───────┬──────────┘ │
│          │            │
│  ┌───────▼──────────┐ │     ┌──────────────────┐
│  │ output-buffer-svc│ │     │  MCP Clients     │
│  │ (ring buffer)    │ │     │  (Claude Code,   │
│  └───────┬──────────┘ │     │   OpenCode, etc.)│
│          │            │     └────────┬─────────┘
│  ┌───────▼──────────┐ │              │
│  │ mcpServer        │◄──────────────┘
│  │ (TCP:4545 JSON-RPC)  Tool calls from agents
│  └───────┬──────────┘ │
│          │            │
│  ┌───────▼──────────┐ │     ┌──────────────────┐
│  │ agent-comms      │─┼──→  │  Agent Sockets   │
│  │ (TCP:4546 JSON-RPC)     │  (real-time msgs) │
│  └──────────────────┘ │     └──────────────────┘
└───────────────────────┘
```

---

## 9. Summary: Core AI/Agent Dichotomy

|                            | Warp                                                                            | Athena's Core                                                                              |
| -------------------------- | ------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ |
| **AI paradigm**            | **In-process assistant** — AI runs inside the app, calls internal APIs directly | **External agent orchestrator** — AI orchestrates separate CLI agent processes via IPC/PTY |
| **MCP direction**          | **Consumer** — connects to external MCP tool servers to extend AI capabilities  | **Provider** — exposes internal capabilities as MCP tools for external agents              |
| **Codebase understanding** | Deep — AST parsing, full-text search, ripgrep, codebase index                   | Shallow — workspace metadata only in system prompt                                         |
| **Agent execution**        | In-process (primary) + cloud (Oz) + external CLI (secondary)                    | External CLI processes only (primary and only model)                                       |
| **Multi-agent**            | Cloud-mediated protobuf API (Oz platform)                                       | Local file-based coordination (`.ade/` directory)                                          |
| **Plugin model**           | JS sandbox (QuickJS) for shell completions                                      | Agent session manager with capability scoping                                              |
| **Safety**                 | Diff validation before applying AI changes; QuickJS sandbox for plugins         | No diff validation; no plugin sandbox                                                      |
| **Cloud dependency**       | Required for Oz, cloud AI, session sharing, team features                       | None — fully offline/local by design                                                       |
