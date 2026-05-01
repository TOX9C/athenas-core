# MCP API Reference

Complete reference for the Model Context Protocol (MCP) tools and resources exposed by Athena's Core.

## Server Configuration

```typescript
interface ServerConfig {
  name: string // Server name
  version: string // Server version
  transport: TransportType // 'stdio' | 'websocket'
  websocketPort?: number // Port for WebSocket transport
  athenaHost?: string // Athena host for WebSocket
  athenaPort?: number // Athena port for WebSocket
  authToken?: string // Authentication token
}
```

## Tools

### `notify`

Send a notification to the Athena user interface.

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

### `request_input`

Prompt the user for text input. Blocks until the user responds or timeout.

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

### `update_status`

Update the status of an agent in Athena.

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

### `report_error`

Report an error from an agent.

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

### `report_completion`

Report that an agent has completed its task.

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

| Field   | Type                                             | Required | Description     |
| ------- | ------------------------------------------------ | -------- | --------------- |
| `tasks` | `Array<{ title: string; description?: string }>` | Yes      | Tasks to create |

**Response:** `{ success: boolean, taskIds: string[] }`

---

### `get_next_task`

Get the next pending task for an agent to work on.

**Parameters:**

| Field     | Type     | Required | Description                |
| --------- | -------- | -------- | -------------------------- |
| `agentId` | `string` | No       | Filter by agent assignment |

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

### `athena_forward_output`

Forward output from a pane to a plugin for real-time processing.

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

### Stdio

The MCP server communicates over stdin/stdout using JSON-RPC 2.0. This is the default transport for external MCP clients.

```bash
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
