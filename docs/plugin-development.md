# Plugin Development Guide

This guide covers how to build plugins for Athena's Core using the Model Context Protocol (MCP).

## Architecture

Athena plugins communicate via MCP, which provides a standardized interface for AI agents and tools. The architecture supports two transport modes:

1. **Stdio** — Plugin runs as a child process, communicates over stdin/stdout
2. **WebSocket** — Plugin connects to Athena's WebSocket server at `ws://localhost:<port>`

## Plugin Manifest

Every plugin requires a manifest describing its identity, permissions, and entry point:

```typescript
interface PluginManifest {
  id: string // Unique identifier (e.g., 'com.example.my-plugin')
  name: string // Human-readable name
  version: string // Semver version
  description: string // What the plugin does
  author: string // Author name
  entryPoint: string // Path to the plugin entry script
  permissions: PluginPermission[] // Required permissions
  mcpConfig?: {
    // MCP server configuration (optional)
    command: string // Command to start the MCP server
    args: string[] // Arguments for the command
    env?: Record<string, string> // Environment variables
  }
}
```

## Creating an MCP Plugin

### 1. Set Up the Project

```bash
mkdir my-athena-plugin && cd my-athena-plugin
npm init -y
npm install @modelcontextprotocol/sdk zod
```

### 2. Implement the MCP Server

```typescript
import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'
import { z } from 'zod'

const server = new McpServer({
  name: 'my-plugin',
  version: '1.0.0',
})

// Register a tool
server.tool(
  'my_custom_tool',
  'Does something custom',
  { input: z.string().describe('The input to process') },
  async ({ input }) => {
    return {
      content: [{ type: 'text', text: `Processed: ${input}` }],
    }
  },
)

// Start the server
const transport = new StdioServerTransport()
await server.connect(transport)
```

### 3. Register with Athena

In the Athena renderer (or via IPC):

```typescript
await window.athena.plugins.register({
  id: 'com.example.my-plugin',
  name: 'My Plugin',
  version: '1.0.0',
  description: 'Custom plugin for Athena',
  author: 'Developer',
  entryPoint: './index.js',
  permissions: ['notifications'],
  mcpConfig: {
    command: 'node',
    args: ['./dist/index.js'],
  },
})

await window.athena.plugins.enable('com.example.my-plugin')
```

## Available MCP Tools

When your plugin runs as an MCP server inside Athena, it can call these Athena-provided tools:

### `notify`

Send a notification to the user.

```typescript
server.tool('notify', 'Send notification', {
  type: z.enum(['info', 'warning', 'error', 'success']),
  title: z.string(),
  message: z.string(),
  priority: z.enum(['low', 'normal', 'high', 'critical']),
}, async (params) => { ... })
```

### `request_input`

Prompt the user for input.

```typescript
{
  prompt: string
  defaultResponse?: string
  timeout?: number  // ms
}
```

### `update_status`

Update an agent's status.

```typescript
{
  agentId: string
  status: AgentStatus  // 'running' | 'idle' | 'error' | 'waiting' | 'done' | 'blocked' | 'stalled'
  message?: string
  progress?: number  // 0-1
}
```

### `report_error`

Report an error from an agent.

```typescript
{
  agentId: string
  error: string
  stack?: string
  code?: string | number
  recoverable: boolean
}
```

### `report_completion`

Report task completion.

```typescript
{
  agentId: string
  summary: string
  artifacts?: string[]
  metrics?: Record<string, number>
  duration?: number
}
```

### `create_tasks`

Create new tasks.

```typescript
{
  tasks: Array<{ title: string; description?: string }>
}
```

### `get_next_task`

Get the next pending task.

```typescript
{
  agentId?: string  // Filter by agent
}
```

### `update_task_status`

Update a task's status.

```typescript
{
  taskId: string
  status: string
}
```

## Available MCP Resources

Athena exposes these resources for plugins to read:

### `athena://state`

Full application state: active space, spaces, theme, agents, tasks.

### `athena://agents`

List of all agent states (id, type, status, cwd, etc.).

### `athena://tasks`

List of all tasks (id, title, status, description).

## Testing Your Plugin

### Unit Tests

Use vitest for testing:

```typescript
import { describe, it, expect } from 'vitest'

describe('my plugin', () => {
  it('should process input', async () => {
    const result = await processInput('test')
    expect(result).toContain('Processed')
  })
})
```

### Integration Tests

Test your plugin against Athena's MCP server:

```typescript
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StdioClientTransport } from '@modelcontextprotocol/sdk/client/stdio.js'

const transport = new StdioClientTransport({
  command: 'node',
  args: ['./dist/index.js'],
})

const client = new Client({ name: 'test-client', version: '1.0.0' })
await client.connect(transport)

const tools = await client.listTools()
expect(tools.tools.length).toBeGreaterThan(0)
```

## Best Practices

1. **Declare minimal permissions** — Only request what you need
2. **Handle errors gracefully** — Use `report_error` with `recoverable: true` for transient issues
3. **Report progress** — Use `update_status` with `progress` for long-running operations
4. **Clean up resources** — Close connections and stop processes on disable
5. **Use meaningful IDs** — Prefix plugin IDs with your domain (e.g., `com.example.plugin-name`)
6. **Version your plugin** — Follow semver; breaking changes require major version bumps
