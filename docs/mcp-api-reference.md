# MCP API Reference

Complete reference for the Model Context Protocol (MCP) tools and resources exposed by Athena's Core.

## Server Configuration

Athena's canonical desktop integration is the Rust MCP server in `athena-core` over loopback TCP. External stdio clients launch `bin/mcp-proxy.js`, which forwards stdin/stdout to `127.0.0.1:4545`.

```typescript
interface ServerConfig {
  name: string // Server name
  version: string // Server version
  transport: 'tcp' | 'stdio' | 'websocket'
  host?: string // Rust TCP host; defaults to 127.0.0.1
  port?: number // Rust TCP port; defaults to 4545
  websocketPort?: number // Optional Node/WebSocket transport port
  authToken?: string // Authentication token for TCP/WebSocket clients
}
```

### Canonical external-client configuration

```json
{
  "mcpServers": {
    "athena": {
      "command": "node",
      "args": ["/path/to/athenas-core/bin/mcp-proxy.js"],
      "env": {
        "ATHENA_MCP_HOST": "127.0.0.1",
        "ATHENA_MCP_PORT": "4545",
        "ATHENA_MCP_TOKEN": "<TOKEN_FROM_ATHENA>"
      }
    }
  }
}
```

The Tauri app starts the Rust TCP listener on `127.0.0.1:4545` during backend initialization. If that port is temporarily occupied, boot retries the bind until shutdown. The Rust implementation also has a testable line-delimited stdio loop for trusted child-process use. The Node SDK server is a separate optional package, not the desktop backend's canonical transport.

## External tool discovery

`tools/list` is generated from the Rust executor-backed surface. It includes the legacy aliases used by existing clients (`create_tasks`, `get_next_task`, `update_task_status`, `spawn_agents`, `get_output`, `list_agent_panes`, `code_search`, and `search_files`) plus the canonical executor names such as `kanban_create_task`, `fs_read_file`, `read_agent_output`, and `workspace_list`.

Tools that previously returned placeholder responses (`request_input`, `notify`, `status_update`, `athena_forward_output`, `send_message_to_agent`, and `read_agent_messages`) are not advertised by the desktop Rust MCP server until they have a real executor-backed implementation. Existing agent lifecycle/status communication uses the separate agent-comms channel. This is an intentional discovery compatibility change: clients should use the returned `tools/list` result rather than assuming the historical registry.

## Tools

### `notify` (not advertised by desktop Rust MCP)

The historical notification contract is retained here for protocol reference. The desktop Rust MCP server does not advertise this tool; use the agent-comms channel or the optional Node MCP package for notification features.

**Parameters:**

| Field       | Type                                          | Required | Description                          |
| ----------- | --------------------------------------------- | -------- | ------------------------------------ |
| `type`      | `'info' \| 'warning' \| 'error' \| 'success'` | Yes      | Notification category                |
| `title`     | `string`                                      | Yes      | Short title                          |
| `message`   | `string`                                      | Yes      | Detailed message                     |
| `priority`  | `'low' \| 'normal' \| 'high' \| 'critical'`   | Yes      | Priority level                       |
| `agentId`   | `string`                                      | No       | Originating agent ID                 |
| `timestamp` | `number`                                      | No       | Unix timestamp (auto-set if omitted) |

**Response:** `{ success: boolean }`

---

### `request_input` (not advertised by desktop Rust MCP)

The historical blocking-input contract is retained here for protocol reference. The desktop Rust MCP server does not advertise this tool until a real executor-backed UI flow is available.

**Parameters:**

| Field             | Type     | Required | Description              |
| ----------------- | -------- | -------- | ------------------------ |
| `prompt`          | `string` | Yes      | Question to ask the user |
| `defaultResponse` | `string` | No       | Pre-filled default value |
| `timeout`         | `number` | No       | Timeout in milliseconds  |
| `agentId`         | `string` | No       | Agent requesting input   |

**Response:**

```typescript
interface InputResponse {
  value: string // User's response (or default)
  cancelled: boolean // User dismissed the prompt
  timedOut: boolean // Prompt timed out without response
}
```

---

### `update_status` (not advertised by desktop Rust MCP)

The historical status contract is retained here for protocol reference. The desktop Rust MCP server does not advertise this tool; agent lifecycle/status updates use the separate agent-comms channel.

**Parameters:**

| Field      | Type                      | Required | Description                 |
| ---------- | ------------------------- | -------- | --------------------------- |
| `agentId`  | `string`                  | Yes      | Agent to update             |
| `status`   | `AgentStatus`             | Yes      | New status                  |
| `message`  | `string`                  | No       | Status message              |
| `progress` | `number`                  | No       | Progress fraction (0.0–1.0) |
| `details`  | `Record<string, unknown>` | No       | Additional details          |

**AgentStatus values:** `'running' | 'idle' | 'error' | 'waiting' | 'done' | 'blocked' | 'stalled'`

**Response:** `{ success: boolean }`

---

### `report_error` (not advertised by desktop Rust MCP)

The historical error-reporting contract is retained here for protocol reference. The desktop Rust MCP server does not advertise this tool.

**Parameters:**

| Field         | Type                      | Required | Description                    |
| ------------- | ------------------------- | -------- | ------------------------------ |
| `agentId`     | `string`                  | Yes      | Agent that errored             |
| `error`       | `string`                  | Yes      | Error message                  |
| `stack`       | `string`                  | No       | Stack trace                    |
| `code`        | `string \| number`        | No       | Error code                     |
| `recoverable` | `boolean`                 | Yes      | Whether the agent can continue |
| `context`     | `Record<string, unknown>` | No       | Additional context             |

**Response:** `{ success: boolean }`

---

### `report_completion` (not advertised by desktop Rust MCP)

The historical completion-reporting contract is retained here for protocol reference. The desktop Rust MCP server does not advertise this tool.

**Parameters:**

| Field       | Type                     | Required | Description                     |
| ----------- | ------------------------ | -------- | ------------------------------- |
| `agentId`   | `string`                 | Yes      | Completed agent                 |
| `summary`   | `string`                 | Yes      | Completion summary              |
| `artifacts` | `string[]`               | No       | List of produced artifact paths |
| `metrics`   | `Record<string, number>` | No       | Quantitative metrics            |
| `duration`  | `number`                 | No       | Duration in milliseconds        |

**Response:** `{ success: boolean }`

---

### `create_tasks`

Create new tasks in Athena's task system.

**Parameters:**

| Field     | Type                                             | Required | Description                   |
| --------- | ------------------------------------------------ | -------- | ----------------------------- |
| `spaceId` | `string`                                         | Yes      | Workspace receiving the tasks |
| `tasks`   | `Array<{ title: string; description?: string }>` | Yes      | Tasks to create               |

**Response:** `{ success: boolean, taskIds: string[] }`

---

### `get_next_task`

Get the next pending task for an agent to work on.

**Parameters:**

| Field | Type | Required | Description                                     |
| ----- | ---- | -------- | ----------------------------------------------- |
| —     | —    | No       | Uses the active workspace's next available task |

**Response:** `{ task: TaskState | null }`

---

### `update_task_status`

Update a task's status.

**Parameters:**

| Field    | Type     | Required | Description                                        |
| -------- | -------- | -------- | -------------------------------------------------- |
| `taskId` | `string` | Yes      | Task to update                                     |
| `status` | `string` | Yes      | New status (e.g., 'in_progress', 'done', 'failed') |

**Response:** `{ success: boolean }`

---

### `get_output`

Read captured terminal output for a specific pane.

**Parameters:**

| Field       | Type     | Required | Description                            |
| ----------- | -------- | -------- | -------------------------------------- |
| `paneId`    | `string` | Yes      | Terminal pane identifier               |
| `limit`     | `number` | No       | Max lines to return (default 100)      |
| `sinceLine` | `number` | No       | Return lines after this line number    |
| `sinceTime` | `number` | No       | Return lines after this Unix timestamp |

**Response:**

```text
[1] first line of output
[2] second line of output
...
```

Lines are prefixed with `[lineNum]`. Returns a "no output" message if the pane has no captured output.

---

### `list_agent_panes`

List all agent panes with captured output.

**Parameters:** None

**Response:**

```text
pane-1 (claude) — 42 lines, last activity: 2025-04-28T12:00:00.000Z
pane-2 (opencode) — 15 lines, last activity: 2025-04-28T11:55:00.000Z
```

Returns "No agent panes with captured output." if none exist.

---

### `athena_forward_output` (not advertised by desktop Rust MCP)

The historical output-forwarding contract is retained here for protocol reference. The desktop Rust MCP server does not advertise this tool; output forwarding uses the plugin/agent-comms integration.

**Parameters:**

| Field      | Type     | Required | Description                               |
| ---------- | -------- | -------- | ----------------------------------------- |
| `paneId`   | `string` | Yes      | Pane to forward output from               |
| `pluginId` | `string` | Yes      | Target plugin ID                          |
| `options`  | `object` | No       | Forwarding options (batch size, interval) |

**Response:** `{ success: boolean, type: 'output_forwarded' }`

---

## Resources

### `athena://state`

Returns the full application state.

**Response type:** `AthenaAppState`

```typescript
interface AthenaAppState {
  activeSpaceId: string | null
  spaces: SpaceState[]
  theme: string
  activePanel: string
  agents: AgentState[]
  tasks: TaskState[]
}
```

---

### `athena://agents`

Returns all agent states.

**Response type:** `AgentState[]`

```typescript
interface AgentState {
  id: string
  type: string
  role?: string
  status: AgentStatus
  cwd?: string
  pid?: number
  startedAt?: number
  lastActivityAt?: number
  message?: string
  progress?: number
}
```

---

### `athena://tasks`

Returns all tasks.

**Response type:** `TaskState[]`

```typescript
interface TaskState {
  id: string
  title: string
  status: string
  description?: string
  spaceId?: string
}
```

---

## Transport

### TCP (canonical desktop transport)

The Rust server listens on `127.0.0.1:4545` and speaks line-delimited JSON-RPC 2.0. Authenticate TCP clients with the token issued by Athena. External stdio clients should use `bin/mcp-proxy.js` rather than opening the TCP socket themselves.

```bash
ATHENA_MCP_HOST=127.0.0.1 ATHENA_MCP_PORT=4545 \
  node bin/mcp-proxy.js
```

### Stdio

The Rust MCP server also communicates over stdin/stdout using JSON-RPC 2.0 in an explicitly launched trusted child-process context. Stdio uses the same executor-backed tool router as TCP, but intentionally skips TCP token authentication because process ownership is the trust boundary. The desktop Tauri process starts only the TCP transport; it does not consume the app's own stdin/stdout. The `mcp_init(port)` command ensures the Rust TCP listener is active on the requested port: it is idempotent for the active port and returns a conflict error for a different port while another listener is running. TCP shutdown signals the accept loop, interrupts active client reads, and waits for the listener generation to drop before releasing the port for reuse.

```bash
# Direct subprocess mode, when the Rust MCP server is launched by the client
athena-mcp-server
```

### WebSocket

Connect to `ws://localhost:<port>` with the auth token in the `Authorization` header:

```
Authorization: Bearer <session-token>
```

The WebSocket transport uses the same JSON-RPC 2.0 message format over a persistent connection.

---

## Error Handling

All tools return a standard error format on failure:

```json
{
  "isError": true,
  "content": [{ "type": "text", "text": "Error: <description>" }]
}
```

Common error codes:

| Code             | Description                               |
| ---------------- | ----------------------------------------- |
| `UNAUTHORIZED`   | Invalid or missing auth token             |
| `INVALID_PARAMS` | Missing or malformed parameters           |
| `NOT_FOUND`      | Referenced entity (agent, task) not found |
| `TIMEOUT`        | Operation timed out (e.g., input request) |
| `INTERNAL`       | Internal server error                     |
