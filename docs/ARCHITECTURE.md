# Architecture

## System Overview

Athena's Core is a Tauri 2 desktop application with a Dioxus (Rust/WASM) frontend. The architecture follows a clear separation between the WebView-rendered UI and the native Rust backend.

```
┌────────────────────────────────────────────────────────────────────┐
│                        Athena's Core                               │
├──────────────────────────────────┬─────────────────────────────────┤
│         WebView (Dioxus)         │        Native (Tauri)           │
│                                  │                                 │
│  ┌────────────────────────────┐  │  ┌───────────────────────────┐  │
│  │  App (lib.rs)              │  │  │  main.rs                   │  │
│  │  ├─ Sidebar                │  │  │  ├─ tauri::Builder         │  │
│  │  ├─ WorkspaceTabs          │  │  │  ├─ AppState::manage()     │  │
│  │  ├─ TerminalGrid           │  │  │  ├─ 134 invoke_handlers    │  │
│  │  ├─ AthenaPanel            │  │  │  └─ graceful_shutdown      │  │
│  │  ├─ KanbanBoard            │  │  └───────────────────────────┘  │
│  │  ├─ SwarmBoard             │  │                                 │
│  │  ├─ CommandPalette         │  │  ┌───────────────────────────┐  │
│  │  ├─ SettingsModal          │  │  │  commands/mod.rs           │  │
│  │  ├─ NotificationBell       │  │  │  ├─ window_*               │  │
│  │  ├─ PluginEventBus         │  │  │  ├─ fs_*                   │  │
│  │  └─ StatusBar              │  │  │  ├─ store_*                │  │
│  │                            │  │  │  ├─ session_*              │  │
│  │  ┌──────────────────────┐  │  │  │  ├─ pty_*                  │  │
│  │  │  15 Stores           │  │  │  │  ├─ athena_*               │  │
│  │  │  (provide/use_*)     │  │  │  │  ├─ notification_*         │  │
│  │  └──────────────────────┘  │  │  │  ├─ plan_*                 │  │
│  │                            │  │  │  ├─ agent_*                │  │
│  │  ┌──────────────────────┐  │  │  │  ├─ search_*               │  │
│  │  │  tauri_bridge        │  │  │  │  ├─ mcp_*                  │  │
│  │  │  invoke() / listen() │◄─┼──┼──┼─► swarm_*                 │  │
│  │  └──────────────────────┘  │  │  │  ├─ shell_integration_*    │  │
│  │                            │  │  │  ├─ tool_*                 │  │
│  │  ┌──────────────────────┐  │  │  │  ├─ browser_*              │  │
│  │  │  xterm.js            │  │  │  │  ├─ plugin_*               │  │
│  │  │  (via web_sys)       │◄─┼──┼──┼─► plugin_host_*           │  │
│  │  └──────────────────────┘  │  │  │  └─ security_*             │  │
│  └────────────────────────────┘  │  └───────────────────────────┘  │
│                                  │                                 │
│                                  │  ┌───────────────────────────┐  │
│                                  │  │  state.rs (AppState)       │  │
│                                  │  │  ├─ pty_manager            │  │
│                                  │  │  ├─ orchestrator           │  │
│                                  │  │  ├─ mcp_server             │  │
│                                  │  │  ├─ agent_comms            │  │
│                                  │  │  ├─ output_buffer          │  │
│                                  │  │  ├─ notification_service   │  │
│                                  │  │  ├─ plan_manager           │  │
│                                  │  │  ├─ swarm_coordinator      │  │
│                                  │  │  ├─ shell_integration      │  │
│                                  │  │  ├─ tool_executor          │  │
│                                  │  │  ├─ browser_manager        │  │
│                                  │  │  ├─ plugin_manager         │  │
│                                  │  │  ├─ store (KeyValueStore)  │  │
│                                  │  │  └─ session_store          │  │
│                                  │  └───────────────────────────┘  │
└──────────────────────────────────┴─────────────────────────────────┘
```

## Crate Descriptions

### src-tauri (`athenas-core`)

The Tauri application binary. Serves as the entry point and IPC bridge.

**Dependencies:** `tauri`, `tauri-plugin-shell`, `tauri-plugin-dialog`, `tauri-plugin-log`, `tauri-plugin-clipboard-manager`, `tauri-plugin-notification`, `tokio`, and all workspace crates. No in-app updater plugin is shipped in the current build.

**Key files:**

- `main.rs` — App builder, command registration, graceful shutdown handler
- `state.rs` — `AppState` struct holding all shared services behind `Arc<Mutex>` / `Arc<tokio::sync::Mutex>`
- `commands/mod.rs` — 134 `#[tauri::command]` functions organized by domain (per-domain modules re-exported from `mod.rs`); `commands/provider_config.rs` builds the LLM provider config from the store and exposes `llm_list_models` for the Settings model picker

### athena-frontend

Dioxus web application compiled to WASM. Renders the entire UI.

**Dependencies:** `dioxus`, `dioxus-web`, `wasm-bindgen`, `web-sys`, `gloo`, `serde`, `chrono`.

**Key modules:**

- `lib.rs` — Root `App` component with 15 stores, keyboard shortcuts, layout
- `components/` — 85+ RSX components organized by feature (terminal, athena, kanban, swarm, etc.)
- `stores/` — 15 signal-based stores matching Electron's Zustand stores
- `tauri_bridge.rs` — `invoke()` and `listen()` wrappers for Tauri IPC
- `themes/` — Theme definitions and CSS variable application
- `components/workspace/xterm_mount.rs` — xterm.js terminal integration via `web_sys`

### athena-browser

The browser is a native Tauri child WebView, not an iframe. Dioxus owns the toolbar and a measured placeholder; `src-tauri/src/commands/browser.rs` creates, closes, validates, and positions the child WebView. `BrowserManager` owns URL/title/loading/history state and receives native navigation, title, and page-load callbacks. The child accepts only validated HTTP(S) URLs and is explicitly closed during app shutdown. The frontend parks the child during sidebar/main-area relocation and reconciles its state from the returned model snapshot plus `browser:*` events. Browser commands are local-desktop-only and are intentionally excluded from the Mobile Mirror relay allowlist.

### athena-core

Core business logic: LLM orchestration, MCP server, agent communications, search, notifications.

**Dependencies:** `tokio`, `reqwest`, `serde`, `serde_json`, `thiserror`, `regex`, `uuid`, `chrono`, `log`.

**Modules:**

- `orchestrator.rs` — `AthenaOrchestrator`: dispatches messages to Anthropic/OpenAI-compatible APIs, handles tool call loops
- `llm_models.rs` — OpenAI-compatible `/models` list fetcher used by Settings → Athena to populate the model picker (auth header attached only when a key exists; URL validated against the same SSRF guard as chat)
- `mcp.rs` — `McpServer`: TCP JSON-RPC 2.0 server on port 4545, exposes 30 executor-backed and legacy-alias tools to external agents
- `agent_comms.rs` — `AgentComms`: TCP server on port 4546 for agent lifecycle (initialize, notify, status, input request, heartbeat)
- `tool_executor.rs` — Built-in tool implementations (create_tasks, get_next_task, update_task_status, notify, etc.)
- `output_buffer.rs` — Line-numbered, timestamped output capture per agent pane
- `notification.rs` — Notification service with history, read/unread tracking, counts
- `plan_manager.rs` — Plan/step tracking for AI-generated execution plans
- `search.rs` — Code search via ripgrep integration
- `shell_integration.rs` — OSC 633 parsing for shell integration sequences
- `shell_hooks.rs` — Shell hook processing
- `swarm.rs` — Swarm coordinator for multi-agent file-based message passing
- `types.rs` — Shared type definitions (LLMProvider, ImageData, SessionHistoryEntry, SearchOptions, etc.)

### athena-terminal

PTY session management using `portable-pty`.

**Dependencies:** `portable-pty`, `thiserror`, `log`.

**Key types:**

- `SessionManager` — Manages multiple PTY sessions by ID
- `PtyError` — Error enum for PTY operations
- Callback types: `OnDataCallback`, `OnReadyCallback`, `OnExitCallback`

### athena-store

Persistent storage layer.

**Dependencies:** `serde`, `serde_json`, `tokio`, `uuid`, `chrono`.

**Key types:**

- `KeyValueStore` — File-based JSON key-value store (compatible with electron-store)
- `SessionStore` — Chat session CRUD with message history
- Image storage with orphan cleanup

### athena-fs

File system utilities and directory watching.

### athena-plugins

Plugin system with manifest parsing, event bus, and session management.

## Data Flow

### Backend → Frontend (Events)

```
PTY Output
  │
  ▼
SessionManager.read_pty_loop()
  │
  ▼
on_data callback → app.emit("terminal:data:{pane_id}", payload)
  │
  ▼
tauri_bridge.listen("terminal:data:{pane_id}") → terminalStore.receiveData()
  │
  ▼
xterm.terminal.write(data) → rendered in TerminalPane
```

```
LLM Response
  │
  ▼
AthenaOrchestrator.send_message() → returns text
  │
  ▼
athena_chat command returns Result<String>
  │
  ▼
tauri_bridge.invoke("athena_chat") → athenaStore receives response
  │
  ▼
AthenaPanel renders message in chat history
```

```
Agent Notification
  │
  ▼
AgentComms.handle_notification() → emit_to_renderer("agents:statusUpdate", data)
  │
  ▼
app.emit("agents:statusUpdate", payload)
  │
  ▼
tauri_bridge.listen("agents:statusUpdate") → agentStatusStore.update()
  │
  ▼
Sidebar agent list updates status indicator
```

### Frontend → Backend (Commands)

```
User types in terminal
  │
  ▼
TerminalPane.onInput(data)
  │
  ▼
tauri_bridge.invoke("pty_write", { id, data })
  │
  ▼
pty_write command → pty_manager.write(&id, data)
  │
  ▼
SessionInner.writer.write_all(data) → PTY receives input
```

```
User sends chat message
  │
  ▼
AthenaInput.onSubmit(message)
  │
  ▼
tauri_bridge.invoke("athena_chat", { message })
  │
  ▼
athena_chat command → orchestrator.send_message() → LLM API
  │
  ▼
Response returned to frontend
```

## IPC Mechanism

### Tauri Commands (Request/Response)

The frontend calls `tauri_bridge.invoke(command, payload)` which maps to `#[tauri::command]` functions in `commands/mod.rs`. Commands are synchronous or async, returning `Result<T, String>` or `Result<T, CommandError>`.

```rust
// Frontend
let result = tauri_bridge::invoke("pty_spawn", &spawn_args).await?;

// Backend
#[tauri::command]
pub fn pty_spawn(state: State<'_, AppState>, id: String, cwd: String, shell: String) -> Result<(), String> {
    let manager = state.pty_manager.lock().map_err(|e| e.to_string())?;
    manager.spawn(id, cwd, shell, None).map_err(|e| e.to_string())
}
```

### Tauri Events (Pub/Sub)

The backend emits events via `app.emit(event_name, payload)` which the frontend subscribes to via `tauri_bridge::listen(event_name)`.

```rust
// Backend (in pty_manager callbacks)
app.emit(&format!("terminal:data:{}", id), payload)?;

// Frontend
tauri_bridge::listen(&format!("terminal:data:{}", pane_id), |payload| {
    terminal_store.receive_data(&pane_id, &payload.data);
});
```

## State Management

### Frontend (Dioxus Signals)

Each store is a `Signal<T>` provided via context:

```rust
// Provider (in App component)
pub fn provide_ui_store() {
    provide_context(use_signal(UiState::default));
}

// Consumer
pub fn use_ui_store() -> UseSignal<UiState> {
    consume_context()
}
```

15 stores mirror the Electron app's Zustand stores:

- `ui` — Theme, panel, sidebar, modals, font settings
- `workspace` — Spaces, panes, active space
- `athena` — Chat messages, status, session context
- `notification` — Bell count, history, toasts
- `editor` — Open files, tabs, active file
- `terminal` — Pane list, terminal refs, data buffers
- `layout` — Panel sizes, split ratios
- `session` — Chat session list
- `swarm` — Swarm state, agent list
- `task` — Kanban tasks, columns
- `command` — Command palette items
- `agent_output` — Captured output per agent
- `agent_status` — Agent connection status
- `panel_manager` — Panel state
- `plugin_bus` — Plugin event bus

### Backend (AppState)

All services live in `AppState`, managed by Tauri's `app.state::<AppState>()`:

```rust
pub struct AppState {
    pub pty_manager: Arc<Mutex<SessionManager>>,
    pub orchestrator: Arc<tokio::sync::Mutex<AthenaOrchestrator>>,
    pub mcp_server: Arc<tokio::sync::Mutex<McpServer>>,
    pub agent_comms: AgentComms,
    pub output_buffer: Arc<OutputBuffer>,
    pub notification_service: Arc<NotificationService>,
    pub plan_manager: Arc<Mutex<PlanManager>>,
    pub swarm_coordinator: Arc<tokio::sync::Mutex<SwarmCoordinator>>,
    pub shell_integration_parser: Arc<Mutex<ShellIntegrationParser>>,
    pub tool_executor: Arc<Mutex<ToolExecutor>>,
    pub browser_manager: Arc<Mutex<BrowserManager>>,
    pub plugin_manager: Arc<Mutex<PluginManager>>,
    pub store: Arc<KeyValueStore>,
    pub session_store: Arc<SessionStore>,
    pub pending_questions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    app_handle: Mutex<Option<AppHandle>>,
}
```

## Network Services

### MCP Server (Port 4545)

JSON-RPC 2.0 over TCP. Exposes Athena's tool interface to external agents and plugins.

**Protocol:**

1. Client connects to `127.0.0.1:4545`
2. Sends `{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"token":"<uuid>"}}`
3. Server validates token, returns capabilities
4. Client sends `tools/list` to discover available tools
5. Client calls tools via `tools/call` with `{"name":"...","arguments":{...}}`

**Available tools (21):** `close_terminals`, `launch_builtin_agent`, `run_command_in_terminals`, `launch_custom_agent`, `read_agent_output`, `list_agents`, `check_agent_status`, `create_execution_plan`, `dispatch_plan_step`, `prompt_agent`, `ask_user`, `evaluate_results`, `kanban_list_tasks`, `kanban_create_task`, `kanban_update_task`, `kanban_delete_task`, `fs_read_file`, `fs_list_dir`, `fs_search`, `workspace_list`, `workspace_get_active`, `workspace_switch`. Source of truth: `crates/athena-core/src/tool_schema.rs`.

### Agent Comms (Port 4546)

JSON-RPC over TCP for agent lifecycle management.

**Protocol:**

1. Agent connects to `127.0.0.1:4546`
2. Sends `initialize` with token, pluginId, agentId
3. Server registers session, returns sessionId + capabilities
4. Agent sends: `notifications/message`, `agents/status`, `agents/requestInput`, `agents/heartbeat`
5. Server can send messages to agents via `send_to_agent()`

**Message types:** `Notification`, `StatusUpdate`, `InputRequest`, `Error`, `Completion`, `Heartbeat`, `Register`

## Security Model

### Path Validation

All file system commands validate paths against `$HOME`:

```rust
fn validate_path(path: &Path) -> Result<PathBuf, CommandError> {
    let home = std::env::var("HOME")?;
    let cleaned = path.clean();
    if !cleaned.starts_with(Path::new(&home)) {
        return Err(CommandError::PermissionDenied(...));
    }
    // Double-check after canonicalization
    ...
}
```

### API Key Storage

API keys are stored in the OS keychain via the `keyring` crate, with fallback to the plaintext `KeyValueStore` for legacy compatibility.

### MCP Authentication

Both MCP and Agent Comms servers require a UUID token for initialization. Connections without valid tokens are rejected.

### Tauri Capabilities

- The main window capability (`src-tauri/capabilities/default.json`) grants the 134 command-aligned custom permissions plus core/dialog/window/event/clipboard entries. It is **not** least-privilege by feature area; the backend validators are the primary security boundary. See [`docs/release/CAPABILITY_PLUGIN_INVENTORY.md`](release/CAPABILITY_PLUGIN_INVENTORY.md) for the full inventory and the C-1 finding regarding future capability splitting.
- Plugins in use: `tauri-plugin-shell`, `tauri-plugin-dialog`, `tauri-plugin-log`, `tauri-plugin-clipboard-manager`, `tauri-plugin-notification`, `tauri-plugin-window-state`.
- Release delivery uses signed/notarized DMG publication and a documented manual-update runbook; no in-app updater plugin or update endpoint is included in this build. See [`docs/release/UPDATER_DECISION_0.3.0.md`](release/UPDATER_DECISION_0.3.0.md).

Filesystem and process access are gated by the explicit command handlers in `src-tauri/src/commands/` and the path sandbox in `athena-fs`. The filesystem root resolver accepts only a verified source-tree marker or standard macOS bundle `Contents/Resources`; it does not fall back to `/`.

### Content Security Policy

A Content Security Policy is enforced on the WebView to prevent unauthorized script execution.
