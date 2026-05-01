# MCP Server Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Broadcast standard MCP log notifications to connected clients when background agents stall or exit.

**Architecture:** The MCP server (`mcpServer.ts`) runs on a local TCP socket. It will maintain a `Set` of active, authenticated connections. To avoid circular imports, the system will use Electron's global `app` module as an Event Bus. `swarmCoordinator.ts` will emit `agent:stalled` and `ptyManager.ts` will emit `agent:exited`. The MCP server listens to these and broadcasts them as standard JSON-RPC progress/log notifications.

**Tech Stack:** TypeScript, Node.js net sockets, Electron `app` Event Emitter, JSON-RPC MCP Protocol.

---

### Task 1: MCP Server Tracking & Broadcasting

**Files:**

- Modify: `electron/mcpServer.ts`

- [ ] **Step 1: Set up client tracking mechanism**
      In `electron/mcpServer.ts`, import `app` from `electron` and create a tracking variable at the root level of the file:

```typescript
import * as net from 'net'
import { BrowserWindow, app } from 'electron' // Add app import
import * as ptyMgr from './ptyManager'
import { randomUUID } from 'crypto'

const SESSION_TOKEN = randomUUID()

// Add active clients tracker
const activeClients = new Set<net.Socket>()

// Add the notification broadcast helper
export function broadcastNotification(method: string, params: any) {
  const payload = JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n'
  for (const client of activeClients) {
    if (!client.destroyed) {
      client.write(payload)
    } else {
      activeClients.delete(client)
    }
  }
}
```

- [ ] **Step 2: Add connection listeners to clear dead sockets**
      Inside the `initMcpServer` function, where `net.createServer` is called:

```typescript
export function initMcpServer(mainWindow: BrowserWindow): void {
  const server = net.createServer((socket) => {
    // Add cleanup handlers for disconnected sockets
    socket.on('close', () => activeClients.delete(socket))
    socket.on('error', () => activeClients.delete(socket))

    let buffer = ''
    // ... existing socket.on('data', ...)
```

- [ ] **Step 3: Register active sockets upon valid authentication**
      Inside the `handleRequest` function, right after validating the initialize request:

```typescript
  if (req.method === 'initialize') {
    if (req.params?.token !== SESSION_TOKEN) {
      send({ jsonrpc: '2.0', id: req.id, error: { code: -32600, message: 'Invalid or missing auth token' } })
      socket.end()
      return
    }
    send({ jsonrpc: '2.0', id: req.id, result: { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'athena-orchestrator', version: '1.0.0' } } })

    // Add to active clients when successfully initialized
    activeClients.add(socket)
```

- [ ] **Step 4: Connect global application events to the broadcaster**
      At the bottom of `initMcpServer`, register the global app event hooks to trigger the push notifications:

```typescript
  server.listen(4545, '127.0.0.1')

  // Listen to application events from other modules
  app.on('agent:stalled', ({ agentId }) => {
    broadcastNotification('notifications/message/level', {
      level: 'warning',
      message: `Agent ${agentId} has stalled (waiting for input).`
    })
  })

  app.on('agent:exited', ({ id, exitCode }) => {
    broadcastNotification('notifications/message/level', {
      level: 'info',
      message: `Agent ${id} exited with code ${exitCode}.`
    })
  })
}
```

- [ ] **Step 5: Verify types and commit**

```bash
git add electron/mcpServer.ts
git commit -m "feat(mcp): implement active client tracking and broadcast notifications"
```

### Task 2: Emit Stalled Events

**Files:**

- Modify: `electron/swarmCoordinator.ts`

- [ ] **Step 1: Import Electron `app` object**
      At the top of the file:

```typescript
import { BrowserWindow, ipcMain, app } from 'electron'
```

- [ ] **Step 2: Emit `agent:stalled` inside the watch interval**
      Inside the `pollInterval` execution inside `ipcMain.on('swarm:watchState', ...)`:

```typescript
let modified = false
for (const agent of state.agents ?? []) {
  if (
    agent.status !== 'done' &&
    agent.status !== 'stalled' &&
    agent.lastActionAt &&
    now - agent.lastActionAt > 90_000
  ) {
    agent.status = 'stalled'
    modified = true

    // Emit application event for MCP/System Hooks
    app.emit('agent:stalled', { agentId: agent.id })
  }
}
```

- [ ] **Step 3: Commit**

```bash
git add electron/swarmCoordinator.ts
git commit -m "feat(orchestrator): emit system event when agent stalls"
```

### Task 3: Emit Exit Events

**Files:**

- Modify: `electron/ptyManager.ts`

- [ ] **Step 1: Import Electron `app` object**
      Add the `app` export to the top Electron imports:

```typescript
import { BrowserWindow, app } from 'electron'
```

- [ ] **Step 2: Emit `agent:exited` in the onExit listener**
      Inside the `spawn` function:

```typescript
ptyProcess.onExit(({ exitCode }) => {
  sessions.delete(id)
  history.delete(id)
  mainWindow.webContents.send(`pty:exit:${id}`, exitCode)

  // Broadcast exit to MCP and system listeners
  app.emit('agent:exited', { id, exitCode })
})
```

- [ ] **Step 3: Commit**

```bash
git add electron/ptyManager.ts
git commit -m "feat(orchestrator): emit system event when agent exits"
```
