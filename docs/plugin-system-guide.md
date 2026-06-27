# Plugin Development Guide

This guide covers both how to build plugins for Athena's Core and the runtime/plugin-system API surface itself. Plugins integrate via the Model Context Protocol (MCP).

## Architecture

Athena plugins communicate via MCP, which provides a standardized interface for AI agents and tools. The architecture supports two transport modes:

1. **Stdio** — Plugin runs as a child process, communicates over stdin/stdout
2. **WebSocket** — Plugin connects to Athena's WebSocket server at `ws://localhost:<port>`

Plugins are extensions that communicate with Athena through MCP. They can:

- Send notifications to the user
- Request input from the user
- Report status, errors, and completions
- Create and manage tasks
- Read application state (spaces, agents, tasks)
- Read captured terminal output from agent panes
- Forward output to other plugins for processing

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

## Managing Plugins (Runtime API)

### Listing Installed Plugins

```typescript
const registry = await window.athena.plugins.list()
// Returns: { [pluginId]: { name, version, status, description, author, config, error? } }
```

### Registering a Plugin

```typescript
const result = await window.athena.plugins.register({
  id: 'my-plugin',
  name: 'My Plugin',
  version: '1.0.0',
  description: 'Does useful things',
  author: 'Your Name',
  entryPoint: './index.js',
  permissions: ['terminal', 'notifications'],
  mcpConfig: {
    command: 'node',
    args: ['./my-plugin.js'],
    env: { API_KEY: '...' },
  },
})
// Returns: { success: true, id: 'my-plugin' }
```

### Enabling/Disabling Plugins

```typescript
await window.athena.plugins.enable('my-plugin')
await window.athena.plugins.disable('my-plugin')
```

### Configuring a Plugin

```typescript
await window.athena.plugins.configure('my-plugin', {
  apiKey: 'new-key',
  maxRetries: 5,
})
```

### Removing a Plugin

```typescript
await window.athena.plugins.unregister('my-plugin')
```

## Listening for Plugin Events

```typescript
// Registry changes
const unsub = window.athena.plugins.onRegistryUpdated((registry) => {
  console.log('Plugin registry updated:', registry)
})

// Plugin enabled
window.athena.plugins.onPluginEnabled(({ id, name }) => {
  console.log(`Plugin ${name} enabled`)
})

// Plugin disabled
window.athena.plugins.onPluginDisabled(({ id }) => {
  console.log(`Plugin ${id} disabled`)
})

// Plugin error
window.athena.plugins.onPluginError(({ id, error }) => {
  console.error(`Plugin ${id} error:`, error)
})

// Clean up
unsub()
```

## Plugin Permissions

Plugins declare required permissions in their manifest:

| Permission      | Description                |
| --------------- | -------------------------- |
| `terminal`      | Access to PTY sessions     |
| `filesystem`    | Read/write file system     |
| `notifications` | Send notifications to user |
| `clipboard`     | Access clipboard           |
| `network`       | Make network requests      |

## Plugin Status Lifecycle

```
installed → enabled → disabled → enabled (re-enable)
                     → error → disabled → enabled (fix + re-enable)
```

## MCP Configuration

Plugins with MCP support include an `mcpConfig` in their manifest. When enabled, Athena's orchestrator can discover and use the plugin's MCP tools and resources.

To get MCP configs for all enabled plugins:

```typescript
const configs = await window.athena.plugins.getAllMcpConfigs()
// Returns: { [pluginId]: { name, command, args, env? } }
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

## Output Reading & Forwarding

Plugins can read captured terminal output from agent panes and forward it for real-time processing.

### Reading Output from the Renderer

```typescript
const { outputCapture } = window.athena

// List all panes with captured output
const agents = await outputCapture.listAgents()

// Read output for a specific pane
const lines = await outputCapture.read('pane-1', { limit: 50 })

// Get buffer metadata
const info = await outputCapture.getInfo('pane-1')

// Subscribe to live output
const unsub = outputCapture.onOutputLine((line) => {
  console.log(`[${line.lineNum}] ${line.text}`)
})

// Clear buffer
await outputCapture.clear('pane-1')
```

### Using the OutputForwarder (Plugin Side)

Plugins that produce output should forward it to Athena via the shared `OutputForwarder`:

```typescript
import { createOutputForwarder, hookStreamToForwarder } from '@athenas-core/plugins/shared'

const forwarder = createOutputForwarder({
  pluginId: 'my-plugin',
  athenaHost: 'localhost',
  athenaPort: 9515,
  authToken: sessionToken,
})

await forwarder.connect()

// Forward a single line
forwarder.forward('my-pane', 'Build completed successfully')

// Hook a Node.js stream (stdout/stderr) to forward automatically
hookStreamToForwarder(process.stdout, forwarder, 'my-pane')
hookStreamToForwarder(process.stderr, forwarder, 'my-pane', 'stderr')

// Clean up
forwarder.disconnect()
```

### OutputForwarder Features

- **Batching**: Lines are batched (100ms interval, 50 max lines per batch) to reduce network overhead
- **Reconnect buffering**: Up to 5000 lines buffered during reconnection attempts
- **Auto-reconnect**: Exponential backoff with 5 retries
- **ANSI stripping**: Optional `stripAnsi` config to clean terminal escape codes before forwarding

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
