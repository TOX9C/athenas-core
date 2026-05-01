# Claude Code Plugin for Athena's Core

Connects [Claude Code](https://docs.anthropic.com/en/docs/claude-code) to Athena's Core MCP server, enabling task management, notifications, agent coordination, and output forwarding from within Claude Code sessions.

## What This Provides

When Claude Code connects to Athena's MCP server, the following tools become available:

| Tool                    | Description                                              |
| ----------------------- | -------------------------------------------------------- |
| `create_tasks`          | Add tasks to Athena's Kanban board                       |
| `get_next_task`         | Pull the next available To Do task                       |
| `update_task_status`    | Update task status (todo/in_progress/in_review/complete) |
| `spawn_agents`          | Spawn new terminal worker agents in Athena               |
| `notify`                | Send a notification to Athena's notification center      |
| `status_update`         | Update the agent's status in Athena's UI                 |
| `athena_forward_output` | Forward stdout/stderr output to Athena in batched form   |

## Output Forwarding

When output forwarding is enabled, agent stdout and stderr are automatically captured and forwarded to Athena's UI via the `athena_forward_output` MCP tool. This lets you see agent output in Athena's notification panel and terminal views even when the agent is running in a background pane.

### How It Works

1. The `OutputForwarder` class hooks into the agent's stdout/stderr streams
2. Output is batched every 100ms or every 50 lines (whichever comes first)
3. Batches are sent as `athena_forward_output` MCP tool calls over the TCP connection
4. If the connection drops, output is buffered (up to 5000 entries) and flushed on reconnect
5. Athena emits an `output_forwarded` plugin event to the renderer for display

### Enabling Output Forwarding

Set the environment variable before running the setup:

```bash
ATHENA_AUTO_FORWARD_OUTPUT=true ATHENA_MCP_TOKEN=<your-token> node setup.js
```

Or use the `forward` command directly to start a forwarding session:

```bash
ATHENA_MCP_TOKEN=<your-token> node setup.js forward
```

## Quick Start

### 1. Get your Athena MCP token

In Athena, open the command palette or settings and copy the MCP auth token. It's also available via the IPC API as `athena.agents.getToken()`.

### 2. Run the setup script

```bash
# From the plugins/claude-code-athena directory
ATHENA_MCP_TOKEN=<your-token> node setup.js

# Or with all options
ATHENA_MCP_TOKEN=<your-token> \
ATHENA_MCP_PORT=4545 \
ATHENA_SESSION_ID=my-session \
ATHENA_AUTO_FORWARD_OUTPUT=true \
node setup.js --global
```

### 3. Restart Claude Code

Claude Code will automatically discover the `athena` MCP server entry in its config.

## Manual Configuration

Add this to your Claude Code MCP config (`.claude/mcp.json` in your project or `~/.claude/mcp.json` globally):

```json
{
  "mcpServers": {
    "athena": {
      "command": "node",
      "args": ["/path/to/athenas-core/bin/mcp-proxy.js"],
      "env": {
        "ATHENA_MCP_TOKEN": "<your-token>",
        "ATHENA_MCP_PORT": "4545",
        "ATHENA_MCP_HOST": "127.0.0.1",
        "ATHENA_AUTO_FORWARD_OUTPUT": "true"
      }
    }
  }
}
```

## Architecture

```
Claude Code ──stdio──> bin/mcp-proxy.js ──TCP──> Athena MCP Server (port 4545)
                                                   │
                               stdout/stderr ──> OutputForwarder ──> athena_forward_output tool
```

The proxy bridges Claude Code's stdio-based MCP client protocol to Athena's TCP-based MCP server. The `ATHENA_MCP_TOKEN` environment variable is used for authentication during the MCP `initialize` handshake. When `ATHENA_AUTO_FORWARD_OUTPUT=true`, the `OutputForwarder` batches and forwards agent output to Athena.

## Auto-Discovery

When an agent pane in Athena spawns Claude Code via `ptyManager`, the following environment variables are automatically injected:

- `CLAUDE_MCP_SERVERS` — JSON config with the `athena` MCP entry pre-populated
- `ATHENA_MCP_TOKEN` — Auth token for MCP server authentication
- `ATHENA_SESSION_ID` — The pane/session ID for correlation
- `ATHENA_AUTO_FORWARD_OUTPUT` — Set to `true` if output forwarding is enabled for the session

This means Claude Code instances launched from within Athena automatically have MCP connectivity and output forwarding without any manual configuration.

## Commands

| Command             | Description                                              |
| ------------------- | -------------------------------------------------------- |
| `setup` / `install` | Write Athena MCP config to Claude Code's config file     |
| `discover`          | Check if Claude Code is installed and show config status |
| `remove`            | Remove the Athena entry from Claude Code's MCP config    |
| `check`             | Verify if Athena's MCP server is reachable               |
| `forward`           | Start output forwarding (hooks stdout/stderr to Athena)  |

## Environment Variables

| Variable                     | Required    | Default     | Description                                         |
| ---------------------------- | ----------- | ----------- | --------------------------------------------------- |
| `ATHENA_MCP_TOKEN`           | Yes (setup) | —           | Auth token from Athena                              |
| `ATHENA_MCP_PORT`            | No          | `4545`      | Athena MCP server port                              |
| `ATHENA_MCP_HOST`            | No          | `127.0.0.1` | Athena MCP server host                              |
| `ATHENA_SESSION_ID`          | No          | —           | Associate this connection with an Athena session    |
| `ATHENA_AUTO_FORWARD_OUTPUT` | No          | `false`     | Enable automatic stdout/stderr forwarding to Athena |
