# Athena Plugin & MCP System — Architecture Specification

> **Version:** 1.1.0-draft
> **Date:** 2026-04-28
> **Author:** Agent 1 (Project Architect)
> **Status:** Ready for implementation

---

## 1. Overview

Athena's Plugin/MCP System enables AI agents running inside terminal panes (Claude Code, OpenCode, Codex, Gemini CLI, etc.) to communicate **back** with the Athena desktop app. Agents become first-class participants that can notify the user, request input, report status, and (in future phases) be controlled bidirectionally — all through a standards-compliant MCP server interface with an optional injection-based plugin layer.

### 1.1 Goals

- **Notifications first**: Alert the user when an agent finishes, errors, or needs input.
- **Extensibility**: The protocol must support future capabilities (agent control, status monitoring, task management, file sharing) without breaking changes.
- **Dual transport**: Agents connect via MCP (external processes) **or** via an injected plugin (session-level hooking).
- **Zero friction for agents**: MCP tooling is the lingua franca — agents already know how to call MCP tools. No custom SDK required.
- **Secure by default**: Auth tokens, scoped capabilities, process isolation.

### 1.2 Non-Goals

- We are not building a general-purpose plugin marketplace or sandbox runtime.
- We are not exposing the renderer DOM to agents.
- We are not replacing the existing `ipcMain`/`ipcRenderer` bridge.

---

## 2. Architecture

### 2.1 System Diagram

```
┌──────────────────────────────────────────────────────────────────┐
│  Athena Electron App                                             │
│                                                                  │
│  ┌────────────────────┐     ┌─────────────────────────────┐     │
│  │  Renderer Process   │◄────│  Main Process               │     │
│  │  (React/Zustand)    │IPC  │                             │     │
│  │                    │     │  ┌─────────────────────────┐ │     │
│  │  ┌──────────────┐  │     │  │  Plugin Host            │ │     │
│  │  │ Notification  │  │     │  │  (pluginHost.ts)        │ │     │
│  │  │ Store         │  │     │  │                         │ │     │
│  │  └──────────────┘  │     │  │  - Manages plugin regs  │ │     │
│  │  ┌──────────────┐  │     │  │  - Routes events        │ │     │
│  │  │ Agent Status  │  │     │  │  - Enforces scopes      │ │     │
│  │  │ Store         │  │     │  └────────┬────────────────┘ │     │
│  │  └──────────────┘  │     │           │                   │     │
│  │                    │     │  ┌────────▼────────────────┐  │     │
│  │                    │     │  │  MCP Server             │  │     │
│  │                    │     │  │  (mcpServer.ts — UPGRADED)│  │     │
│  │                    │     │  │                         │  │     │
│  │                    │     │  │  TCP :4545 (existing)   │  │     │
│  │                    │     │  │  + STDIO transport      │  │     │
│  │                    │     │  └────────┬────────────────┘  │     │
│  └────────────────────┘     └───────────┼──────────────────┘     │
│                                         │                        │
└─────────────────────────────────────────┼────────────────────────┘
                                          │
                        ┌─────────────────┼──────────────────┐
                        │                 │                  │
                   ┌────▼────┐      ┌─────▼─────┐     ┌─────▼─────┐
                   │ Claude  │      │ OpenCode  │     │  Custom   │
                   │ Code    │      │ / Codex   │     │  Agent    │
                   │ (MCP)   │      │ (MCP)     │     │ (MCP/STDIO│
                   └─────────┘      └───────────┘     └───────────┘

                   ┌──────────────────────────────────────────┐
                   │  Agent Session (inside node-pty)         │
                   │                                          │
                   │  ┌─────────────────────────────────┐    │
                   │  │  Injected Plugin Hook             │    │
                   │  │  (athena-agent-hooks.sh / .js)    │    │
                   │  │                                   │    │
                   │  │  - Wraps agent CLI invocation     │    │
                   │  │  - Injects ATHENA_MCP_TOKEN env   │    │
                   │  │  - Sets up MCP config in agent's  │    │
                   │  │    native format                  │    │
                   │  │  - Post-exit: sends exit event    │    │
                   │  └─────────────────────────────────┘    │
                   └──────────────────────────────────────────┘
```

### 2.2 Component Responsibilities

| Component              | File                                        | Responsibility                                                                                                    |
| ---------------------- | ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------- |
| **Plugin Host**        | `electron/pluginHost.ts`                    | Registers plugins, validates manifests, routes events between MCP server and renderer, enforces capability scopes |
| **MCP Server**         | `electron/mcpServer.ts` (upgraded)          | Handles MCP JSON-RPC over TCP and STDIO, dispatches tool calls, broadcasts notifications                          |
| **MCP Proxy**          | `bin/mcp-proxy.js` (existing)               | Bridges agent stdin/stdout to the TCP MCP server for agents that require stdio MCP                                |
| **Plugin Hook**        | `bin/athena-agent-hooks.sh`                 | Shell snippet sourced into agent sessions; sets env vars and MCP config                                           |
| **Renderer Bridge**    | `electron/preload.ts` (extended)            | Exposes `window.athena.plugin.*` to renderer for plugin events                                                    |
| **Notification Store** | `src/store/notificationStore.ts` (extended) | Receives plugin-originated notifications and surfaces them in UI                                                  |
| **Plugin Types**       | `src/types/plugin.ts` (new)                 | TypeScript interfaces for manifests, events, tool schemas                                                         |

### 2.3 Data Flow: Notification Example

```
1. Agent calls MCP tool: notify({ level: "warning", message: "Build failed — needs input" })
2. MCP server receives JSON-RPC request on TCP :4545
3. MCP server authenticates session token
4. MCP server calls pluginHost.emitEvent("notification", { ... })
5. Plugin Host validates scope, routes to mainWindow.webContents.send("plugin:event", payload)
6. Preload bridge delivers to renderer via window.athena.plugin.onEvent()
7. Notification store.addNotification() updates UI
8. NotificationBell component renders the unread indicator
```

---

## 3. Communication Protocol

### 3.1 Transport Layer

The system supports **two** transport modes, chosen based on the agent's MCP client capabilities:

#### 3.1.1 TCP (existing — primary)

- **Port**: `127.0.0.1:4545` (already in use by `mcpServer.ts`)
- **Protocol**: Newline-delimited JSON-RPC 2.0
- **Auth**: Session token via `initialize` method (already implemented)
- **Use when**: Agent supports TCP MCP connections (Claude Code, Codex)

#### 3.1.2 STDIO (new — secondary)

- **Mechanism**: The MCP proxy (`bin/mcp-proxy.js`) bridges stdin/stdout to the TCP server
- **Protocol**: Same JSON-RPC 2.0, piped through the proxy
- **Auth**: `ATHENA_MCP_TOKEN` environment variable injected at spawn time
- **Use when**: Agent only supports stdio MCP (OpenCode, some custom agents)

#### 3.1.3 Why Not WebSocket or HTTP?

- **WebSocket**: Adds a dependency (`ws`) and a second listening port for no functional gain. TCP already provides bidirectional streaming with lower overhead.
- **HTTP REST**: MCP is inherently request-response with server-initiated notifications, which maps poorly to request/response HTTP. The current TCP newline-delimited protocol is already correct.
- **Future**: If browser-based agents or remote connections become a requirement, a WebSocket gateway can be added as a thin adapter on top of the same Plugin Host without changing the protocol.

### 3.2 Authentication

```typescript
interface McpSession {
  sessionId: string // UUID — identifies this MCP connection
  token: string // UUID — ATHENA_MCP_TOKEN, generated at app start
  paneId: string | null // Links to a specific terminal pane (if known)
  agentType: AgentType // 'claude' | 'codex' | 'opencode' | 'gemini' | 'custom' | 'shell'
  capabilities: string[] // Scoped capabilities granted to this session
  connectedAt: number // Unix ms
  lastActivityAt: number // Unix ms — updated on every request
}
```

- The `SESSION_TOKEN` (already in `mcpServer.ts`) is generated once at app start via `randomUUID()`.
- Agents receive it via the `ATHENA_MCP_TOKEN` environment variable.
- The `initialize` handshake validates the token and returns the session's scoped capabilities.
- Each connected socket gets a `sessionId` for event routing.

### 3.3 Protocol Messages

All messages conform to **JSON-RPC 2.0** over newline-delimited TCP:

#### Request (agent → Athena)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "tools/call",
  "params": {
    "name": "notify",
    "arguments": {
      "level": "info",
      "message": "Task complete",
      "metadata": { "taskId": "abc-123" }
    }
  }
}
```

#### Response (Athena → agent)

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "content": [{ "type": "text", "text": "Notification delivered" }]
  }
}
```

#### Server-Initiated Notification (Athena → agent)

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/event",
  "params": {
    "type": "user_response",
    "payload": { "input": "yes", "requestId": "req-456" }
  }
}
```

---

## 4. MCP Tool Interfaces

### 4.1 Tool Registry

All tools are registered in the `TOOLS` array in `mcpServer.ts`. The new plugin tools are added alongside the existing `create_tasks`, `get_next_task`, `update_task_status`, and `spawn_agents` tools.

### 4.2 Namespace Convention

Tools are namespaced by capability domain:

| Namespace | Purpose                     | Phase              |
| --------- | --------------------------- | ------------------ |
| `notify`  | User notifications          | Phase 1            |
| `status`  | Agent status reporting      | Phase 1            |
| `task`    | Task board interaction      | Phase 1 (existing) |
| `control` | Agent lifecycle control     | Phase 2            |
| `input`   | Request user input          | Phase 2            |
| `file`    | File sharing between agents | Phase 3            |

### 4.3 Phase 1 Tool Definitions

#### `notify` — Send a notification to the user

```json
{
  "name": "notify",
  "description": "Send a notification to the Athena UI. Use this to alert the user when a task completes, fails, or requires attention.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "level": {
        "type": "string",
        "enum": ["info", "warning", "error", "success"],
        "description": "Severity level of the notification."
      },
      "message": {
        "type": "string",
        "description": "Human-readable notification text."
      },
      "title": {
        "type": "string",
        "description": "Optional short title for the notification."
      },
      "metadata": {
        "type": "object",
        "description": "Optional structured data attached to the notification.",
        "additionalProperties": true
      },
      "actions": {
        "type": "array",
        "description": "Optional action buttons the user can tap.",
        "items": {
          "type": "object",
          "properties": {
            "id": { "type": "string", "description": "Action identifier" },
            "label": { "type": "string", "description": "Button label" }
          },
          "required": ["id", "label"]
        }
      }
    },
    "required": ["level", "message"]
  }
}
```

**Behavior**: Creates a `Notification` in `notificationStore`, triggers the system notification sound (if not muted), and sets the unread badge count on `NotificationBell`.

**Response**:

```json
{ "content": [{ "type": "text", "text": "Notification delivered." }] }
```

#### `status_update` — Report agent status

```json
{
  "name": "status_update",
  "description": "Report the current status of this agent. Athena uses this to track agent health and display status in the UI.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "status": {
        "type": "string",
        "enum": [
          "idle",
          "thinking",
          "working",
          "waiting_for_input",
          "completed",
          "error",
          "cancelled"
        ],
        "description": "Current agent status."
      },
      "message": {
        "type": "string",
        "description": "Optional human-readable status detail."
      },
      "progress": {
        "type": "object",
        "description": "Optional progress indicator.",
        "properties": {
          "current": { "type": "number", "description": "Current step" },
          "total": { "type": "number", "description": "Total steps" },
          "label": { "type": "string", "description": "Step description" }
        }
      },
      "artifacts": {
        "type": "array",
        "description": "Optional list of files or outputs produced.",
        "items": {
          "type": "object",
          "properties": {
            "path": { "type": "string", "description": "File path or URI" },
            "type": {
              "type": "string",
              "enum": ["file", "url", "image", "log"],
              "description": "Artifact type"
            }
          }
        }
      }
    },
    "required": ["status"]
  }
}
```

**Behavior**: Updates the agent's status in `swarmStore` (if part of a swarm) and in the pane-level agent tracking. Emits a `plugin:event` to the renderer with type `status_update`.

**Response**:

```json
{ "content": [{ "type": "text", "text": "Status updated to: working" }] }
```

#### `request_input` — Request user input (Phase 2 — defined now, implemented later)

```json
{
  "name": "request_input",
  "description": "Request input from the user. The notification appears in the UI with response options. The tool call blocks until the user responds or a timeout is reached.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "prompt": {
        "type": "string",
        "description": "The question or prompt to display to the user."
      },
      "options": {
        "type": "array",
        "description": "Predefined response options.",
        "items": { "type": "string" }
      },
      "allowFreeText": {
        "type": "boolean",
        "description": "Whether the user can type a custom response.",
        "default": true
      },
      "timeoutMs": {
        "type": "number",
        "description": "Maximum wait time in milliseconds. 0 = no timeout.",
        "default": 0
      }
    },
    "required": ["prompt"]
  }
}
```

**Behavior**: Creates a blocking notification in the UI. When the user responds, the MCP server resolves the pending tool call with the user's input. If the timeout elapses, returns an error result.

**Response** (on user reply):

```json
{ "content": [{ "type": "text", "text": "{\"response\": \"yes\", \"type\": \"option\"}" }] }
```

**Response** (on timeout):

```json
{
  "isError": true,
  "content": [{ "type": "text", "text": "Input request timed out after 60000ms" }]
}
```

### 4.4 Phase 2 Tool Definitions (Stubs)

These are defined now in the schema so the tool registry is aware of them, but `handleToolCall` returns a "not yet available" message:

#### `control_pause` — Pause an agent

```json
{
  "name": "control_pause",
  "description": "Pause the execution of a specific agent pane.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "paneId": { "type": "string", "description": "The pane ID to pause." },
      "reason": { "type": "string", "description": "Optional reason for pausing." }
    },
    "required": ["paneId"]
  }
}
```

#### `control_resume` — Resume a paused agent

```json
{
  "name": "control_resume",
  "description": "Resume a paused agent pane.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "paneId": { "type": "string", "description": "The pane ID to resume." }
    },
    "required": ["paneId"]
  }
}
```

#### `control_cancel` — Cancel/terminate an agent

```json
{
  "name": "control_cancel",
  "description": "Cancel and terminate an agent pane.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "paneId": { "type": "string", "description": "The pane ID to cancel." },
      "force": { "type": "boolean", "description": "Force-kill the process.", "default": false }
    },
    "required": ["paneId"]
  }
}
```

---

## 5. Event System

### 5.1 Event Bus Architecture

The `PluginHost` maintains an internal `EventEmitter` that bridges MCP tool calls to the renderer process. Events flow in one direction (agent → app) for Phase 1, with bidirectional flow added in Phase 2.

```typescript
// electron/pluginHost.ts (conceptual)
import { EventEmitter } from 'events'
import { BrowserWindow } from 'electron'

class PluginHost extends EventEmitter {
  private sessions: Map<string, McpSession> = new Map()
  private mainWindow: BrowserWindow | null = null

  registerSession(session: McpSession): void { ... }
  removeSession(sessionId: string): void { ... }
  emitEvent(type: PluginEventType, payload: PluginEventPayload): void {
    // Validate scope
    // Emit to internal listeners
    this.emit(type, payload)
    // Forward to renderer
    this.mainWindow?.webContents.send('plugin:event', { type, payload, timestamp: Date.now() })
  }
}
```

### 5.2 Event Types

```typescript
// src/types/plugin.ts

export type PluginEventType =
  | 'notification' // Agent sent a user notification
  | 'status_update' // Agent reported its status
  | 'task_complete' // Agent finished a task
  | 'task_error' // Agent encountered an error
  | 'needs_input' // Agent is waiting for user input
  | 'agent_spawned' // A new agent was spawned
  | 'agent_exited' // An agent process exited
  | 'agent_stalled' // Agent hasn't reported activity within threshold
  | 'progress_update' // Agent reported incremental progress
  | 'artifact_produced' // Agent produced a file/output artifact
  | 'user_response' // User responded to an input request (Phase 2)
  | 'control_command' // App sent a control command to agent (Phase 2)

export interface PluginEvent {
  id: string // Event UUID
  type: PluginEventType
  source: {
    sessionId: string // MCP session that originated the event
    paneId: string | null // Terminal pane ID (if linked)
    agentType: AgentType // Agent type identifier
    agentId: string | null // Swarm agent ID (if part of swarm)
  }
  payload: PluginEventPayload
  timestamp: number // Unix ms
}

export interface PluginEventPayload {
  // --- notification ---
  level?: 'info' | 'warning' | 'error' | 'success'
  message?: string
  title?: string
  metadata?: Record<string, unknown>
  actions?: Array<{ id: string; label: string }>

  // --- status_update ---
  status?:
    | 'idle'
    | 'thinking'
    | 'working'
    | 'waiting_for_input'
    | 'completed'
    | 'error'
    | 'cancelled'
  progress?: { current: number; total: number; label: string }
  artifacts?: Array<{ path: string; type: 'file' | 'url' | 'image' | 'log' }>

  // --- task_complete / task_error ---
  taskId?: string
  taskTitle?: string
  result?: string
  error?: string

  // --- needs_input ---
  prompt?: string
  options?: string[]
  requestId?: string

  // --- user_response (Phase 2) ---
  response?: string
  responseType?: 'option' | 'freetext'

  // --- agent_exited ---
  exitCode?: number

  // --- control_command (Phase 2) ---
  command?: 'pause' | 'resume' | 'cancel'
}
```

### 5.3 Event Routing Rules

| Event Type          | Renderer Target                           | MCP Broadcast?       | Persist?             |
| ------------------- | ----------------------------------------- | -------------------- | -------------------- |
| `notification`      | `notificationStore`                       | No                   | Yes (50 max)         |
| `status_update`     | `swarmStore` / pane status                | No                   | No (ephemeral)       |
| `task_complete`     | `notificationStore` + `taskStore`         | Yes (to coordinator) | Yes                  |
| `task_error`        | `notificationStore`                       | Yes (to coordinator) | Yes                  |
| `needs_input`       | `notificationStore` (with action buttons) | No                   | Yes (until resolved) |
| `agent_spawned`     | Pane tracker                              | No                   | No                   |
| `agent_exited`      | Pane tracker + `notificationStore`        | No                   | Yes                  |
| `agent_stalled`     | `notificationStore` (warning)             | No                   | Yes                  |
| `progress_update`   | Pane status indicator                     | No                   | No (ephemeral)       |
| `artifact_produced` | File explorer refresh                     | No                   | No                   |
| `user_response`     | MCP server (resolves pending tool call)   | No                   | No                   |
| `control_command`   | Agent's PTY session (via `ptyManager`)    | No                   | No                   |

---

## 6. Plugin Manifest Schema

### 6.1 Purpose

A plugin manifest describes a plugin's identity, capabilities, and configuration. It allows Athena to:

1. Know what tools a plugin provides (for the MCP `tools/list` response)
2. Enforce capability scopes (a notification-only plugin can't control agents)
3. Route events correctly
4. Display plugin metadata in the Settings UI

### 6.2 Manifest Definition

```typescript
// src/types/plugin.ts

export interface PluginManifest {
  /** Unique plugin identifier — reverse-DNS style */
  id: string // e.g., "com.athena.notifications"

  /** Human-readable name */
  name: string // e.g., "Athena Notifications"

  /** Semver version */
  version: string // e.g., "1.0.0"

  /** Plugin description */
  description: string

  /** Author or organization */
  author: string

  /** Minimum Athena version required */
  minAthenaVersion: string // e.g., "0.1.0"

  /** Capabilities this plugin requires — used for scope enforcement */
  capabilities: PluginCapability[]

  /** MCP tools this plugin exposes */
  tools: PluginToolDefinition[]

  /** Event types this plugin subscribes to (for bidirectional routing) */
  subscribesTo?: PluginEventType[]

  /** Configuration schema — defines user-configurable settings */
  config?: PluginConfigSchema

  /** Installation method */
  install: PluginInstallMethod
}

export type PluginCapability =
  | 'notifications' // Can send notifications to the user
  | 'status' // Can report agent status
  | 'tasks' // Can interact with the task board
  | 'agent_control' // Can control agent lifecycle (pause/resume/cancel)
  | 'user_input' // Can request user input (blocking)
  | 'file_access' // Can read/write files in the workspace
  | 'swarm' // Can interact with the swarm coordinator

export interface PluginToolDefinition {
  name: string // Tool name (namespaced)
  description: string
  inputSchema: Record<string, unknown> // JSON Schema object
  capability: PluginCapability // Required capability
  phase: 1 | 2 | 3 // Implementation phase
}

export interface PluginConfigSchema {
  /** JSON Schema for the config object */
  schema: Record<string, unknown>
  /** Default values */
  defaults: Record<string, unknown>
}

export type PluginInstallMethod =
  | { type: 'builtin' } // Ships with Athena
  | { type: 'mcp_server'; command: string; args?: string[]; env?: Record<string, string> }
  | { type: 'hook'; script: string } // Shell hook injected into PTY sessions
```

### 6.3 Builtin Plugin Manifests

The core plugin ships with Athena and doesn't require installation:

```json
{
  "id": "com.athena.core",
  "name": "Athena Core Plugin",
  "version": "1.0.0",
  "description": "Core MCP tools for agent-to-app communication: notifications, status, tasks.",
  "author": "Athena",
  "minAthenaVersion": "0.1.0",
  "capabilities": ["notifications", "status", "tasks", "agent_control", "user_input"],
  "tools": [
    {
      "name": "notify",
      "description": "Send a notification to the Athena UI.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "level": { "type": "string", "enum": ["info", "warning", "error", "success"] },
          "message": { "type": "string" },
          "title": { "type": "string" },
          "metadata": { "type": "object", "additionalProperties": true },
          "actions": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": { "id": { "type": "string" }, "label": { "type": "string" } },
              "required": ["id", "label"]
            }
          }
        },
        "required": ["level", "message"]
      },
      "capability": "notifications",
      "phase": 1
    },
    {
      "name": "status_update",
      "description": "Report agent status.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "status": {
            "type": "string",
            "enum": [
              "idle",
              "thinking",
              "working",
              "waiting_for_input",
              "completed",
              "error",
              "cancelled"
            ]
          },
          "message": { "type": "string" },
          "progress": {
            "type": "object",
            "properties": {
              "current": { "type": "number" },
              "total": { "type": "number" },
              "label": { "type": "string" }
            }
          },
          "artifacts": {
            "type": "array",
            "items": {
              "type": "object",
              "properties": {
                "path": { "type": "string" },
                "type": { "type": "string", "enum": ["file", "url", "image", "log"] }
              }
            }
          }
        },
        "required": ["status"]
      },
      "capability": "status",
      "phase": 1
    },
    {
      "name": "request_input",
      "description": "Request user input (blocks until response).",
      "inputSchema": {
        "type": "object",
        "properties": {
          "prompt": { "type": "string" },
          "options": { "type": "array", "items": { "type": "string" } },
          "allowFreeText": { "type": "boolean", "default": true },
          "timeoutMs": { "type": "number", "default": 0 }
        },
        "required": ["prompt"]
      },
      "capability": "user_input",
      "phase": 2
    },
    {
      "name": "control_pause",
      "description": "Pause an agent pane.",
      "inputSchema": {
        "type": "object",
        "properties": { "paneId": { "type": "string" }, "reason": { "type": "string" } },
        "required": ["paneId"]
      },
      "capability": "agent_control",
      "phase": 2
    },
    {
      "name": "control_resume",
      "description": "Resume a paused agent.",
      "inputSchema": {
        "type": "object",
        "properties": { "paneId": { "type": "string" } },
        "required": ["paneId"]
      },
      "capability": "agent_control",
      "phase": 2
    },
    {
      "name": "control_cancel",
      "description": "Cancel an agent.",
      "inputSchema": {
        "type": "object",
        "properties": {
          "paneId": { "type": "string" },
          "force": { "type": "boolean", "default": false }
        },
        "required": ["paneId"]
      },
      "capability": "agent_control",
      "phase": 2
    }
  ],
  "subscribesTo": ["user_response", "control_command"],
  "install": { "type": "builtin" }
}
```

### 6.4 Custom Plugin Installation

Third-party plugins are installed by placing a `athena-plugin.json` manifest in the workspace's `.ade/plugins/` directory or in the user-level `~/.athena/plugins/` directory.

**Discovery order:**

1. Builtin plugins (always loaded)
2. User-level `~/.athena/plugins/*/athena-plugin.json`
3. Workspace-level `.ade/plugins/*/athena-plugin.json`

**Loading**: At app start, `pluginHost.ts` scans all directories, validates manifests against the schema, and registers tools. Invalid manifests are skipped with a console warning.

### 6.5 Plugin Registration Flow

```
1. App starts → pluginHost.scanPlugins()
2. For each manifest found:
   a. Validate JSON schema
   b. Check minAthenaVersion compatibility
   c. Merge tools into the MCP TOOLS array
   d. Register event subscriptions
   e. If install.type === 'mcp_server', start the subprocess
3. Plugin Host is ready — MCP server can now advertise all registered tools
```

---

## 7. Plugin Hook System (Injection-Based)

### 7.1 Problem

Not all agents connect via MCP. Some are plain shell sessions. The hook system allows any PTY session to participate in the plugin ecosystem by injecting environment variables and startup scripts.

### 7.2 Implementation

When `ptyManager.spawn()` is called with an `agentCmd`, the hook system:

1. Sets `ATHENA_MCP_TOKEN` in the PTY environment
2. Sets `ATHENA_MCP_PORT=4545`
3. Sets `ATHENA_SESSION_ID=<paneId>`
4. Constructs the agent's MCP configuration in its native format
5. Prepends the MCP env setup to the agent command

This is already partially implemented in `mcpServer.ts:handleToolCall` (the `spawn_agents` case), but it's hardcoded for Claude. The plugin system generalizes it.

### 7.3 Agent-Specific MCP Configuration

Each agent type has a different MCP configuration format:

```typescript
// electron/agentMcpConfig.ts (new)

interface McpConfigResult {
  env: string // Shell export statements to prepend
  command: string // Modified agent command (if needed)
}

export function buildMcpConfig(
  agentType: AgentType,
  token: string,
  proxyPath: string,
): McpConfigResult {
  const baseEnv = `export ATHENA_MCP_TOKEN='${token}' ATHENA_MCP_PORT=4545 ATHENA_SESSION_ID='$ATHENA_PANE_ID'`

  switch (agentType) {
    case 'claude':
      // Claude uses CLAUDE_MCP_SERVERS env var
      const claudeConfig = JSON.stringify({
        athena: { command: 'node', args: [proxyPath], env: { ATHENA_MCP_TOKEN: token } },
      })
      return {
        env: `${baseEnv}; export CLAUDE_MCP_SERVERS='${claudeConfig.replace(/'/g, "'\\''")}';`,
        command: '', // appended before the agent command
      }

    case 'opencode':
      // OpenCode uses OPENCODE_MCP_SERVERS or opencode.json
      const openCodeConfig = JSON.stringify({
        athena: { command: 'node', args: [proxyPath], env: { ATHENA_MCP_TOKEN: token } },
      })
      return {
        env: `${baseEnv}; export OPENCODE_MCP_SERVERS='${openCodeConfig.replace(/'/g, "'\\''")}';`,
        command: '',
      }

    case 'gemini':
      // Gemini CLI uses GEMINI_MCP_CONFIG or similar
      const geminiConfig = JSON.stringify({
        athena: { command: 'node', args: [proxyPath], env: { ATHENA_MCP_TOKEN: token } },
      })
      return {
        env: `${baseEnv}; export GEMINI_MCP_SERVERS='${geminiConfig.replace(/'/g, "'\\''")}';`,
        command: '',
      }

    case 'codex':
      // Codex uses similar pattern
      const codexConfig = JSON.stringify({
        athena: { command: 'node', args: [proxyPath], env: { ATHENA_MCP_TOKEN: token } },
      })
      return {
        env: `${baseEnv}; export CODEX_MCP_SERVERS='${codexConfig.replace(/'/g, "'\\''")}';`,
        command: '',
      }

    case 'custom':
    case 'shell':
    default:
      return { env: baseEnv, command: '' }
  }
}
```

### 7.4 Post-Exit Hook

When an agent PTY exits (`pty:exit`), the hook system automatically emits an `agent_exited` event to the plugin host, carrying the exit code and pane ID. This already happens via `app.emit('agent:exited', ...)` in `ptyManager.ts:107` — the plugin host simply subscribes to this Electron app event.

---

## 8. Renderer Integration

### 8.1 Preload Bridge Extensions

Add a `plugin` namespace to `window.athena`:

```typescript
// In preload.ts, add to the athena object:
plugin: {
  onEvent: (cb: (event: PluginEvent) => void) => {
    const handler = (_event: any, data: PluginEvent) => cb(data)
    ipcRenderer.on('plugin:event', handler)
    return () => ipcRenderer.removeListener('plugin:event', handler)
  },
  respondToInput: (requestId: string, response: string) => {
    ipcRenderer.send('plugin:respondToInput', requestId, response)
  },
  listPlugins: () => ipcRenderer.invoke('plugin:list'),
  getPluginConfig: (pluginId: string) => ipcRenderer.invoke('plugin:getConfig', pluginId),
  setPluginConfig: (pluginId: string, config: Record<string, unknown>) =>
    ipcRenderer.invoke('plugin:setConfig', pluginId, config),
}
```

### 8.2 Notification Store Extension

The existing `notificationStore` gains an additional method for plugin-originated notifications:

```typescript
// Extended Notification interface
export interface Notification {
  id: string
  paneId: string
  paneName: string
  agentType: AgentType
  message: string
  timestamp: number
  read: boolean
  spaceId: string
  // --- New fields ---
  source?: 'plugin' | 'system' // Distinguish plugin vs system notifications
  level?: 'info' | 'warning' | 'error' | 'success'
  title?: string
  metadata?: Record<string, unknown>
  actions?: Array<{ id: string; label: string }>
  requestId?: string // Links to a request_input call (if applicable)
}
```

### 8.3 Agent Status Store (New)

A lightweight store for tracking agent status across panes:

```typescript
// src/store/agentStatusStore.ts (new)
import { create } from 'zustand'

export interface AgentStatus {
  paneId: string
  status:
    | 'idle'
    | 'thinking'
    | 'working'
    | 'waiting_for_input'
    | 'completed'
    | 'error'
    | 'cancelled'
  message?: string
  progress?: { current: number; total: number; label: string }
  lastUpdatedAt: number
}

interface AgentStatusState {
  statuses: Record<string, AgentStatus>
  updateStatus: (paneId: string, update: Partial<AgentStatus>) => void
  removeStatus: (paneId: string) => void
}

export const useAgentStatusStore = create<AgentStatusState>((set) => ({
  statuses: {},
  updateStatus: (paneId, update) =>
    set((s) => ({
      statuses: {
        ...s.statuses,
        [paneId]: { ...s.statuses[paneId], paneId, lastUpdatedAt: Date.now(), ...update },
      },
    })),
  removeStatus: (paneId) =>
    set((s) => {
      const { [paneId]: _, ...rest } = s.statuses
      return { statuses: rest }
    }),
}))
```

### 8.4 UI Components Affected

| Component              | Change                                                                            |
| ---------------------- | --------------------------------------------------------------------------------- |
| `NotificationBell.tsx` | Render `level`-aware styling (error=red, warning=amber, success=green, info=blue) |
| `TerminalPane.tsx`     | Show `AgentStatus` badge (thinking spinner, error indicator)                      |
| `SwarmBoard.tsx`       | Consume `agentStatusStore` for real-time status instead of polling                |
| `SettingsModal.tsx`    | Add "Plugins" section showing installed plugins, config, enable/disable           |
| `AthenaPanel.tsx`      | Show plugin-originated notifications in chat stream                               |

---

## 9. Security Model

### 9.1 Threat Model

| Threat                        | Mitigation                                                                           |
| ----------------------------- | ------------------------------------------------------------------------------------ |
| Unauthorized MCP connection   | Session token required at `initialize`; TCP bound to `127.0.0.1` only                |
| Tool privilege escalation     | Capabilities are scoped per session; `agent_control` requires explicit grant         |
| Malicious plugin manifest     | Manifests are validated against JSON schema; unknown capabilities are rejected       |
| PTY injection via MCP         | Only the MCP server can write to PTY sessions; agents can't target arbitrary paneIds |
| Token leakage via environment | Token is in process environment (not world-readable); per-app-instance rotation      |

### 9.2 Capability Scoping

When an MCP session initializes, it declares its `agentType`. The plugin host maps this to a default capability set:

```typescript
const DEFAULT_CAPABILITIES: Record<AgentType, PluginCapability[]> = {
  claude: ['notifications', 'status', 'tasks', 'user_input'],
  codex: ['notifications', 'status', 'tasks', 'user_input'],
  opencode: ['notifications', 'status', 'tasks', 'user_input'],
  gemini: ['notifications', 'status', 'tasks', 'user_input'],
  custom: ['notifications', 'status'], // Restricted by default
  shell: ['notifications', 'status'], // Restricted by default
}
```

If a tool call arrives for a capability not in the session's scope, the MCP server returns:

```json
{
  "isError": true,
  "content": [{ "type": "text", "text": "Capability 'agent_control' not granted to this session." }]
}
```

### 9.3 Rate Limiting

To prevent notification spam, the plugin host enforces per-session rate limits:

- **Notifications**: Max 10 per minute per session, then drop silently
- **Status updates**: Max 30 per minute per session (agents working fast)
- **Input requests**: Max 1 concurrent per session (must resolve before next)

---

## 10. Implementation Phases

### Phase 1: Notifications & Status (Target: Immediate)

**Files to create:**

- `electron/pluginHost.ts` — Plugin host with event routing
- `electron/agentMcpConfig.ts` — Agent-specific MCP configuration builder
- `src/types/plugin.ts` — Plugin TypeScript interfaces
- `src/store/agentStatusStore.ts` — Agent status zustand store

**Files to modify:**

- `electron/mcpServer.ts` — Add `notify` and `status_update` tools to TOOLS array; add handler logic; integrate with pluginHost
- `electron/preload.ts` — Add `window.athena.plugin.*` namespace
- `electron/main.ts` — Initialize pluginHost alongside mcpServer
- `src/types/global.d.ts` — Add `plugin` namespace to `Window.athena`
- `src/store/notificationStore.ts` — Add `source`, `level`, `title`, `metadata`, `actions`, `requestId` fields
- `src/components/Notifications/NotificationBell.tsx` — Level-aware styling
- `src/components/Terminal/TerminalPane.tsx` — Agent status badge
- `bin/mcp-proxy.js` — Pass `ATHENA_SESSION_ID` through environment

**Estimated effort:** ~400 lines new code, ~150 lines modified

### Phase 2: User Input & Agent Control (Target: +2 weeks)

**Files to create:**

- `electron/pendingRequests.ts` — Manages blocking `request_input` tool calls
- `src/components/Plugin/InputRequestModal.tsx` — UI for responding to agent input requests

**Files to modify:**

- `electron/mcpServer.ts` — Add `request_input`, `control_pause`, `control_resume`, `control_cancel` tools
- `electron/pluginHost.ts` — Handle bidirectional event routing
- `electron/preload.ts` — Add `respondToInput` method
- `electron/ptyManager.ts` — Add pause/resume support (SIGSTOP/SIGCONT on Unix)

### Phase 3: External Plugins & Marketplace (Target: +4 weeks)

**Files to create:**

- `src/components/Settings/PluginManager.tsx` — Plugin browser/manager UI
- `electron/pluginInstaller.ts` — Plugin download and installation logic
- `electron/pluginSandbox.ts` — Sandboxed subprocess management for external MCP servers

**Files to modify:**

- `electron/pluginHost.ts` — Dynamic plugin loading, hot-reload
- `src/components/Settings/SettingsModal.tsx` — Add "Plugins" tab

---

## 11. File Structure (After Phase 1)

```
athenas-core/
├── electron/
│   ├── main.ts                    # + initPluginHost(mainWindow)
│   ├── preload.ts                 # + window.athena.plugin.* namespace
│   ├── mcpServer.ts               # + notify, status_update tools; + pluginHost integration
│   ├── pluginHost.ts              # NEW — event routing, session management, capability scoping
│   ├── agentMcpConfig.ts          # NEW — agent-specific MCP config builder
│   ├── ptyManager.ts              # (no changes in Phase 1 — already emits agent:exited)
│   ├── athenaOrchestrator.ts      # (no changes)
│   ├── toolExecutor.ts            # (no changes)
│   ├── swarmCoordinator.ts        # (no changes)
│   └── ...
├── bin/
│   └── mcp-proxy.js               # + ATHENA_SESSION_ID passthrough
├── src/
│   ├── types/
│   │   ├── plugin.ts              # NEW — PluginManifest, PluginEvent, PluginCapability, etc.
│   │   └── global.d.ts            # + window.athena.plugin type declarations
│   ├── store/
│   │   ├── notificationStore.ts   # Extended — source, level, title, metadata, actions, requestId
│   │   ├── agentStatusStore.ts    # NEW — pane-level agent status tracking
│   │   └── ...
│   ├── components/
│   │   ├── Notifications/
│   │   │   └── NotificationBell.tsx  # Level-aware styling
│   │   ├── Terminal/
│   │   │   └── TerminalPane.tsx      # Agent status badge
│   │   └── ...
│   └── ...
└── docs/
    └── plugin-system-spec.md      # This file
```

---

## 12. API Reference Summary

### 12.1 MCP Tools (Phase 1 — Available)

| Tool                 | Required Capability | Description                               |
| -------------------- | ------------------- | ----------------------------------------- |
| `notify`             | `notifications`     | Send a notification to the user           |
| `status_update`      | `status`            | Report agent status and optional progress |
| `create_tasks`       | `tasks`             | Add tasks to the Kanban board (existing)  |
| `get_next_task`      | `tasks`             | Pull next available task (existing)       |
| `update_task_status` | `tasks`             | Update a task's status (existing)         |
| `spawn_agents`       | `swarm`             | Spawn new agent workers (existing)        |

### 12.2 MCP Tools (Phase 2 — Defined, Returns "not yet available")

| Tool             | Required Capability | Description                   |
| ---------------- | ------------------- | ----------------------------- |
| `request_input`  | `user_input`        | Request user input (blocking) |
| `control_pause`  | `agent_control`     | Pause an agent pane           |
| `control_resume` | `agent_control`     | Resume a paused agent         |
| `control_cancel` | `agent_control`     | Cancel an agent               |

### 12.3 Renderer APIs (`window.athena.plugin`)

| Method                                | Description                                                  |
| ------------------------------------- | ------------------------------------------------------------ |
| `onEvent(cb)`                         | Subscribe to all plugin events; returns unsubscribe function |
| `respondToInput(requestId, response)` | Send user input back to a pending `request_input` call       |
| `listPlugins()`                       | Returns array of registered PluginManifest objects           |
| `getPluginConfig(pluginId)`           | Returns current config for a plugin                          |
| `setPluginConfig(pluginId, config)`   | Updates config for a plugin                                  |

### 12.4 IPC Channels (Main ↔ Renderer)

| Channel                 | Direction                | Payload                    |
| ----------------------- | ------------------------ | -------------------------- |
| `plugin:event`          | Main → Renderer          | `PluginEvent`              |
| `plugin:respondToInput` | Renderer → Main          | `{ requestId, response }`  |
| `plugin:list`           | Renderer → Main (invoke) | Returns `PluginManifest[]` |
| `plugin:getConfig`      | Renderer → Main (invoke) | Returns plugin config      |
| `plugin:setConfig`      | Renderer → Main (invoke) | Acknowledges config update |

---

## 13. Testing Strategy

### 13.1 Unit Tests

- `pluginHost.ts`: Event routing, capability scoping, rate limiting
- `agentMcpConfig.ts`: Config generation for each agent type
- `agentStatusStore.ts`: Zustand store operations

### 13.2 Integration Tests

- MCP server: Send `initialize` → `tools/list` → `tools/call` (notify) → verify renderer receives event
- MCP server: Auth rejection for invalid tokens
- MCP server: Rate limit enforcement
- End-to-end: Spawn an agent with MCP config → agent calls `notify` → notification appears in UI

### 13.3 Manual Test Script

```bash
# 1. Start Athena app
npm run dev

# 2. Get the session token from console logs or env
# 3. Connect via TCP and send test messages:
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"token":"<TOKEN>"}}' | nc 127.0.0.1 4545
echo '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | nc 127.0.0.1 4545
echo '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"notify","arguments":{"level":"info","message":"Hello from MCP!"}}}' | nc 127.0.0.1 4545
```

---

## 14. Migration Notes

### 14.1 Backward Compatibility

- The existing MCP tools (`create_tasks`, `get_next_task`, `update_task_status`, `spawn_agents`) continue to work unchanged.
- The `broadcastNotification` function in `mcpServer.ts` is superseded by the plugin event system but remains functional during the transition.
- The `agent:stalled` and `agent:exited` app events continue to work; the plugin host subscribes to them rather than replacing them.

### 14.2 Breaking Changes (Phase 2 only)

- `request_input` introduces blocking tool calls, which requires the MCP server to hold a JSON-RPC response until the user acts. The existing `handleRequest` synchronous flow must be refactored to support async deferral.
- The `notificationStore` `Notification` type adds new optional fields — this is additive and non-breaking.

---

## 15. Open Questions

| #   | Question                                                       | Decision                                                                                                                                        |
| --- | -------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Should external MCP server plugins run in separate processes?  | **Yes** — Phase 3 will use child_process.fork() for isolation                                                                                   |
| 2   | Should the plugin host persist event history to disk?          | **No** — Events are ephemeral; notifications are capped at 50 in-memory                                                                         |
| 3   | Should agents be able to send notifications to _other_ agents? | **Not directly** — Agents communicate via the swarm mailbox system; the plugin system is agent→app only                                         |
| 4   | Should the MCP token rotate on each agent spawn?               | **Deferred** — Current per-app-instance token is sufficient for local-only TCP; rotation adds complexity for minimal security gain on localhost |
| 5   | Should plugin events be replayed on renderer reconnect?        | **No** — Renderer reconnects are rare (only on hard reload); missed events are acceptable                                                       |
| 6   | Can agents read each other's terminal output?                  | **Yes, with capability grant** — `output_read` capability allows cross-agent reads; self-read always allowed                                    |

---

ARCHITECTURE SPEC COMPLETE

## 16. Agent Output Reading — Architecture Addition

> **Section Version:** 1.1.0-draft
> **Date:** 2026-04-28
> **Status:** Ready for implementation

### 16.1 Problem Statement

The Athena Orchestrator can **send** commands to agents (spawn, write to PTY, task assignment) but cannot **read** their output. The orchestrator's system prompt includes `activePanes` metadata (pane IDs, workspace dirs) but has zero visibility into what agents are actually doing — their stdout, stderr, progress messages, errors, or completion results.

This creates a fundamental observability gap: the orchestrator is blind to agent state between `status_update` MCP calls. If an agent crashes, loops, or produces unexpected output, the orchestrator cannot detect or react to it.

### 16.2 Design Goals

- **Read agent output from the MCP layer**: The orchestrator (or any authorized agent) can query an agent pane's terminal output via MCP tool calls.
- **Incremental reading**: Support reading new output since a bookmark (timestamp, line number, or message ID) to avoid re-transmitting the entire buffer on every poll.
- **Streaming**: Allow subscribing to real-time output chunks via MCP server-initiated notifications, avoiding polling entirely.
- **Security**: Agents cannot read each other's output by default. Only the orchestrator role and explicitly authorized sessions can cross-read.
- **Performance**: Output buffers are bounded, indexed, and cheap to query. No unbounded memory growth.

### 16.3 Output Buffer Architecture

#### 16.3.1 Capture Mechanism

Agent output is already captured at the source — `ptyManager.ts` intercepts all PTY output via `ptyProcess.onData()`. The current implementation stores raw chunks in `historyChunks` (an array of strings, capped at `MAX_HISTORY_BYTES = 100_000`).

The output reading system adds a **structured ring buffer** alongside the existing raw chunk storage:

```
ptyProcess.onData(data) ──► raw historyChunks (existing, unchanged)
                         │
                         └──► outputBuffer.append(data)  (NEW — structured ring buffer)
```

**Why not read from the xterm.js buffer?** xterm.js lives in the renderer process. The MCP server runs in the main process. Reading the xterm buffer would require a synchronous IPC round-trip for every query, adding latency and coupling the MCP layer to the renderer lifecycle (crashes, reloads, etc.). The main-process ring buffer is always available.

**Why not IPC from the terminal process?** `node-pty` already pipes all output through `onData` in the main process. There is no separate terminal process to IPC with — the PTY is a native subprocess whose I/O is already captured.

#### 16.3.2 Ring Buffer Design

Each active agent pane gets its own `AgentOutputBuffer` instance:

```typescript
// electron/outputBuffer.ts (new)

export interface OutputEntry {
  id: string // Unique message ID: "out-{paneId}-{lineNumber}"
  paneId: string // The PTY/pane this output belongs to
  timestamp: number // Unix ms — when this chunk was received
  lineNumber: number // Monotonically increasing line counter (never resets)
  content: string // The text content (ANSI-stripped)
  raw: string // Original content with ANSI escape codes (for faithful replay)
  isStderr: boolean // false for all node-pty output (PTY doesn't distinguish)
}

export class AgentOutputBuffer {
  private entries: OutputEntry[] = []
  private head = 0 // Write pointer for ring buffer
  private totalLines = 0 // Monotonic line counter (never resets)
  private byteSize = 0 // Current total byte size of stored content

  constructor(
    private readonly paneId: string,
    private readonly maxEntries: number = 2000, // Max entries in ring
    private readonly maxBytes: number = 512_000, // ~512KB per agent
  ) {}

  append(rawChunk: string): void {
    const content = rawChunk
      .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')
      .replace(/\x1b\].*?(?:\x07|\x1b\\)/g, '')
      .replace(/\x1b[()][0-9A-B]/g, '')
      .replace(/\r/g, '')

    const lines = content.split('\n').filter((l) => l.trim().length > 0)
    if (lines.length === 0) return

    for (const line of lines) {
      this.totalLines++

      const entry: OutputEntry = {
        id: `out-${this.paneId}-${this.totalLines}`,
        paneId: this.paneId,
        timestamp: Date.now(),
        lineNumber: this.totalLines,
        content: line,
        raw: rawChunk,
        isStderr: false,
      }

      if (this.entries.length < this.maxEntries) {
        this.entries.push(entry)
      } else {
        this.byteSize -= this.entries[this.head].content.length
        this.entries[this.head] = entry
        this.head = (this.head + 1) % this.maxEntries
      }

      this.byteSize += line.length

      while (this.byteSize > this.maxBytes && this.entries.length > 1) {
        const evicted = this.entries.shift()!
        this.byteSize -= evicted.content.length
        this.head = (this.head - 1 + this.maxEntries) % this.maxEntries
      }
    }
  }

  readAll(): OutputEntry[] {
    if (this.entries.length < this.maxEntries) {
      return [...this.entries]
    }
    return [...this.entries.slice(this.head), ...this.entries.slice(0, this.head)]
  }

  readSinceLine(sinceLineNumber: number): OutputEntry[] {
    return this.readAll().filter((e) => e.lineNumber > sinceLineNumber)
  }

  readSinceTimestamp(sinceTimestamp: number): OutputEntry[] {
    return this.readAll().filter((e) => e.timestamp > sinceTimestamp)
  }

  readSinceId(sinceId: string): OutputEntry[] {
    const all = this.readAll()
    const idx = all.findIndex((e) => e.id === sinceId)
    if (idx === -1) return all
    return all.slice(idx + 1)
  }

  readTail(n: number): OutputEntry[] {
    const all = this.readAll()
    return all.slice(-n)
  }

  getMeta() {
    const all = this.readAll()
    return {
      paneId: this.paneId,
      entryCount: all.length,
      byteSize: this.byteSize,
      totalLines: this.totalLines,
      firstLineNumber: all[0]?.lineNumber ?? 0,
      lastLineNumber: all[all.length - 1]?.lineNumber ?? 0,
      lastTimestamp: all[all.length - 1]?.timestamp ?? 0,
    }
  }

  clear(): void {
    this.entries = []
    this.head = 0
    this.byteSize = 0
  }
}
```

#### 16.3.3 Buffer Manager

A singleton `OutputBufferManager` owns all per-pane buffers and hooks into `ptyManager`:

```typescript
// electron/outputBufferManager.ts (new)

import { AgentOutputBuffer, OutputEntry } from './outputBuffer'
import { BrowserWindow, app } from 'electron'

const buffers = new Map<string, AgentOutputBuffer>()

interface OutputSubscription {
  id: string // UUID
  paneId: string // Which pane to watch
  sessionId: string // MCP session that subscribed
  sinceLineNumber: number // Last sent line (for incremental push)
  createdAt: number
}
const subscriptions = new Map<string, OutputSubscription>()

const OUTPUT_PUSH_INTERVAL = 500 // ms

export function initOutputBuffers(mainWindow: BrowserWindow): void {
  app.on('pty:output' as any, (event: { id: string; data: string }) => {
    getOrCreateBuffer(event.id).append(event.data)
  })

  app.on('agent:exited', (event: { id: string; exitCode: number }) => {
    const buf = buffers.get(event.id)
    if (buf) {
      buf.append(`\n[Process exited with code ${event.exitCode}]\n`)
    }
  })

  setInterval(() => {
    for (const sub of subscriptions.values()) {
      const buf = buffers.get(sub.paneId)
      if (!buf) continue
      const newEntries = buf.readSinceLine(sub.sinceLineNumber)
      if (newEntries.length === 0) continue
      sub.sinceLineNumber = newEntries[newEntries.length - 1].lineNumber
      const { broadcastNotification } = require('./mcpServer')
      broadcastNotification('notifications/output', {
        subscriptionId: sub.id,
        paneId: sub.paneId,
        entries: newEntries.map((e) => ({
          id: e.id,
          lineNumber: e.lineNumber,
          timestamp: e.timestamp,
          content: e.content,
        })),
      })
    }
  }, OUTPUT_PUSH_INTERVAL)
}

function getOrCreateBuffer(paneId: string): AgentOutputBuffer {
  let buf = buffers.get(paneId)
  if (!buf) {
    buf = new AgentOutputBuffer(paneId)
    buffers.set(paneId, buf)
  }
  return buf
}

export function getBuffer(paneId: string): AgentOutputBuffer | undefined {
  return buffers.get(paneId)
}
export function deleteBuffer(paneId: string): void {
  buffers.get(paneId)?.clear()
  buffers.delete(paneId)
}
export function listBuffers() {
  return Array.from(buffers.entries()).map(([paneId, buf]) => ({ paneId, meta: buf.getMeta() }))
}
export function addSubscription(sub: OutputSubscription): void {
  subscriptions.set(sub.id, sub)
}
export function removeSubscription(id: string): void {
  subscriptions.delete(id)
}
export function getSubscriptionsForSession(sid: string) {
  return Array.from(subscriptions.values()).filter((s) => s.sessionId === sid)
}
export function getSubscriptionCount(paneId: string) {
  return Array.from(subscriptions.values()).filter((s) => s.paneId === paneId).length
}
```

#### 16.3.4 Integration with ptyManager

One-line addition in the `onData` handler — emit an app event that the `OutputBufferManager` listens to:

```typescript
// In ptyManager.ts, inside the onData handler, AFTER existing code:

ptyProcess.onData((data) => {
  // ... existing code (historyChunks, ready patterns, renderer send) ...

  // NEW: Emit for output buffer capture
  app.emit('pty:output' as any, { id, data })
})
```

This follows the existing pattern — `ptyManager` already emits `agent:exited` via `app.emit()` at line 108.

#### 16.3.5 Buffer Lifecycle

| Event                              | Buffer Action                                                          |
| ---------------------------------- | ---------------------------------------------------------------------- |
| Agent spawned (`pty:spawn`)        | `getOrCreateBuffer(paneId)` — creates if new, clears if reusing        |
| Agent output (`pty:output`)        | `buffer.append(data)` — adds entries                                   |
| Agent exits (`agent:exited`)       | Append exit marker; buffer retained for post-mortem reads              |
| Pane reused (new spawn on same ID) | `deleteBuffer(paneId)` then `getOrCreateBuffer(paneId)` — fresh buffer |
| App shutdown                       | All buffers garbage collected (in-memory only)                         |

### 16.4 Data Types

```typescript
// src/types/output.ts (new)

/** A single output entry from an agent's terminal */
export interface AgentOutput {
  id: string // Unique entry ID: "out-{paneId}-{lineNumber}"
  paneId: string // The terminal pane this output came from
  timestamp: number // Unix ms — when the chunk was received by the main process
  content: string // ANSI-stripped text content of this output line
  lineNumber: number // Monotonic line number (never resets, even across buffer eviction)
  isStderr: boolean // Whether this is stderr output (currently always false for node-pty)
}

/** Metadata about a connected/running agent */
export interface AgentInfo {
  paneId: string // Terminal pane identifier
  agentType: AgentType // 'claude' | 'codex' | 'opencode' | 'gemini' | 'custom' | 'shell'
  status: AgentStatus // Current agent status
  connectedAt: number // Unix ms — when this agent's PTY was spawned
  lastOutputAt: number // Unix ms — timestamp of the most recent output entry
  lastLineNumber: number // Line number of the most recent output entry
  bufferEntryCount: number // Number of entries currently in the output buffer
  bufferByteSize: number // Current byte size of the output buffer
  hasExited: boolean // Whether the agent process has exited
  exitCode: number | null // Exit code if hasExited is true
}

/** Agent status values (shared with existing PluginSession status) */
export type AgentStatus = 'active' | 'idle' | 'waiting_input' | 'disconnected'

/** A subscription to an agent's real-time output stream */
export interface OutputSubscription {
  id: string // UUID — subscription identifier
  paneId: string // Which agent pane to stream
  sessionId: string // MCP session that owns this subscription
  sinceLineNumber: number // Incremental read cursor (last pushed line)
  createdAt: number // Unix ms — when the subscription was created
}

/** Response from athena_read_output */
export interface ReadOutputResponse {
  paneId: string
  entries: AgentOutput[]
  meta: {
    totalLines: number
    bufferedLines: number
    firstLineNumber: number
    lastLineNumber: number
    truncated: boolean
  }
}

/** Response from athena_list_agents */
export interface ListAgentsResponse {
  agents: AgentInfo[]
  totalCount: number
}
```

### 16.5 MCP Tool Definitions

#### 16.5.1 `athena_read_output` — Read an agent's current output buffer

```json
{
  "name": "athena_read_output",
  "description": "Read the current output buffer of a specific agent pane. Returns the terminal output content (ANSI-stripped). Use this to inspect what an agent is doing, check for errors, or read completion messages.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "paneId": {
        "type": "string",
        "description": "The pane ID of the agent whose output to read."
      },
      "tail": {
        "type": "number",
        "description": "Return only the last N lines. If omitted, returns the full buffer. Use for large outputs.",
        "default": null
      },
      "includeRaw": {
        "type": "boolean",
        "description": "If true, include the raw ANSI output in addition to stripped content. Larger response.",
        "default": false
      }
    },
    "required": ["paneId"]
  }
}
```

**Behavior**: Reads from the `AgentOutputBuffer` for the given `paneId`. If `tail` is specified, returns only the last N entries. If `includeRaw` is true, each entry includes the `raw` field with ANSI sequences.

**Response**:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\n  \"paneId\": \"worker-1714300000-0\",\n  \"entries\": [\n    { \"id\": \"out-worker-1714300000-0-42\", \"lineNumber\": 42, \"timestamp\": 1714300012500, \"content\": \"Running tests...\", \"isStderr\": false },\n    { \"id\": \"out-worker-1714300000-0-43\", \"lineNumber\": 43, \"timestamp\": 1714300013100, \"content\": \"✓ All 23 tests passed\", \"isStderr\": false }\n  ],\n  \"meta\": { \"totalLines\": 43, \"bufferedLines\": 43, \"firstLineNumber\": 1, \"lastLineNumber\": 43, \"truncated\": false }\n}"
    }
  ]
}
```

**Error cases**:

- Pane not found: `{ "isError": true, "content": [{ "type": "text", "text": "No output buffer for pane '{paneId}'." }] }`
- Permission denied: `{ "isError": true, "content": [{ "type": "text", "text": "Capability 'output_read' not granted to this session." }] }`

#### 16.5.2 `athena_stream_output` — Subscribe to real-time agent output

```json
{
  "name": "athena_stream_output",
  "description": "Subscribe to a real-time stream of an agent's output. New output entries are pushed via MCP server-initiated notifications (notifications/output). Returns a subscription ID for later cancellation.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "paneId": {
        "type": "string",
        "description": "The pane ID of the agent to stream."
      },
      "sinceLineNumber": {
        "type": "number",
        "description": "Start streaming from this line number (exclusive). If omitted, streams from the current end of the buffer (only new output).",
        "default": null
      }
    },
    "required": ["paneId"]
  }
}
```

**Behavior**: Creates an `OutputSubscription` in the `OutputBufferManager`. The manager's push loop (500ms interval) checks for new entries since the subscription's cursor and sends them as MCP server-initiated notifications:

```json
{
  "jsonrpc": "2.0",
  "method": "notifications/output",
  "params": {
    "subscriptionId": "sub-abc123",
    "paneId": "worker-1714300000-0",
    "entries": [
      {
        "id": "out-worker-1714300000-0-44",
        "lineNumber": 44,
        "timestamp": 1714300014000,
        "content": "Building project..."
      }
    ]
  }
}
```

**Immediate response** (acknowledges the subscription):

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"subscriptionId\": \"sub-abc123\", \"paneId\": \"worker-1714300000-0\", \"status\": \"streaming\", \"note\": \"Output will be pushed via notifications/output. Use athena_stop_stream to cancel.\"}"
    }
  ]
}
```

**Note on transport**: Streaming uses the existing TCP connection's server-initiated notification mechanism — the same channel used by `broadcastNotification`. No WebSocket is required. The MCP JSON-RPC protocol already supports server-initiated notifications. If the subscriber disconnects, the subscription is automatically cleaned up when the socket closes.

#### 16.5.3 `athena_stop_stream` — Cancel an output subscription

```json
{
  "name": "athena_stop_stream",
  "description": "Cancel a previously created output stream subscription.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "subscriptionId": {
        "type": "string",
        "description": "The subscription ID returned by athena_stream_output."
      }
    },
    "required": ["subscriptionId"]
  }
}
```

**Response**: `{ "content": [{ "type": "text", "text": "Subscription sub-abc123 cancelled." }] }`

#### 16.5.4 `athena_list_agents` — List all running agents

```json
{
  "name": "athena_list_agents",
  "description": "List all connected/running agents with their pane IDs, status, and output metadata. Use this to discover which agents are available for output reading.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "status": {
        "type": "string",
        "enum": ["active", "idle", "waiting_input", "disconnected", "all"],
        "description": "Filter by agent status. 'all' returns every known agent including exited ones with buffered output.",
        "default": "active"
      }
    }
  }
}
```

**Behavior**: Merges data from `OutputBufferManager.listBuffers()` and `pluginHost` sessions. For each pane with a buffer or a known session, constructs an `AgentInfo` object.

**Response**:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\n  \"agents\": [\n    {\n      \"paneId\": \"worker-1714300000-0\",\n      \"agentType\": \"claude\",\n      \"status\": \"active\",\n      \"connectedAt\": 1714300000000,\n      \"lastOutputAt\": 1714300014000,\n      \"lastLineNumber\": 44,\n      \"bufferEntryCount\": 44,\n      \"bufferByteSize\": 12400,\n      \"hasExited\": false,\n      \"exitCode\": null\n    }\n  ],\n  \"totalCount\": 1\n}"
    }
  ]
}
```

#### 16.5.5 `athena_get_output_since` — Read output incrementally

```json
{
  "name": "athena_get_output_since",
  "description": "Read an agent's output since a specific point in time. Supports three cursor types: line number, timestamp, or message ID. Use this for incremental polling — call once to get the initial output, then call again with the last entry's ID/lineNumber to get only new output.",
  "inputSchema": {
    "type": "object",
    "properties": {
      "paneId": {
        "type": "string",
        "description": "The pane ID of the agent."
      },
      "since": {
        "type": "object",
        "description": "The cursor to read output after. Exactly one field must be provided.",
        "properties": {
          "lineNumber": {
            "type": "number",
            "description": "Read output after this line number (exclusive lower bound)."
          },
          "timestamp": {
            "type": "number",
            "description": "Read output after this Unix ms timestamp (exclusive lower bound)."
          },
          "messageId": {
            "type": "string",
            "description": "Read output after this message ID (exclusive lower bound). If the ID was evicted from the buffer, returns all buffered output."
          }
        }
      },
      "limit": {
        "type": "number",
        "description": "Maximum number of entries to return. Prevents unbounded responses.",
        "default": 500
      }
    },
    "required": ["paneId", "since"]
  }
}
```

**Behavior**: Reads from the `AgentOutputBuffer` using one of three cursor strategies:

| Cursor Type  | Method Called                   | Use Case                                                 |
| ------------ | ------------------------------- | -------------------------------------------------------- |
| `lineNumber` | `buffer.readSinceLine(n)`       | Polling with a known line position                       |
| `timestamp`  | `buffer.readSinceTimestamp(ts)` | Time-based queries                                       |
| `messageId`  | `buffer.readSinceId(id)`        | Most precise — use the `id` from the last entry received |

The `limit` parameter caps the response size. If more entries exist than `limit`, the response includes `truncated: true` and the caller should paginate using the last returned entry's cursor.

**Response**:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\n  \"paneId\": \"worker-1714300000-0\",\n  \"entries\": [\n    { \"id\": \"out-worker-1714300000-0-45\", \"lineNumber\": 45, \"timestamp\": 1714300015000, \"content\": \"Deploying to staging...\", \"isStderr\": false },\n    { \"id\": \"out-worker-1714300000-0-46\", \"lineNumber\": 46, \"timestamp\": 1714300016000, \"content\": \"✓ Deployment successful\", \"isStderr\": false }\n  ],\n  \"meta\": { \"totalLines\": 46, \"bufferedLines\": 46, \"firstLineNumber\": 1, \"lastLineNumber\": 46, \"truncated\": false }\n}"
    }
  ]
}
```

### 16.6 Namespace and Capability Updates

#### 16.6.1 New Namespace: output

Updated full namespace table:

| Namespace | Purpose                            | Phase                   |
| --------- | ---------------------------------- | ----------------------- |
| notify    | User notifications                 | Phase 1                 |
| status    | Agent status reporting             | Phase 1                 |
| task      | Task board interaction             | Phase 1 (existing)      |
| output    | Agent output reading and streaming | Phase 1 (this addition) |
| control   | Agent lifecycle control            | Phase 2                 |
| input     | Request user input                 | Phase 2                 |
| file      | File sharing between agents        | Phase 3                 |

#### 16.6.2 New Capabilities: output_read and output_stream

```typescript
// Added to PluginCapability union in src/types/plugin.ts:

export type PluginCapability =
  | 'notifications'
  | 'status'
  | 'tasks'
  | 'agent_control'
  | 'user_input'
  | 'file_access'
  | 'swarm'
  | 'output_read' // NEW - can read other agents terminal output
  | 'output_stream' // NEW - can subscribe to real-time output streams
```

#### 16.6.3 Default Capability Grants

```typescript
const DEFAULT_CAPABILITIES: Record<AgentType, PluginCapability[]> = {
  claude: ['notifications', 'status', 'tasks', 'user_input', 'output_read', 'output_stream'],
  codex: ['notifications', 'status', 'tasks', 'user_input', 'output_read', 'output_stream'],
  opencode: ['notifications', 'status', 'tasks', 'user_input', 'output_read', 'output_stream'],
  gemini: ['notifications', 'status', 'tasks', 'user_input', 'output_read', 'output_stream'],
  custom: ['notifications', 'status', 'output_read'], // Can read but not stream by default
  shell: ['notifications', 'status'], // No output reading for raw shells
}
```

**Design rationale**: Standard AI agents (claude, codex, opencode, gemini) are trusted to read other agents output - this is how the orchestrator coordinates. Custom agents get read access but not streaming (to limit resource usage). Raw shell sessions get no output access.

### 16.7 Security and Performance

#### 16.7.1 Access Control

**Rule 1 - Self-read is always allowed**: An agent can always read its own output (where paneId matches the session paneId). This does not require the output_read capability.

**Rule 2 - Cross-read requires capability**: Reading another agent output requires the output_read capability. If a session lacks it:

```json
{
  "isError": true,
  "content": [
    {
      "type": "text",
      "text": "Capability output_read not granted to this session. You can only read your own output."
    }
  ]
}
```

**Rule 3 - Streaming requires separate capability**: Subscribing to another agent output stream requires output_stream. This is a distinct capability because streaming consumes server resources (push loop, memory for subscription state).

**Rule 4 - Orchestrator has full access**: The orchestrator session (identified by agentType claude and launched by the user) implicitly has both capabilities. This is enforced through the DEFAULT_CAPABILITIES map.

#### 16.7.2 Output Size Limits

| Limit                                            | Value                                | Rationale                                                                   |
| ------------------------------------------------ | ------------------------------------ | --------------------------------------------------------------------------- |
| Max entries per buffer                           | 2,000 lines                          | Prevents unbounded growth; ~2000 lines covers most task outputs             |
| Max bytes per buffer                             | 512 KB                               | 2x the existing MAX_HISTORY_BYTES (100KB); allows richer structured storage |
| Max entries per athena_read_output response      | 2,000 (full buffer)                  | Capped by buffer size itself                                                |
| Max entries per athena_get_output_since response | 500 (default), 2,000 (max via limit) | Prevents massive responses on first incremental read                        |
| Max subscriptions per session                    | 10                                   | Prevents subscription abuse                                                 |
| Max subscriptions per pane                       | 25                                   | Prevents fan-out resource exhaustion                                        |

#### 16.7.3 Truncation Strategy

When a response exceeds entry limits:

1. If tail is specified in athena_read_output: return exactly the last N entries.
2. If limit is specified in athena_get_output_since: return up to N entries starting from the cursor, and set truncated: true in the meta. The caller should issue a follow-up request with the cursor advanced to the last received entry.
3. If no limit is specified and the buffer is very large (>500 entries): return the first 500 entries with truncated: true and a message suggesting the tail or since parameters for pagination.

#### 16.7.4 Rate Limiting

| Operation               | Limit                    | Enforcement                                                          |
| ----------------------- | ------------------------ | -------------------------------------------------------------------- |
| athena_read_output      | 30 reads/minute/session  | Drop silently above limit; return error on 3rd consecutive violation |
| athena_get_output_since | 60 reads/minute/session  | Incremental reads are cheap; higher limit                            |
| athena_stream_output    | 5 creates/minute/session | Subscription setup is heavier; lower limit                           |
| athena_list_agents      | 20 calls/minute/session  | Snapshot queries are cheap but should not be polled aggressively     |

#### 16.7.5 Memory Management

- Buffers for exited agents are retained for **5 minutes** after exit, then garbage collected. This allows post-mortem reads without leaking memory indefinitely.
- A periodic sweep (every 60s) checks for exited agents with no active subscriptions and buffers older than 5 minutes, and deletes them.
- The total memory budget for all output buffers combined is **8 MB**. If the sum of all buffer byte sizes exceeds this, the oldest (by lastOutputAt) exited agent buffer is evicted first, then the oldest active agent buffer is trimmed.

### 16.8 Integration Points

#### 16.8.1 Orchestrator AI Integration

The Athena Orchestrator (athenaOrchestrator.ts) currently builds its system prompt with activePanes metadata. With output reading, the orchestrator can:

1. **On each orchestration turn**, call athena_list_agents to discover active agents.
2. **Before delegating a task**, call athena_get_output_since on relevant agents to check if they are available and not stuck.
3. **After task delegation**, poll athena_get_output_since to monitor progress without waiting for status_update calls.
4. **On agent stall detection**, call athena_read_output with tail: 50 to read the last 50 lines and diagnose the issue.
5. **When an agent reports completion**, call athena_read_output to verify the output before marking the task as done.

**Updated orchestrator system prompt** (addition to buildSystemPrompt):

```
OUTPUT READING:
You can read agent terminal output to monitor progress and diagnose issues:
- Use athena_list_agents to see all running agents and their status.
- Use athena_read_output (with tail: 50) to quickly check an agent latest output.
- Use athena_get_output_since to incrementally poll for new output (more efficient than full reads).
- Use athena_stream_output to subscribe to real-time output for long-running tasks.
When an agent appears stalled or you need to verify completion, read their output before taking action.
```

#### 16.8.2 Orchestrator Tool Registration

The output reading tools are added to the orchestrator own tool set in toolExecutor.ts:

```typescript
// In ORCHESTRATOR_TOOLS array in toolExecutor.ts, add:

{
  name: 'athena_read_output',
  description: "Read an agent terminal output buffer. Use tail parameter for recent output.",
  input_type: { paneId: 'string', tail: 'number?' },
},
{
  name: 'athena_get_output_since',
  description: "Read agent output incrementally since a cursor (line number, timestamp, or message ID).",
  input_type: { paneId: 'string', since: 'object', limit: 'number?' },
},
{
  name: 'athena_list_agents',
  description: "List all running agents with their pane IDs, status, and output metadata.",
  input_type: { status: 'string?' },
},
{
  name: 'athena_stream_output',
  description: "Subscribe to an agent real-time output stream.",
  input_type: { paneId: 'string', sinceLineNumber: 'number?' },
},
{
  name: 'athena_stop_stream',
  description: "Cancel an output stream subscription.",
  input_type: { subscriptionId: 'string' },
},
```

The orchestrator executeToolCall function dispatches these to the MCP server own tool handler (direct function call, not TCP loopback).

#### 16.8.3 MCP Server Handler Integration

In mcpServer.ts, the handleToolCall function gains new cases. See section 16.5 for the full handler code for each tool. The key dispatch pattern:

- athena_read_output: calls outputBufferManager.getBuffer(paneId), returns entries + meta
- athena_get_output_since: calls getBuffer + readSinceLine/readSinceTimestamp/readSinceId, applies limit
- athena_list_agents: merges listBuffers() with getPluginHostSessions(), returns AgentInfo[]
- athena_stream_output: calls getBuffer + addSubscription, returns subscriptionId
- athena_stop_stream: calls removeSubscription

#### 16.8.4 Session-Per-Socket Tracking

The current MCP server does not track which net.Socket belongs to which session. To enforce per-session capability checks on output reading, we need to associate sockets with sessions:

```typescript
// In mcpServer.ts, add:

const socketSessions = new Map<net.Socket, { sessionId: string; capabilities: string[]; paneId: string | null }>()

// On initialize success:
socketSessions.set(socket, { sessionId: randomUUID(), capabilities: [...], paneId: req.params?.paneId || null })

// On socket close:
socket.on('close', () => { activeClients.delete(socket); socketSessions.delete(socket) })
```

This is a prerequisite for capability enforcement on the output tools.

### 16.9 Updated API Reference (Phase 1)

| Tool                    | Required Capability           | Description                              |
| ----------------------- | ----------------------------- | ---------------------------------------- |
| notify                  | notifications                 | Send a notification to the user          |
| status_update           | status                        | Report agent status                      |
| create_tasks            | tasks                         | Add tasks to Kanban board (existing)     |
| get_next_task           | tasks                         | Pull next available task (existing)      |
| update_task_status      | tasks                         | Update task status (existing)            |
| spawn_agents            | swarm                         | Spawn new agent workers (existing)       |
| athena_read_output      | output_read (or self-read)    | Read an agent terminal output buffer     |
| athena_get_output_since | output_read (or self-read)    | Read output incrementally since a cursor |
| athena_list_agents      | none (all sessions)           | List running agents and their metadata   |
| athena_stream_output    | output_stream (or self-read)  | Subscribe to real-time output stream     |
| athena_stop_stream      | none (own subscriptions only) | Cancel an output stream subscription     |

### 16.10 Updated File Structure

```
athenas-core/
├── electron/
│   ├── main.ts                 # + initOutputBuffers(mainWindow)
│   ├── mcpServer.ts            # + athena_read_output, athena_get_output_since, athena_list_agents,
│   │                           #   athena_stream_output, athena_stop_stream tools;
│   │                           #   + socketSessions Map for per-socket session tracking
│   ├── outputBuffer.ts         # NEW - AgentOutputBuffer ring buffer class
│   ├── outputBufferManager.ts  # NEW - buffer lifecycle, subscriptions, streaming push loop
│   ├── ptyManager.ts           # + app.emit('pty:output', ...) in onData handler
│   ├── pluginHost.ts           # (no changes - OutputBufferManager is standalone)
│   └── ...
├── src/
│   ├── types/
│   │   ├── output.ts           # NEW - AgentOutput, AgentInfo, OutputSubscription, ReadOutputResponse
│   │   ├── plugin.ts           # + output_read, output_stream in PluginCapability union
│   │   └── ...
│   └── ...
└── docs/
    └── plugin-system-spec.md   # This file (updated with Section 16)
```

### 16.11 Updated Open Questions

| #   | Question                                                       | Decision                                                                                                                                 |
| --- | -------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| 6   | Can agents read each other terminal output?                    | **Yes, with capability grant** - output_read capability allows cross-agent reads; self-read is always allowed                            |
| 7   | Should output buffers be persisted to disk for crash recovery? | **No (Phase 1)** - In-memory only. Disk persistence adds complexity; PTY output is ephemeral and reproducible by re-running the agent    |
| 8   | Should streaming use WebSocket instead of TCP notifications?   | **No** - TCP server-initiated notifications already work. WebSocket can be added as a thin adapter later if browser-based agents need it |

---

OUTPUT READING SPEC COMPLETE
