# Warp vs Athena's Core — Backend/Core Logic Comparison

> A detailed comparison of backend architecture, data flow, and core logic between [Warp](https://github.com/warpdotdev/warp) and Athena's Core.

---

## 1. Language & Runtime

| Aspect              | Warp                                 | Athena's Core                                  |
| ------------------- | ------------------------------------ | ---------------------------------------------- |
| Language            | Rust (98.2% of codebase)             | TypeScript / Node.js                           |
| Package system      | Cargo workspace with 60+ crates      | npm with Electron-Vite                         |
| Async runtime       | Tokio                                | Node.js event loop (single-threaded)           |
| Compilation targets | Native (macOS/Windows/Linux) + WASM  | Electron (Chromium + Node.js)                  |
| Process model       | Multi-threaded with explicit locking | Single main process + renderer process via IPC |

---

## 2. Terminal Emulation & PTY Management

### Warp

- Custom ANSI/VT parser forked from Alacritty/VTE — a full VT100/VT510 state machine (~70KB `mod.rs`)
- Dedicated **PTY reader thread** using `mio::Poll` (epoll on Linux, kqueue on macOS)
- `FairMutex<TerminalModel>` with a **64KB max locked read** — releases the lock periodically to prevent UI starvation during large output bursts
- **256KB read buffer** per PTY
- Supports **Bracketed Synchronized Updates (BSU)** to avoid tearing during large output
- Two spawn strategies: a dedicated **Terminal Server subprocess** (preferred on Unix) that isolates PTY file descriptors, or direct spawn as fallback
- Platform-specific PTY implementations: POSIX `forkpty()`/`openpty()` on Unix, Windows `ConPty` API
- Docker sandbox support — shells can be launched inside containers via `sbx run`
- Event loop adapted from Alacritty with Warp-specific extensions

### Athena's Core

- Delegates to **`node-pty`** (native C++ addon) — no custom ANSI parser
- Raw PTY data streamed directly to **xterm.js** in the renderer process via Electron IPC
- Shell-readiness detection via **regex pattern matching** on raw output (`$`, `❯`, `>>>`, `%`, `╰─>`, `(y/n)`)
- Sessions stored in a `Map<string, IPty>` — no dedicated reader thread
- **100KB history cap** per session with chunked eviction (oldest chunks dropped first)
- No synchronized output support, no terminal server subprocess isolation
- Agent commands written to PTY as raw text + `\r` with a 1-second delay after spawn
- Graceful shutdown: sends Ctrl+C → 50ms delay → `/exit\r` → 800ms wait for disk flushes

### Key Differences

- Warp parses ANSI itself; Athena delegates parsing to xterm.js in the renderer
- Warp has a dedicated I/O thread with bounded locking; Athena runs PTY events on the Node.js main thread
- Warp isolates PTY file descriptors in a subprocess; Athena runs PTYs in-process
- Warp supports BSU for tear-free rendering; Athena has no equivalent
- Warp has platform-specific PTY implementations; Athena relies on `node-pty`'s cross-platform abstraction

---

## 3. Shell Integration

### Warp

- Deep **shell-specific hook injection** for each supported shell:
  - **ZSH**: `precmd` (before prompt) + `preexec` (before command); launched with `--no-rcs -g` then sources RC files manually
  - **Bash**: `bash-preexec` semantics; launched via `exec -a bash` with `--rcfile` process substitution
  - **Fish**: `--init-command` flag; launched with `--no-config` then `--login` with init script; disables Fish's own OSC 133 emission
  - **PowerShell**: `-NoLogo -NoProfile -NoExit -Command <init_script>`
- **Custom DCS (Device Control String) escape sequences** carrying **hex-encoded JSON payloads** — intercepted at the ANSI parser level (`dcs_hooks.rs`, 27KB)
- DCS hooks report: command text, current working directory, exit codes, environment variables, prompt content (PS1/rprompt), shell hostname
- This mechanism enables the **block model** — `preexec` DCS creates a new command block, `precmd` DCS finalizes the previous block
- 40KB `shell/mod.rs` covering shell detection, configuration, RC-file modification, and Warp-specific hook setup

### Athena's Core

- **No shell hooks or prompt injection** of any kind
- Agent commands are simply written to the PTY as text (`ptyProcess.write(agentCmd + '\r')`)
- **No command boundary detection** — output is an undifferentiated stream
- **No exit code capture** from the shell
- **No CWD tracking** from shell hooks — instead resolved via `lsof -a -p <pid> -d cwd -F n` (Unix only), which is fragile and requires the process to still be running
- Shell type determined by workspace config, not auto-detected

### Key Differences

- Warp injects hooks into every supported shell; Athena has zero shell integration
- Warp knows exactly when each command starts and ends; Athena has no command boundary awareness
- Warp captures exit codes, CWD changes, and environment info from the shell; Athena captures none of these
- Warp's DCS protocol enables structured data extraction from shell events; Athena's PTY is a pure byte pipe

---

## 4. Terminal Data Model (The Core Architectural Split)

### Warp — Block Model

- Each **command + output pair** is a separate **Grid** object (a "block")
- Output from one command **cannot overwrite** a previous block's content — enforced by a `block_filter.rs` (29KB) that intercepts and reroutes ANSI sequences at block boundaries
- **Early-output handler** (`early_output.rs`, 25KB) prevents output from being misattributed to the wrong block when commands emit before `precmd` fires
- Each block owns its own:
  - Grid (rows × columns of cells)
  - Scroll region and selection state
  - Rendering context
- The **Blockgrid** is a composite of all block grids, rendered as a scrollable list
- Each cell stores: character, foreground/background colors (true-color RGB), text attributes (bold/italic/underline/strikethrough), image references (iTerm2/Kitty image protocols), selection state
- Row-wise storage with efficient scrolling and line-wrapping, inherited from Alacritty

### Athena's Core — Flat Stream Model

- All output is a **sequential line buffer** per pane (`output-buffer-service.ts`)
- **5000 lines / 2MB cap** per pane buffer — oldest lines evicted on overflow
- Lines are **ANSI-stripped text** with line numbers and timestamps — no cell-level formatting preservation
- No concept of command blocks — output is an undifferentiated stream of `[lineNum] text` entries
- No structural separation between commands and their output
- Subscriber-based real-time streaming — callbacks fire on each new line
- Cursor-based pagination via `sinceLine` and `sinceTime` parameters

### Key Differences

- Warp's block model gives structural meaning to terminal output; Athena treats output as a flat log
- Warp preserves full formatting per cell (colors, attributes, images); Athena strips ANSI on ingestion
- Warp can render each block independently with its own scroll/selection; Athena has one scroll region per pane
- Warp's block filter prevents cross-block corruption; Athena has no such concern (no blocks to corrupt)
- Warp's early-output handler ensures correct attribution; Athena has no attribution concept at all

---

## 5. Session Persistence

### Warp

- **Diesel ORM + SQLite** with **40+ schema migrations** tracking the full data model evolution
- Persisted entities include:
  - **Windows, Tabs, Panes**: Full layout persistence (positions, titles, splits, active pane)
  - **Blocks**: Command text, output content (binary), timestamps, shell host, PS1/rprompt snapshots
  - **Commands**: Dedicated table for fast command history search
  - **Notebooks**: Persisted REPL-style notebook panes
  - **Workflows/Team Workflows**: Shared runbooks
  - **Object Metadata**: Warp Drive objects (cloud-synced)
  - **User Profiles**: Multi-profile support
  - **Folders**: Organization of Drive objects
- Sessions **restored on app restart** from persisted state
- Full schema defined in `crates/persistence/src/schema.rs`

### Athena's Core

- **Chat sessions**: File-per-session JSON in `userData/athena-sessions/` — simple flat files, no ORM
- **Images**: Stored as binary `.bin` files with UUID references, loaded on demand
- **Plugin registry**: Persisted in `electron-store` (JSON file under `userData/`)
- **Workspace/spaces/tasks**: Persisted in `electron-store` — key-value pairs, no schema migrations
- **No terminal output persistence** — all output buffers are in-memory only, lost on restart
- **No layout persistence** — pane arrangements not restored on restart
- Orphan image cleanup available via `cleanupOrphanedImages()` but not called automatically
- **Execution plans**: In-memory only (`plan-manager.ts`) — lost on restart

### Key Differences

- Warp uses a relational database with migrations; Athena uses flat JSON files and electron-store
- Warp persists everything (layout, blocks, commands, profiles); Athena persists chat history and settings only
- Warp restores full session state on restart; Athena loses all terminal/output/plan state
- Warp has 40+ schema migrations enabling safe evolution; Athena has no schema versioning
- Warp's persistence is queryable (SQLite); Athena's is simple key-value or file-per-entity

---

## 6. Output Capture & Buffering

### Warp

- **Grid-based cell storage** per block — each cell is a rich data structure
- Row-wise storage with efficient scrolling and line wrapping (Alacritty-derived)
- Block filter prevents cross-block ANSI leakage
- Early-output handler correctly attributes pre-prompt output
- ANSI parsing preserves all formatting — the grid cells hold the parsed result
- Content indexing (`indexing.rs`, 12KB) enables search within terminal output
- Images embedded via iTerm2/Kitty image protocols stored as cell references

### Athena's Core

- **In-memory line array** per pane (`output-buffer-service.ts`)
- `OutputLine` structure: `{ paneId, lineNum, timestamp, text }` — text only, no formatting
- **ANSI stripping on ingestion** — all escape codes removed via regex before storage
- **Dual buffering**: `ptyManager.ts` has its own 100KB raw-history chunks, and `output-buffer-service.ts` has the 5000-line/2MB stripped buffer — these are independent
- `output-capture.ts` bridges PTY lifecycle to the buffer service
- Subscriber callbacks fire per-line for real-time streaming to the renderer
- **No content indexing** — output is only iterable by line number or timestamp

### Key Differences

- Warp preserves the full richness of terminal output; Athena strips it to plain text
- Warp's per-block grid enables per-command scrolling and selection; Athena's flat buffer is one scroll region
- Warp has content indexing for search; Athena has sequential iteration only
- Warp's early-output handler solves attribution; Athena has no attribution concept
- Athena has dual buffering (raw in ptyManager + stripped in output-buffer-service); Warp has a single authoritative grid model

---

## 7. Notification System

### Warp

- No dedicated notification service module in the backend — notifications are handled at the UI/framework level through WarpUI
- AI-related notifications (agent status, input requests, completions) flow through the agent event system and WarpUI's action system
- System notifications (macOS Notification Center, etc.) handled natively per-platform

### Athena's Core

- Dedicated **notification-service.ts** (196 lines) with:
  - In-memory history array + `electron-store` persistence (500 max)
  - Typed notifications: `info`, `warning`, `error`, `success`, `needs_input`, `task_complete`, `task_error`
  - System notification integration via Electron's `Notification` API (with type-based icons and sound control)
  - Full IPC interface: `history`, `getCount`, `markRead`, `markAllRead`, `dismiss`, `clearAll`, `push`
  - Renderer event channels: `notifications:new`, `notifications:updated`, `notifications:dismissed`, `notifications:allRead`, `notifications:cleared`, `notifications:clicked`
  - Filtering by type, source, and read status
  - Async persistence — `persistHistory()` runs fire-and-forget after each mutation

### Key Differences

- Warp handles notifications implicitly through its UI framework; Athena has an explicit, queryable notification service
- Athena's notification service is persistent (survives restart via electron-store); Warp's are UI-level only
- Athena provides structured notification types, filtering, read/unread tracking, and batch operations
- Athena's notifications are accessible to external agents via the MCP `notify` tool; Warp's agent events are internal

---

## 8. File Operations

### Warp

- **`crates/warp_files/`**: Dedicated crate for file system operations
- **`crates/virtual_fs/`**: Virtual filesystem abstraction layer
- **`crates/watcher/`**: File watching (likely `notify` crate or similar)
- **`crates/warp_ripgrep/`**: ripgrep integration for content search — used by AI for code search
- **`crates/repo_metadata/`**: Repository metadata extraction
- **`crates/lsp/`**: Language Server Protocol client for code intelligence
- AI can read/write files directly via agent actions, with **diff validation** (`diff_validation.rs`) before applying changes
- Full codebase indexing with file outline extraction and source code embeddings

### Athena's Core

- **`fileSystem.ts`** (70 lines): Simple recursive directory tree reader
  - `readTree()`: Recursive with max depth 6, skips `node_modules`, `.git`, `.next`, `dist`, `build`, `.ade`
  - `readFileContent()` / `writeFileContent()`: Thin wrappers over `fs/promises`
  - `getDirectories()`: Lists subdirectory names only
- **File watching**: Chokidar-based via IPC — `fs:watchDir`/`fs:unwatchDir` with debounced (300ms) change events
- **Image reading**: `fs:readFileAsBase64` with MIME type detection by extension
- **Dialog support**: `fs:showOpenDialog` (directories), `fs:showImageDialog` (images with filters)
- **`fs:exists`**: Simple access-check via `fs/promises.access()`
- No content search, no LSP integration, no codebase indexing, no diff validation
- No virtual filesystem abstraction

### Key Differences

- Warp has a full file operations stack (virtual FS, ripgrep, LSP, repo metadata); Athena has basic read/write/tree only
- Warp has codebase indexing and source embeddings for AI context; Athena has none
- Warp validates AI-generated diffs before applying; Athena has no such safety check
- Warp has LSP integration for code intelligence; Athena has none
- Warp has a virtual filesystem abstraction; Athena accesses the real filesystem directly
- Athena has native OS dialog support (open directory, image picker); Warp handles this at the UI level

---

## 9. AI/Agent Architecture

### Warp

- AI is **deeply integrated** into the terminal fabric:
  - Reads terminal **block context** directly (`block_context.rs`)
  - Executes commands **in-terminal** — AI output appears as terminal blocks
  - Renders rich documents **inline** (markdown, code blocks, GFM tables)
- **Multi-agent orchestration** via protobuf API (`warp_multi_agent_api`)
- **Agent SDK** for building custom agents that run within Warp
- **Ambient/background agents** (Oz cloud platform) — scheduled, image-capable, RTC-enabled
- **Codebase indexing**: File outline extraction, full source code embeddings, relevant file discovery
- **Diff validation**: AI-generated diffs are validated before application
- **LLM abstraction**: Multiple providers with BYOK support, model/profile selector UI (80KB)
- **Skills system**: Pluggable AI capabilities
- **Voice input** for AI commands
- **AI prediction**: Auto-suggestions for commands based on context
- **Code review generation**, block title generation, fact extraction
- **Cloud execution environments** for agent sandboxes
- **`isolation_platform` crate**: Agent sandboxing
- Feature-gated with 30+ AI-related feature flags (agent_mode, mcp_server, cloud_mode, orchestration, etc.)

### Athena's Core

- AI is a **separate orchestration layer** (`athenaOrchestrator.ts`):
  - Dispatches tools that spawn/manipulate PTY sessions and send IPC events
  - AI never directly reads terminal content — only via `read_agent_output` tool (line buffer access)
  - AI responses returned as text strings to the chat UI, not rendered inline in terminals
- **Plan-Execute-Monitor-Evaluate workflow**: Structured execution plans with step-level dependency DAG
- **Two TCP servers** for external agent integration:
  - MCP server (port 4545): 11 tools for task management, notifications, output reading, agent messaging
  - Agent-Comms (port 4546): Session management, bidirectional messaging, input request/response
- **Tool registry** (12 tools): launch_builtin_agent, launch_custom_agent, close_terminals, run_command_in_terminals, read_agent_output, list_agents, check_agent_status, create_execution_plan, dispatch_plan_step, prompt_agent, ask_user, evaluate_results
- **Agentic loop guardrails**: Max 50 iterations, stall detection (5 consecutive identical tool calls), context compaction at 50 messages, image stripping beyond last 4 messages
- **Dual LLM provider support**: Anthropic SDK (native) + OpenAI-compatible (OpenAI, NVIDIA NIM, LM Studio)
- No codebase indexing, no diff validation, no ambient agents, no voice input, no skills system, no prediction

### Key Differences

- Warp's AI reads terminal state directly; Athena's AI accesses it only through tool calls
- Warp's AI operates within the terminal (output as blocks); Athena's AI is external (dispatches to terminals)
- Warp has multi-agent orchestration with protobuf APIs; Athena has plan-based step dispatch
- Warp has codebase indexing and diff validation; Athena has neither
- Warp has ambient/background agents and cloud execution; Athena has stalled-agent detection only
- Athena has explicit plan management (create/dispatch/evaluate); Warp's agent orchestration is more fluid
- Athena has agentic loop guardrails (stall detection, context compaction); Warp's safety mechanisms are at the agent action level
- Warp has 30+ AI feature flags; Athena has no feature flag system

---

## 10. Inter-Agent Communication

### Warp

- **Protobuf-based multi-agent API** (`warp_multi_agent_api`)
- **Agent shared sessions** — agents can share terminal session context
- **Cloud orchestration platform** (Oz) for coordinating distributed agents
- **Agent SDK** provides standardized communication interfaces

### Athena's Core

- **File-based mailbox system** (`.ade/mailbox/{agentId}.json`) for swarm coordination
  - Messages have: id, from, to, content, timestamp, read flag
  - Atomic writes via temp file + rename
- **TCP-based agent-comms** (port 4546) for live agent sessions
  - JSON-RPC over newline-delimited TCP with UUID token auth
  - Session management with status tracking (active/idle/waiting_input/disconnected)
  - Bidirectional messaging: `send_message_to_agent`, `read_agent_messages`
  - Pending input request/response via in-memory Promise map
- **Broadcast** to all connected agent sockets
- **MCP tool-level messaging**: `send_message_to_agent` and `read_agent_messages` MCP tools

### Key Differences

- Warp uses protobuf for structured inter-agent communication; Athena uses JSON-RPC over TCP + file-based mailboxes
- Warp has a cloud orchestration layer; Athena's coordination is purely local
- Warp has shared terminal sessions between agents; Athena's agents communicate via message passing only
- Athena's file-based mailbox enables cross-process coordination without shared memory; Warp relies on its platform

---

## 11. MCP Implementation

### Warp

- Full MCP **server and client** using `rmcp` (Rust MCP client crate)
- Supports multiple transports: **SSE**, **streamable HTTP**, and **child process**
- **OAuth support** for MCP connections (`mcp_oauth` feature flag)
- File-based MCP configuration (`file_based_mcp` feature flag)
- Can act as both MCP server and client
- Debugging support via `mcp_debugging_ids` feature flag

### Athena's Core

- MCP **server only** — custom JSON-RPC-over-TCP implementation on port 4545
- **Single transport**: newline-delimited TCP, localhost only (127.0.0.1)
- **UUID token auth** — token generated at app startup, passed to agents via env vars
- 11 tools exposed: create_tasks, get_next_task, update_task_status, spawn_agents, notify, status_update, get_output, list_agent_panes, athena_forward_output, send_message_to_agent, read_agent_messages
- No SSE, no HTTP transport, no OAuth, no MCP client capability
- MCP proxy (`bin/mcp-proxy.js`) used to bridge agent processes to the MCP server

### Key Differences

- Warp is both MCP server and client; Athena is server only
- Warp supports multiple transport protocols; Athena supports TCP only
- Warp has OAuth and debugging support; Athena has simple token auth only
- Warp uses a standard MCP library (`rmcp`); Athena implements MCP from scratch
- Athena uses a proxy process to bridge agents; Warp connects directly

---

## 12. Concurrency & Threading

### Warp

- **Dedicated PTY reader thread** with `mio::Poll`
- `FairMutex<TerminalModel>` — carefully documented locking rules to prevent deadlocks
- **GPU render thread** (Metal on macOS, Vulkan on Linux/Windows, WebGPU on WASM)
- **UI thread** (WarpUI framework) — separate from I/O and rendering
- Explicit guidance in WARP.md: prefer passing locked references down the call stack over re-acquiring locks

### Athena's Core

- **Single Node.js main process** — all backend modules share the same event loop
- PTY data events flow via **Electron IPC** to the renderer (asynchronous but single-threaded on backend)
- **xterm.js** runs in the renderer process — terminal rendering is separate from backend logic
- No locking concerns (single-threaded), but also no backend parallelism
- Output capture hooks are synchronous callbacks from PTY data events

### Key Differences

- Warp has 3+ threads (PTY I/O, GPU render, UI); Athena's backend is single-threaded
- Warp requires careful mutex management; Athena has no locking
- Warp's PTY reader is on a dedicated thread with bounded locking; Athena's PTY data flows through the Node.js event loop
- Warp's rendering is GPU-accelerated on a dedicated thread; Athena's is Chromium's renderer

---

## 13. Stalled Agent Detection

### Warp

- Handled at the cloud orchestration platform level (Oz)
- Agents managed through health/heartbeat systems in the multi-agent API
- No local stall detection in the terminal model

### Athena's Core

- **Two independent stall detectors**:
  - `swarmCoordinator.ts`: Polls `.ade/swarm-state.json` every 5s, marks agents as "stalled" after 90s of inactivity, emits `agent:stalled` app event
  - `agent-comms.ts`: Scans TCP sessions every 15s, marks sessions as "idle" after 90s of inactivity, emits `agents:statusUpdate` IPC event
- Stall detection triggers notifications via the notification service
- No integration between the two detectors — they operate independently

### Key Differences

- Warp delegates agent health to its cloud platform; Athena does local polling-based detection
- Athena has two independent stall detectors with overlapping responsibility; Warp has a unified platform approach
- Athena's stall detection is purely time-based; Warp's platform likely has richer health signals

---

## 14. Feature Flag System

### Warp

- **Sophisticated runtime feature flags** via `FeatureFlag` enum in `warp_core/src/features.rs`
- **200+ feature flags** in `app/Cargo.toml` controlling the product surface
- **Tiered rollout**: `DOGFOOD_FLAGS`, `PREVIEW_FLAGS`, `RELEASE_FLAGS`
- Preference: runtime checks over `#[cfg(...)]` compile-time gates for toggling
- Examples: `agent_mode`, `mcp_server`, `cloud_mode`, `completions_v2`, `plugin_host`, `full_source_code_embedding`, `voice_input`, `integration_tests`

### Athena's Core

- **No feature flag system** — all features are always active
- No mechanism for gradual rollout or A/B testing
- No way to disable features at runtime

### Key Differences

- Warp has a mature feature flag system with tiered rollout; Athena has none
- Warp can toggle features at runtime; Athena would require code changes
- Warp's feature flags enable safe experimentation; Athena ships everything to all users

---

## 15. Graceful Shutdown

### Warp

- Standard Rust process signal handling
- Terminal model cleanup handled by Rust's drop semantics
- SQLite connections closed gracefully via Diesel
- Cloud agent sessions terminated through the orchestration platform

### Athena's Core

- **Multi-step orchestrated shutdown** in `main.ts`:
  1. Send Ctrl+C (`\x03`) to all PTY sessions to interrupt running tasks
  2. Wait 50ms
  3. Send `/exit\r` to all sessions
  4. Wait 800ms for processes to save to disk
  5. Shut down agent-comms (close all TCP sockets, reject pending input requests, close TCP server)
  6. Shut down output capture (clear all buffers, null out window reference)
  7. Call `app.quit()`
- `before-quit` event is intercepted and `preventDefault()`-ed on first invocation to allow the cleanup sequence to complete

### Key Differences

- Warp relies on Rust's RAII/drop semantics for cleanup; Athena has an explicit multi-step sequence
- Athena sends specific signals (Ctrl+C, `/exit`) to terminal processes; Warp relies on process termination
- Athena has specific handling for pending agent input requests (rejects them on shutdown); Warp's agents are managed by the cloud platform

---

## 16. Plugin System

### Warp

- **JavaScript runtime** (`warp_js` + `rquickjs` QuickJS wrapper) for plugin execution
- **Node.js runtime** option (`node_runtime` crate) for plugins requiring Node.js
- Feature-gated behind `plugin_host` flag
- Plugins can contribute completions, themes, and other extensions

### Athena's Core

- **Plugin registry** in `electron-store` with lifecycle: install → enable → configure → disable → unregister
- **Agent-type plugins**: `claude-code-athena` and `opencode-athena` with shared connection library
- **MCP proxy** (`bin/mcp-proxy.js`) bridges agent processes to the MCP server
- **`agentMcpConfig.ts`**: Constructs env vars and spawn prefixes for each agent type:
  - `ATHENA_MCP_TOKEN`, `ATHENA_MCP_PORT`, `ATHENA_MCP_HOST`
  - `ATHENA_COMMS_TOKEN`, `ATHENA_COMMS_PORT`
  - `ATHENA_PANE_ID`, `ATHENA_SESSION_ID`
  - `CLAUDE_MCP_SERVERS` or `OPENCODE_MCP_SERVERS` (JSON config for MCP client)
- **`pluginHost.ts`**: Session management for connected agent plugins, event routing, status updates, plugin discovery/setup
- **Eager IPC registration** in `plugin-manager.ts` to prevent race conditions when renderer mounts before init completes

### Key Differences

- Warp has a JS runtime for executing plugin code in-process; Athena's plugins are external processes communicating via TCP
- Warp's plugin system is feature-gated; Athena's is always active
- Athena injects connection credentials via environment variables; Warp's plugins connect through its MCP framework
- Athena has specific plugin setup for each agent type (claude, opencode); Warp has a more general plugin model

---

## 17. Rendering Pipeline

### Warp

- **Custom WarpUI framework** — Flutter-inspired Entity-Component-Handle pattern
- **GPU-accelerated rendering**:
  - macOS: Cocoa + Metal
  - Linux/Windows: `winit` + `wgpu` (Vulkan/DirectX)
  - WASM: WebGPU/WGSL shaders
- Custom **glyph atlas** via `font-kit` for rasterization
- Primitive shaders: rect, image, glyph (~200 LOC each)
- Composited at **144+ FPS**
- Block list rendering: `BlockListElement` (208KB) + `GridRenderer` (103KB) + `BlockgridRenderer`

### Athena's Core

- **Electron renderer process** with Chromium's rendering pipeline
- **xterm.js** for terminal rendering (canvas-based, WebGL addon available)
- **React 19 + TailwindCSS** for UI components
- No custom GPU rendering pipeline
- Terminal rendering delegated to xterm.js's internal canvas/WebGL implementation

### Key Differences

- Warp has a custom GPU rendering framework; Athena uses Chromium's renderer + xterm.js
- Warp renders at 144+ FPS with custom shaders; Athena's FPS depends on Chromium
- Warp's UI framework is Flutter-inspired with entity-component architecture; Athena's is React-based
- Warp has per-block rendering with dedicated renderers; Athena renders terminals as xterm.js canvases

---

## 18. Data Flow Summary

### Warp

```
[Shell Process]
      │
      │ stdin/stdout via PTY slave
      ▼
[PTY Master FD] ◄── user keystrokes written to PTY master
      │
      │ raw bytes (ANSI + UTF-8 + DCS hooks)
      ▼
[PTY Reader Thread] ── mio::Poll (event-driven)
      │
      │ 256KB read buffer
      ▼
[ANSI Parser] ── VT100/VT510 state machine
      │         ├── DCS hooks → block creation / prompt detection
      │         ├── CSI/OSC → cursor movement, colors, etc.
      │         └── Raw text → grid cell updates
      ▼
[TerminalModel] ── Arc<FairMutex<TerminalModel>>
      │         ├── Blocks (each with own Grid)
      │         ├── Blockgrid (composite)
      │         └── Alt Screen buffer
      ▼
[UI Render Thread] ── WarpUI framework
      │         ├── BlockListElement
      │         ├── GridRenderer
      │         └── BlockgridRenderer
      ▼
[GPU Renderer] ── Metal / Vulkan / WebGPU
```

### Athena's Core

```
[Shell Process / Agent CLI]
      │
      │ stdin/stdout via node-pty
      ▼
[ptyManager.ts] ── Node.js main process
      │
      │ raw PTY data
      ├──► historyChunks (100KB, raw, for xterm replay)
      ├──► outputCaptureHooks.onData()
      │         ▼
      │    [output-buffer-service.ts] ── in-memory line array
      │         │   ANSI-stripped, numbered, timestamped
      │         │   5000 lines / 2MB per pane
      │         └──► subscriber callbacks → renderer IPC
      │
      └──► mainWindow.webContents.send(`pty:data:${id}`, data)
                │
                ▼
          [Renderer Process]
          xterm.js ── ANSI parsing + canvas/WebGL rendering
          React UI ── chat, panels, notifications
```

---

## 19. Architectural Philosophy Summary

| Dimension              | Warp                                                                 | Athena's Core                                                  |
| ---------------------- | -------------------------------------------------------------------- | -------------------------------------------------------------- |
| **Core principle**     | Terminal-first — AI and agents are extensions of the terminal        | Agent-first — the terminal is a managed resource for AI agents |
| **Data model**         | Block-structured — every command is a first-class object             | Stream-structured — output is a flat log of text lines         |
| **Shell relationship** | Deep integration via hooks, DCS, and prompt injection                | Arms-length — PTY is a byte pipe, no shell awareness           |
| **AI relationship**    | AI operates within the terminal, reading blocks and executing inline | AI orchestrates terminals externally via tool calls and IPC    |
| **Persistence**        | Relational (SQLite) — everything is queryable and migrated           | File-based (JSON) — simple, portable, but limited              |
| **Rendering**          | Custom GPU pipeline — maximum performance and control                | Chromium renderer — leverages web platform, less control       |
| **Extensibility**      | Feature flags + JS plugin runtime + MCP client/server                | Plugin registry + MCP server + TCP agent-comms                 |
| **Concurrency**        | Multi-threaded with explicit locking                                 | Single-threaded with IPC                                       |
| **Scope**              | Terminal emulator that gained AI                                     | AI orchestrator that manages terminals                         |
