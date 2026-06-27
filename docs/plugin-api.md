# Athena Plugin API — Developer Guide

> **Version:** 1.2.0
> **Last Updated:** 2026-06-27
> **Crate:** `crates/athena-plugins`

---

## 1. Overview

The Athena Plugin system enables external tools and AI agents to communicate bidirectionally with Athena's Core via the **Model Context Protocol (MCP)**. Plugins run as separate child processes and communicate with Athena through MCP tools and events.

### What Plugins Can Do

- Send notifications to the user
- Report agent status and progress
- Request user input (blocking)
- Create and manage tasks on the Kanban board
- Read captured terminal output from agent panes
- Stream agent output in real time
- Spawn new agent workers
- Emit and subscribe to plugin events

### Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Athena Desktop App (Tauri + Dioxus)                        │
│                                                              │
│  ┌──────────────────┐      ┌──────────────────────┐        │
│  │  Frontend UI      │◄────►│  Tauri Backend        │        │
│  │  (Dioxus/WASM)    │ IPC  │                       │        │
│  └──────────────────┘      │  ┌─────────────────┐  │        │
│                            │  │ athena-plugins  │  │        │
│                            │  │  PluginManager   │  │        │
│                            │  │  - Registry      │  │        │
│                            │  │  - Sessions      │  │        │
│                            │  │  - Events        │  │        │
│                            │  └────────┬────────┘  │        │
│                            └───────────┼───────────┘        │
│                                       │                    │
│                            ┌──────────▼──────────┐          │
│                            │   MCP Server        │          │
│                            │   (TCP port 4545)   │          │
│                            └──────────┬──────────┘          │
└───────────────────────────────────────┼───────────────────────┘
                                        │ MCP JSON-RPC
                     ┌──────────────────┼──────────────────┐
                     │                  │                  │
               ┌─────▼─────┐    ┌──────▼──────┐   ┌──────▼──────┐
               │  Claude   │    │   Codex     │   │   Custom    │
               │  Code     │    │ / OpenCode  │   │   Agent     │
               └───────────┘    └─────────────┘   └─────────────┘```

### Key Rust Types (from `athena-plugins` crate)

| Type | File:Line | Purpose |
|------|-----------|---------|
| `PluginManager` | `src/lib.rs:571` | Central registry, session, and event coordinator |
| `PluginManifest` | `src/lib.rs:196` | Plugin metadata, capabilities, and installation config |
| `PluginEntry` | `src/lib.rs:224` | Installed plugin combining manifest + runtime status |
| `PluginSession` | `src/lib.rs:252` | Live connection between a plugin and an agent |
| `PluginEvent` | `src/lib.rs:315` | An event emitted by a plugin, broadcast to subscribers |
| `PluginCallbacks` | `src/lib.rs:353` | Trait for hooking plugin lifecycle and events |

---

## 2. Plugin Lifecycle

Plugins move through a well-defined state machine managed by `PluginManager`.

### 2.1 Status States

```rust
pub enum PluginStatus {
    Installed,   // Freshly registered, not yet enabled
    Enabled,     // Active and running
    Disabled,    // Stopped but retained in registry
    Error,       // Encountered an unrecoverable error
}
```

### 2.2 Lifecycle Flow

```
Discovery → Registration → Installed → Enabled → Disabled → Unregistered
                                            ↓
                                          Error → (can be re-enabled)
```

### 2.3 Lifecycle Methods

All methods are on `PluginManager`, which is accessed via T commands from the frontend or directly in Rust.

#### Discovery

```rust
impl PluginManager {
    /// Scan a directory for `.json` manifest files and return parsed manifests.
    /// Invalid files are captured as errors but do not abort the scan.
    pub fn discover_plugins(&self, dir: &Path)
        -> Result<Vec<Result<PluginManifest, PluginError>>, PluginError>;

    /// Discover and auto-register all valid plugins in a directory.
    /// Returns (registered_ids, errors).
    pub fn discover_and_register(&self, dir: &Path)
        -> Result<(Vec<String>, Vec<PluginError>), PluginError>;
}
```

- Manifests must end in `.json`
- Manifests larger than `MAX_MANIFEST_BYTES` (1 MiB) are skipped
- Each manifest is validated before being registered

#### Registration

```rust
impl PluginManager {
    /// Register a plugin from its manifest.
    /// Returns the plugin ID on success.
    /// Fails if the same ID is already active (but allows re-registration after disable).
    pub fn register_plugin(&self, manifest: PluginManifest) -> Result<String, PluginError>;
}
```

**Tauri command:** `plugin_register(plugin_id, name, version)`

**Callbacks fired:**
- `PluginCallbacks::on_plugin_registered(plugin_id, name)`
- Internal event: `"plugin:registered"`
- Registry update event: `"plugin:registryUpdated"`

#### Enable

```rust
impl PluginManager {
    /// Enable a previously installed/disabled plugin.
    pub fn enable_plugin(&self, plugin_id: &str) -> Result<(), PluginError>;
}
```

**Tauri command:** `plugin_enable(plugin_id)`

**Callbacks fired:**
- `PluginCallbacks::on_plugin_enabled(plugin_id, name)`
- Internal event: `"plugin:enabled"`
- Registry update event

**Idempotent:** calling `enable_plugin` on an plugin that is already enabled returns `Ok(())`.

#### Disable

```rust
impl PluginManager {
    /// Disable an enabled plugin.
    pub fn disable_plugin(&self, plugin_id: &str) -> Result<(), PluginError>;
}
```

**Tauri command:** `plugin_disable(plugin_id)`

**Callbacks fired:**
- `PluginCallbacks::on_plugin_disabled(plugin_id)`
- Internal event: `"plugin:disabled"`
- Registry update event

#### Unregister (Unload)

```rust
impl PluginManager {
    /// Unregister a plugin by ID.
    pub fn unregister_plugin(&self, plugin_id: &str) -> Result<(), PluginError>;
}
```

**Tauri command:** `plugin_unregister(plugin_id)`

Removes the plugin from the registry entirely. If the plugin was enabled, `on_plugin_disabled` is fired first.

#### Error State

```rust
impl PluginManager {
    /// Set a plugin to error status with an error message.
    pub fn set_plugin_error(&self, plugin_id: &str, error: &str) -> Result<(), PluginError>;
}
```

**Tauri command:** `plugin_set_error(plugin_id, error)`

Transitions a plugin to `PluginStatus::Error` and stores the error message.

**Callbacks fired:**
- `PluginCallbacks::on_plugin_error(plugin_id, error)`
- Internal event: `"plugin:error"`
- Registry update event

---

## 3. Host API — Plugin Capabilities

Plugins declare the capabilities they need in their manifest. Athena enforces these capabilities when routing events and handling MCP tool calls.

### 3.1 Capability Enum

```rust
pub enum PluginCapability {
    Notifications,   // Send user-facing notifications
    Status,          // Report agent status
    Tasks,           // Create/manage Kanban tasks
    AgentControl,    // Pause/resume/cancel agent panes
    UserInput,       // Request user input (blocking)
    FileAccess,      // Read/write workspace files
    Swarm,           // Spawn swarm workers
}
```

### 3.2 Default Capability Grants by Agent Type

When a plugin session registers, it declares an `AgentType`. Capabilities are scoped to the intersection of what the manifest requests and what the agent type is allowed:

| Agent Type | Default Capabilities |
|------------|---------------------|
| `Claude` | `Notifications`, `Status`, `Tasks`, `UserInput` |
| `Codex` | `Notifications`, `Status`, `Tasks`, `UserInput` |
| `Opencode` | `Notifications`, `Status`, `Tasks`, `UserInput` |
| `Gemini` | `Notifications`, `Status`, `Tasks`, `UserInput` |
| `Custom` | `Notifications`, `Status` |
| `Shell` | `Notifications`, `Status` |

If a requested capability is not in the default set, it is filtered out.

---

## 4. Plugin Manifest Format

A plugin manifest is a JSON file (`.json` extension) that fully describes the plugin. These files are discovered in plugin directories and loaded by `PluginManager::discover_plugins()`.

### 4.1 Schema

```jsonc
{
  // ── Required ──────────────────────────────────────────────────
  "id": "com.example.my-plugin",
  "name": "My Plugin",
  "version": "1.0.0",
  "description": "What this plugin does",
  "author": "Your Name",

  // ── Optional ──────────────────────────────────────────────────
  "permissions": ["notifications", "status"],
  "capabilities": ["notifications", "status"],
  "tools": [
    {
      "name": "my_tool",
      "description": "Does something useful",
      "inputSchema": { /* JSON Schema */ },
      "capability": "notifications",
      "phase": 1
    }
  ],
  "subscribesTo": ["notification", "status_update"],
  "minAthenaVersion": "0.1.0",

  // ── MCP Configuration (stdio transport) ───────────────────────
  "mcpConfig": {
    "command": "node",
    "args": ["./dist/index.js"],
    "env": { "NODE_ENV": "production" }
  },

  // ── Installation method ───────────────────────────────────────
  "install": {
    "type": "builtin"
    // OR
    // "type": "mcp_server",
    // "command": "node",
    // "args": ["./server.js"],
    // "env": { "KEY": "val" }
    // OR
    // "type": "hook",
    // "script": "scripts/setup.sh"
  },

  // ── User configuration (optional) ─────────────────────────────
  "config": {
    "schema": { /* JSON Schema defining settings */ },
    "defaults": { "timeout": 30 }
  }
}
```

### 4.2 Rust Struct (Serde-mapped)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    #[serde(default)]
    pub permissions: Vec<PluginCapability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mcp_config: Option<McpConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_athena_version: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<PluginCapability>,
    #[serde(default)]
    pub tools: Vec<PluginToolDefinition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subscribes_to: Option<Vec<PluginEventType>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<PluginConfigSchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub install: Option<PluginInstallMethod>,
}
```

### 4.3 Field Descriptions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `id` | `string` | Yes | Unique reverse-DNS identifier (e.g. `com.athena.notifications`) |
| `name` | `string` | Yes | Human-readable display name |
| `version` | `string` | Yes | Semver version |
| `description` | `string` | Yes | Short summary of what the plugin does |
| `author` | `string` | Yes | Plugin author or organization |
| `permissions` | `PluginCapability[]` | No | Required permissions for this plugin |
| `capabilities` | `PluginCapability[]` | No | Capabilities the plugin requires |
| `tools` | `PluginToolDefinition[]` | No | MCP tool definitions exposed by this plugin |
| `subscribesTo` | `PluginEventType[]` | No | Event types this plugin should receive |
| `mcpConfig` | `McpConfig` | No | MCP server command/args/env |
| `install` | `PluginInstallMethod` | No | How the plugin is installed/invoked |
| `config` | `PluginConfigSchema` | No | JSON schema + defaults for plugin settings |
| `minAthenaVersion` | `string` | No | Minimum Athena version required |

---

## 5. Example Plugin — Minimal Working Example

### 5.1 File Structure

```
my-athena-plugin/
├── athena-plugin.json    // Must end in .json
├── index.js              // MCP server entry point
└── package.json
```

### 5.2 `athena-plugin.json`

```json
{
  "id": "com.example.status-reporter",
  "name": "Status Reporter",
  "version": "1.0.0",
  "description": "Reports agent status back to Athena",
  "author": "Example Dev",
  "capabilities": ["notifications", "status"],
  "subscribesTo": [],
  "mcpConfig": {
    "command": "node",
    "args": ["./index.js"]
  },
  "install": {
    "type": "mcp_server",
    "command": "node",
    "args": ["./index.js"]
  }
}
```

### 5.3 MCP Server (`index.js` — Node.js)

```javascript
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'

const server = new McpServer({
  name: 'status-reporter',
  version: '1.0.0',
})

// Tool: Report status to Athena
server.tool(
  'report_status',
  'Report the agent\'s current status to Athena',
  {
    status: z.enum(['idle', 'working', 'thinking', 'error']).describe('Current agent status'),
    message: z.string().optional().describe('Optional status detail'),
  },
  async ({ status, message }) => {
    // This would use Athena's MCP server tools (notify, status_update, etc.)
    console.log(`Status reported: ${status} - ${message}`)
    return {
      content: [{ type: 'text', text: `Status ${status} reported.` }],
    }
  },
)

// Start the server
const transport = new StdioServerTransport()
await server.connect(transport)
```

### 5.4 Registering the Plugin (Frontend — Dioxus/Rust)

```rust
use athena_plugins::{PluginManager, PluginManifest, PluginStatus};

let manager = PluginManager::new();

// Register from a manifest loaded from disk
let manifest: PluginManifest = serde_json::from_str(&manifest_json)?;
let plugin_id = manager.register_plugin(manifest)?;

// Enable it
manager.enable_plugin(&plugin_id)?;

// Check status
let info = manager.get_plugin_info(&plugin_id).unwrap();
assert_eq!(info.status, PluginStatus::Enabled);
```

### 5.5 Reacting to Plugin Events (in a Plugin)

Plugins can subscribe to events they care about during session registration:

```rust
use athena_plugins::{
    PluginManager, AgentType, PluginEventType,
};

let session = manager.register_session(
    "com.example.my-plugin",
    AgentType::Claude,
    Some("agent-123".to_string()),
    Some("pane-1".to_string()),
    None, // use default capabilities for Claude
)?;

// Subscribe to notifications and status updates
manager.subscribe_session(
    &session.id,
    &[
        PluginEventType::Notification,
        PluginEventType::StatusUpdate,
    ],
)?;
```

---

## 6. Events — Emitting and Listening

The plugin event system supports both pushing events from plugins and subscribing to events from within plugins.

### 6.1 Available Event Types

```rust
pub enum PluginEventType {
    Notification,      // User-facing notification
    StatusUpdate,      // Agent status changed
    TaskComplete,      // Agent finished a task
    TaskError,         // Agent encountered an error
    NeedsInput,        // Agent is waiting for user input
    AgentSpawned,      // New agent spawned
    AgentExited,       // Agent terminated
    AgentStalled,      // Agent timed out / unresponsive
    ProgressUpdate,    // Agent reports incremental progress
    ArtifactProduced,  // File/output created
    UserResponse,      // User responded to an input request
    ControlCommand,    // App sent a control command
    AgentConnected,    // Agent connected to MCP server
    AgentDisconnected, // Agent disconnected from MCP server
    PluginRegistered,  // New plugin registered
    PluginError,       // Plugin error occurred
    OutputForwarded,   // Terminal output forwarded
}
```

### 6.2 Event Structure

```rust
pub struct PluginEvent {
    pub id: String,                      // evt-{uuid}
    pub event_type: PluginEventType,     // Event type ("type" in JSON)
    pub source: PluginEventSource,       // Metadata about who emitted the event
    pub payload: PluginEventPayload,     // Rich event data
    pub timestamp: i64,                  // Unix ms
}

pub struct PluginEventSource {
    pub session_id: String,
    pub pane_id: Option<String>,
    pub agent_type: String,
    pub agent_id: Option<String>,
}
```

### 6.3 Event Payload

```rust
pub struct PluginEventPayload {
    pub level: Option<PayloadLevel>,          // info, warning, error, success
    pub message: Option<String>,
    pub title: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub task_title: Option<String>,
    pub result: Option<String>,
    pub error: Option<String>,
    pub prompt: Option<String>,             // For input requests
    pub options: Option<Vec<String>>,       // For input options
    pub request_id: Option<String>,          // For pending requests
    pub response: Option<String>,           // For user responses
    pub exit_code: Option<i32>,
    pub command: Option<String>,
    pub session_id: Option<String>,
    pub agent_id: Option<String>,
    pub plugin_id: Option<String>,
}

pub enum PayloadLevel { Info, Warning, Error, Success }
```

### 6.4 Emitting an Event

```rust
use athena_plugins::{PluginEventType, PluginEventSource, PluginEventPayload, PayloadLevel};

let event = manager.emit_plugin_event(
    PluginEventType::Notification,
    PluginEventSource {
        session_id: "sess-abc".to_string(),
        pane_id: Some("pane-1".to_string()),
        agent_type: "claude".to_string(),
        agent_id: Some("agent-123".to_string()),
    },
    PluginEventPayload {
        level: Some(PayloadLevel::Info),
        title: Some("Build complete".to_string()),
        message: Some("All 42 tests passed.".to_string()),
        ..Default::default() // PluginEventPayload requires manual default init
    },
);
```

**Note:** `PluginEventPayload` does not derive `Default`. Construct it explicitly or use a helper.

Emitting an event:
- Generates a unique event ID and timestamp
- Calls `PluginCallbacks::on_plugin_event(&event)`
- Emits `"plugin:event"` via the event emitter (Tauri → renderer)
- Returns the full event

### 6.5 Subscribing to Events

Sessions can subscribe to specific event types:

```rust
manager.subscribe_session(
    "session-id-uuid",
    &[
        PluginEventType::Notification,
        PluginEventType::StatusUpdate,
        PluginEventType::TaskComplete,
        PluginEventType::OutputForwarded,
    ],
)?;
```

Sessions that are `Disconnected` are automatically excluded from subscriber lists.

---

## 7. Session Management

A **plugin session** represents a live connection between a plugin and an agent.

### 7.1 Session Structure

```rust
pub struct PluginSession {
    pub id: String,                  // UUID
    pub plugin_id: String,
    pub agent_type: AgentType,
    pub agent_id: String,
    pub pane_id: Option<String>,
    pub capabilities: Vec<PluginCapability>,
    pub connected_at: i64,           // Unix ms
    pub last_activity_at: i64,     // Unix ms
    pub status: SessionStatus,
}

pub enum SessionStatus {
    Active,       // Currently communicating
    Idle,         // No activity for > stall_timeout (default 90s)
    WaitingInput, // Waiting for user input
    Disconnected, // Session closed
}
```

### 7.2 Agent Type Enum

```rust
pub enum AgentType {
    Claude,   // Claude Code agent
    Codex,    // OpenAI Codex agent
    Opencode, // OpenCode agent
    Gemini,   // Google Gemini agent
    Custom,   // Custom agent
    Shell,    // Plain shell session
}
```

### 7.3 Session Lifecycle Methods

```rust
impl PluginManager {
    /// Create a new session. Generates a UUID.
    /// If `agent_id` is None, auto-generates "agent-{short_uuid}".
    pub fn register_session(
        &self,
        plugin_id: impl Into<String>,
        agent_type: AgentType,
        agent_id: Option<String>,     // Optional custom agent ID
        pane_id: Option<String>,      // Optional terminal pane
        requested_capabilities: Option<Vec<PluginCapability>>,
    ) -> Result<PluginSession, PluginError>;

    /// Get a session by ID.
    pub fn get_session(&self, session_id: &str) -> Option<PluginSession>;

    /// Find a session by agent ID.
    pub fn get_session_by_agent_id(&self, agent_id: &str) -> Option<PluginSession>;

    /// Update the status of a session (updates last_activity_at).
    pub fn update_session_status(
        &self,
        session_id: &str,
        status: SessionStatus,
        data: Option<&serde_json::Value>,
    ) -> Result<(), PluginError>;

    /// Remove a session. Also cleans up subscriptions and pending messages.
    pub fn remove_session(&self, session_id: &str) -> Result<(), PluginError>;

    /// List all sessions.
    pub fn list_sessions(&self) -> Vec<PluginSession>;
}
```

### 7.4 Health Checking

```rust
impl PluginManager {
    /// Check all sessions for staleness. Inactive sessions (not Disconnected or WaitingInput)
    /// with no activity for longer than the stall timeout are marked Idle.
    /// Default stall timeout: 90 seconds.
    pub fn health_check(&self) -> Result<HealthCheckResult, PluginError>;
}

pub struct HealthCheckResult {
    pub checked_at: i64,
    pub total_sessions: usize,
    pub active_sessions: usize,
    pub idle_sessions: usize,
    pub stalled_sessions: usize,
    pub disconnected_sessions: usize,
    pub stalled_session_ids: Vec<String>,
}
```

Set a custom stall timeout:

```rust
use std::time::Duration;
manager.set_stall_timeout(Duration::from_secs(120))?;
```

---

## 8. Security Model

### 8.1 Manifest Validation

Every manifest is validated before registration. The following checks prevent arbitrary code execution:

#### Hook Script Validation

Hook scripts must be simple relative paths:

| Check | Rule |
|-------|------|
| Absolute paths | Rejected (must be relative) |
| Path traversal (`../`) | Rejected |
| Shell metacharacters (`;`, `|`, `&`, `$`, `` ` ``, `\n`) | Rejected |

#### MCP Command Validation

MCP commands must be whitelisted bare executable names:

| Status | Commands |
|--------|----------|
| Allowed | `node`, `python`, `python3`, `ruby`, `cargo`, `sh`, `bash`, `zsh`, `npx`, `deno`, `uv`, `uvx`, `pipx` |
| Rejected | Absolute paths, relative paths (`./`), paths with `/` |

#### MCP Environment Variable Restrictions

| Forbidden | Reason |
|-----------|--------|
| `PATH` | Environment modification attack, executing untrusted binaries |
| `HOME` | Directory hijacking |

All other environment variables are allowed.

### 8.2 Session Authentication

- `ATHENA_MCP_TOKEN` is generated per app instance and injected into agent PTY sessions.
- Session tokens are passed during the MCP `initialize` handshake.
- Tokens are not persisted; a new token is generated on each app restart.
- The MCP TCP server binds to `127.0.0.1:4545` (localhost only).

### 8.3 Scoped Capabilities

- Plugin sessions declare their `agent_type` during registration.
- Capabilities are scoped by `default_capabilities(agent_type)`.
- Requested capabilities outside the scope are silently filtered out (`scoped_capabilities`).
- Full AI agents (Claude, Codex, OpenCode, Gemini) get broader defaults than `Custom` or `Shell`.

### 8.4 What Plugins Can and Cannot Do

| Can Do | Cannot Do |
|--------|-----------|
| Send notifications via `notify` tool | Access the renderer DOM or change UI directly |
| Report agent status | Execute arbitrary system commands (MCP whitelisting) |
| Request user input (blocking) | Override `PATH` or `HOME` in MCP env |
| Read task board | Read other agents' output without `output_read` cap |
| Spawn new agent workers (with `swarm` capability) | Access arbitrary files outside the workspace |
| Read terminal output (own output is always permitted) | |
| Subscribe to plugin events | |

---

## 9. Error Handling

### 9.1 Error Types

```rust
pub enum PluginError {
    PluginNotFound(String),           // Plugin ID not in registry
    SessionNotFound(String),          // Session ID not in registry
    AlreadyRegistered(String),        // Plugin already active
    ManifestIo(std::io::Error),         // IO error reading manifest file
    ManifestParse { path: String, source: serde_json::Error }, // JSON parse error
    LockPoisoned,                     // Internal Mutex poisoned
    ValidationFailed(String),           // Manifest security validation failed
}
```

### 9.2 Common Failure Modes

| Error | Cause | Resolution |
|-------|-------|------------|
| `PluginNotFound` | Plugin ID not yet registered or was unregistered | Register the plugin first with `register_plugin()` |
| `AlreadyRegistered` | Attempted to register an active plugin | Disable the existing plugin first, or use `discover_and_register()` |
| `SessionNotFound` | Session ID doesn't exist or was already removed | Check the session ID, or re-register the session |
| `ManifestIo` | Cannot read plugin directory or manifest file | Check file permissions and directory path |
| `ManifestParse` | Invalid JSON in manifest | Validate JSON with `serde_json` before loading |
| `ValidationFailed` | Manifest failed security validation | Fix the `install` or `mcpConfig` field |
| `LockPoisoned` | A thread panicked while holding the `Mutex` | Restart the application (rare) |

### 9.3 Tauri Command Error Responses

All Tauri plugin commands return errors as strings. Typical error response:

```rust
// From the frontend (Dioxus/WASM)
match athena_bridge::plugin_enable("my-plugin").await {
    Ok(_) => log::info!("Plugin enabled"),
    Err(e) => log::error!("Failed to enable plugin: {:?}", e),
}
```

---

## 10. Configuration

Plugins can store and retrieve configuration as JSON values:

### 10.1 Setting Config

```rust
// Merges new config with existing config (object-level merge)
manager.set_plugin_config(
    "com.example.my-plugin",
    &serde_json::json!({ "timeout": 60, "verbose": true }),
)?;
```

### 10.2 Getting Config

```rust
let config = manager.get_plugin_config("com.example.my-plugin").unwrap();
let timeout: i64 = config["timeout"].as_i64().unwrap_or(30);
```

### 10.3 Config via Tauri Commands

```rust
// Get
let json = athena_bridge::plugin_get_config("my-plugin").await?;

// Set
let new_config = serde_json::json!({ "apiKey": "secret" }).to_string();
athena_bridge::plugin_set_config("my-plugin", &new_config).await?;
```

Callbacks:
- `PluginCallbacks::on_plugin_configured(plugin_id, &merged_config)`
- Registry update event emitted

---

## 11. Callbacks and Event Integration

Implement `PluginCallbacks` to receive lifecycle events. This is how the Tauri layer hooks into the plugin manager:

```rust
use athena_plugins::*;
use std::collections::HashMap;

struct PluginEventLogger;

impl PluginCallbacks for PluginEventLogger {
    fn on_session_registered(&self, session: &PluginSession) {
        log::info!("Session {} registered for plugin {}", session.id, session.plugin_id);
    }

    fn on_session_removed(&self, session_id: &str, agent_id: &str) {
        log::info!("Session {} (agent {}) removed", session_id, agent_id);
    }

    fn on_session_status_update(
        &self,
        session_id: &str,
        agent_id: &str,
        status: SessionStatus,
        _data: Option<&serde_json::Value>,
    ) {
        log::info!("Session {} status -> {:?}", session_id, status);
    }

    fn on_plugin_event(&self, event: &PluginEvent) {
        log::info!("Plugin event [{:?}] at timestamp {}", event.event_type, event.timestamp);
    }

    fn on_registry_updated(&self, registry: &HashMap<String, PluginInfo>) {
        log::info!("Plugin registry updated: {} plugins", registry.len());
    }

    fn on_plugin_registered(&self, plugin_id: &str, name: &str) {
        log::info!("Plugin registered: {} ({})", name, plugin_id);
    }

    fn on_plugin_enabled(&self, plugin_id: &str, name: &str) {
        log::info!("Plugin enabled: {} ({})", name, plugin_id);
    }

    fn on_plugin_disabled(&self, plugin_id: &str) {
        log::info!("Plugin disabled: {}", plugin_id);
    }

    fn on_plugin_error(&self, plugin_id: &str, error: &str) {
        log::error!("Plugin {} error: {}", plugin_id, error);
    }

    fn on_plugin_configured(&self, plugin_id: &str, config: &serde_json::Value) {
        log::info!("Plugin {} config set to: {}", plugin_id, config);
    }
}
```

For unit tests, `NoopCallbacks` provides a zero-implementation default:

```rust
let manager = PluginManager::with_callbacks(Arc::new(NoopCallbacks));
```

---

## 12. API Quick Reference (Tauri Commands → Rust Methods)

| Tauri Command | Rust Method | Description |
|---------------|-------------|-------------|
| `plugin_list` | `list_plugins()` | List all registered plugins |
| `plugin_get` | `get_plugin_info(id)` | Get a specific plugin's info |
| `plugin_register` | `register_plugin(manifest)` | Register a new plugin |
| `plugin_unregister` | `unregister_plugin(id)` | Remove a plugin from registry |
| `plugin_enable` | `enable_plugin(id)` | Enable a plugin |
| `plugin_disable` | `disable_plugin(id)` | Disable a plugin |
| `plugin_get_config` | `get_plugin_config(id)` | Get plugin configuration |
| `plugin_set_config` | `set_plugin_config(id, &value)` | Set plugin configuration |
| `plugin_set_error` | `set_plugin_error(id, msg)` | Mark plugin as errored |
| `plugin_host_list_sessions` | `list_sessions()` | List all sessions |
| `plugin_host_get_session` | `get_session(id)` | Get a session by ID |
| `plugin_host_emit_event` | `emit_plugin_event(type, source, payload)` | Emit a plugin event |
| `plugin_host_subscribe` | `subscribe_session(id, &[types])` | Subscribe session to events |
| `plugin_host_update_status` | `update_session_status(id, status, data)` | Update session status |
| `plugin_host_unregister_session` | `remove_session(id)` | Close a session |
| `plugin_host_discover_plugins` | `discover_plugins(dir)` | Scan directory for manifests |
| `plugin_host_setup_plugin` | `register_plugin(manifest)` | Register + setup a plugin |
| `plugin_host_remove_plugin` | `unregister_plugin(id)` | Remove a plugin |

## 13. Wiring Events to the Frontend

In the Tauri app, wire the `PluginManager` to emit events to the Dioxus frontend:

```rust
// In src-tauri/src/state.rs (simplified)
fn wire_plugin_events(&self) {
    let plugin_manager = self.plugin_manager.clone();
    plugin_manager.set_event_emitter(move |channel: &str, data: &serde_json::Value| {
        match self.app_handle().emit_all(channel, data) {
            Ok(()) => {}
            Err(e) => log::warn!("failed to emit plugin event {}: {}", channel, e),
        }
    });
}
```

These events are available on the frontend via Tauri listeners on the channels:
- `plugin:registered`
- `plugin:enabled`
- `plugin:disabled`
- `plugin:error`
- `plugin:registryUpdated`
- `plugin:event`

---

## References

- **`crates/athena-plugins/src/lib.rs`** — PluginManager implementation and all data types
- **`crates/athena-plugins/Cargo.toml`** — Crate manifest
- **`src-tauri/src/commands/mod.rs`** — Tauri command handlers (lines 3328–3597)
- **`frontend/src/tauri_bridge.rs`** — Frontend bridge functions (lines 909–1007)
- **`docs/plugin-system-spec.md`** — Full architecture specification (2000+ lines)
- **`docs/plugin-system-guide.md`** — High-level guide for plugin management (includes MCP plugin development tutorial)
