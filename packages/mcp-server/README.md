# @athenas-core/mcp-server

MCP server for Athena's Core — the communication bridge between AI agents (Claude Code, OpenCode, Codex, Gemini CLI, etc.) and the Athena desktop app.

## Installation

```bash
npm install @athenas-core/mcp-server
```

## Quick Start

### As a CLI (stdio — for Claude Code, OpenCode, etc.)

```bash
npx athena-mcp-server
```

Add to your agent's MCP config:

```json
{
  "athena": {
    "command": "npx",
    "args": ["@athenas-core/mcp-server"],
    "env": {
      "ATHENA_MCP_TOKEN": "<token>"
    }
  }
}
```

### As a TCP server (spec-compliant primary transport)

```bash
npx athena-mcp-server --transport tcp --tcp-port 4545
```

### As a WebSocket server

```bash
npx athena-mcp-server --transport websocket --port 4546
```

### Programmatic usage

```typescript
import { AthenaMcpServer } from '@athenas-core/mcp-server'

const server = new AthenaMcpServer({
  transport: 'stdio',
  athenaHost: '127.0.0.1',
  athenaPort: 4545,
  authToken: process.env.ATHENA_MCP_TOKEN,
})

await server.start()
```

## Tools

### Phase 1 (Available)

| Tool                       | Description                                      |
| -------------------------- | ------------------------------------------------ |
| `notify`                   | Send a notification to the user (spec-compliant) |
| `status_update`            | Report agent status (spec-compliant)             |
| `request_input`            | Request user input, blocks until response        |
| `athena_notify`            | Extended notification with priority levels       |
| `athena_update_status`     | Update agent status with progress %              |
| `athena_report_error`      | Report an error with recovery info               |
| `athena_report_completion` | Report task completion with artifacts            |

### Phase 2 (Defined, returns "not yet available")

| Tool             | Description               |
| ---------------- | ------------------------- |
| `control_pause`  | Pause an agent pane       |
| `control_resume` | Resume a paused agent     |
| `control_cancel` | Cancel/terminate an agent |

## Resources

| URI                   | Description                     |
| --------------------- | ------------------------------- |
| `athena://agents`     | Current state of all agents     |
| `athena://agent/{id}` | State of a specific agent       |
| `athena://app-state`  | Full application state snapshot |

## Transports

| Transport   | Use Case                                      |
| ----------- | --------------------------------------------- |
| `stdio`     | CLI agent integration (Claude Code, OpenCode) |
| `tcp`       | Spec-compliant primary transport (:4545)      |
| `websocket` | Real-time communication with the Electron app |

## Configuration

| CLI Flag        | Env Variable             | Default     |
| --------------- | ------------------------ | ----------- |
| `--transport`   | `ATHENA_MCP_TRANSPORT`   | `stdio`     |
| `--port`        | `ATHENA_MCP_PORT`        | `4546`      |
| `--tcp-port`    | `ATHENA_MCP_TCP_PORT`    | `4545`      |
| `--athena-host` | `ATHENA_MCP_HOST`        | `127.0.0.1` |
| `--athena-port` | `ATHENA_MCP_ATHENA_PORT` | `4545`      |
| `--auth-token`  | `ATHENA_MCP_TOKEN`       | —           |

## License

MIT
