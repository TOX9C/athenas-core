# Plugin System Guide

Athena's Core includes a plugin system that allows external tools and AI agents to integrate with the application via the Model Context Protocol (MCP).

## Overview

Plugins are extensions that communicate with Athena through MCP. They can:

- Send notifications to the user
- Request input from the user
- Report status, errors, and completions
- Create and manage tasks
- Read application state (spaces, agents, tasks)
- Read captured terminal output from agent panes
- Forward output to other plugins for processing

## Managing Plugins

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
