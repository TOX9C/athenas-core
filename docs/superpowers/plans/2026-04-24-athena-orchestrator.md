# Athena Orchestrator Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Transform Athena into an orchestrator by spinning up an MCP local server bridging Kanban board tasks and PTY Agent spawning via a Node proxy.

**Architecture:** Electron Main will host a lightweight TCP server (`mcpServer.ts`) processing line-delimited JSON-RPC (MCP protocol). A proxy script (`mcp-proxy.js`) connects standard I/O pipes to this TCP port. The tools exposed enable agents to pull from the `electron-store` "tasks" object and arbitrarily deploy new PTY `claude` instances.

**Tech Stack:** Node `net` Socket, Electron IPC, Zustand/Electron-store.

---

### Task 1: Create the MCP Proxy Bridge

**Files:**

- Create: `bin/mcp-proxy.js`

- [ ] **Step 1: Write the proxy script**
      Create the folder `bin/` if it doesn't exist.
      Write a script that pipes `process.stdin` to a TCP socket on port `4545`, and pipes the socket back to `process.stdout`.

```javascript
#!/usr/bin/env node
const net = require('net')

const client = net.createConnection({ port: 4545 }, () => {
  process.stdin.pipe(client)
  client.pipe(process.stdout)
})

client.on('error', (err) => {
  console.error('MCP Proxy connection error:', err.message)
  process.exit(1)
})

client.on('end', () => {
  process.exit(0)
})
```

- [ ] **Step 2: Make executable**
      Ensure standard execution properties for the bin manually (via bash):

```bash
chmod +x bin/mcp-proxy.js
```

- [ ] **Step 3: Commit**

```bash
git add bin/mcp-proxy.js
git commit -m "feat(orchestrator): create stdio to tcp mcp proxy script"
```

---

### Task 2: Implement MCP Server Core

**Files:**

- Create: `electron/mcpServer.ts`

- [ ] **Step 1: Write the socket server framework**
      Create `electron/mcpServer.ts`. Import `net`, `electron-store`, `path`, and `crypto` (for ids). Set up the base tool listings.

```typescript
import * as net from 'net'
import { BrowserWindow } from 'electron'
import * as ptyMgr from './ptyManager'
import Store from 'electron-store'
import { randomUUID } from 'crypto'

const store = new Store()

const TOOLS = [
  {
    name: 'create_tasks',
    description: 'Add new tasks to the Kanban board.',
    inputSchema: {
      type: 'object',
      properties: {
        spaceId: { type: 'string' },
        tasks: {
          type: 'array',
          items: {
            type: 'object',
            properties: { title: { type: 'string' }, description: { type: 'string' } },
            required: ['title'],
          },
        },
      },
      required: ['spaceId', 'tasks'],
    },
  },
  {
    name: 'get_next_task',
    description: 'Pull the next available To Do task from the board.',
    inputSchema: { type: 'object', properties: {} },
  },
  {
    name: 'update_task_status',
    description: 'Update the status of a specific task.',
    inputSchema: {
      type: 'object',
      properties: {
        taskId: { type: 'string' },
        status: { type: 'string', enum: ['todo', 'in_progress', 'in_review', 'complete'] },
      },
      required: ['taskId', 'status'],
    },
  },
  {
    name: 'spawn_agents',
    description: 'Spawn new terminal worker agents.',
    inputSchema: {
      type: 'object',
      properties: {
        count: { type: 'number' },
        cwd: { type: 'string' },
        instruction: { type: 'string' },
      },
      required: ['count', 'cwd'],
    },
  },
]

export function initMcpServer(mainWindow: BrowserWindow): void {
  const server = net.createServer((socket) => {
    let buffer = ''
    socket.on('data', async (chunk) => {
      buffer += chunk.toString()
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      for (const line of lines) {
        if (!line.trim()) continue
        try {
          const req = JSON.parse(line)
          await handleRequest(socket, mainWindow, req)
        } catch (e) {
          // parse error
        }
      }
    })
  })

  server.listen(4545, '127.0.0.1')
}

async function handleRequest(socket: net.Socket, mainWindow: BrowserWindow, req: any) {
  const send = (res: any) => socket.write(JSON.stringify(res) + '\n')

  if (req.method === 'initialize') {
    send({
      jsonrpc: '2.0',
      id: req.id,
      result: {
        protocolVersion: '2024-11-05',
        capabilities: { tools: {} },
        serverInfo: { name: 'athena-orchestrator', version: '1.0.0' },
      },
    })
  } else if (req.method === 'notifications/initialized') {
    // no-op
  } else if (req.method === 'tools/list') {
    send({ jsonrpc: '2.0', id: req.id, result: { tools: TOOLS } })
  } else if (req.method === 'tools/call') {
    send({
      jsonrpc: '2.0',
      id: req.id,
      result: await handleToolCall(mainWindow, req.params.name, req.params.arguments),
    })
  }
}

// Next task will fill handleToolCall
async function handleToolCall(mainWindow: BrowserWindow, name: string, args: any) {
  return { content: [{ type: 'text', text: 'Not implemented yet' }] }
}
```

- [ ] **Step 2: Commit**

```bash
git add electron/mcpServer.ts
git commit -m "feat(orchestrator): setup mcp socket server core"
```

---

### Task 3: Implement Kanban Store Operations

**Files:**

- Modify: `electron/mcpServer.ts`

- [ ] **Step 1: Implement `handleToolCall` for Kanban logic**
      Update `handleToolCall` in `electron/mcpServer.ts`:

```typescript
async function handleToolCall(mainWindow: BrowserWindow, name: string, args: any) {
  try {
    const tasks: any[] = store.get('tasks') as any[] || []

    if (name === 'create_tasks') {
      const newTasks = args.tasks.map((t: any) => ({
        id: randomUUID(),
        spaceId: args.spaceId,
        title: t.title,
        description: t.description || '',
        status: 'todo',
        order: Date.now(),
        createdAt: Date.now()
      }))
      store.set('tasks', [...tasks, ...newTasks])
      // Trigger UI sync
      mainWindow.webContents.send('store:updateTasks')
      return { content: [{ type: 'text', text: `Created ${newTasks.length} tasks.` }] }
    }

    if (name === 'get_next_task') {
      const todoTasks = tasks.filter(t => t.status === 'todo').sort((a, b) => a.order - b.order)
      if (todoTasks.length === 0) {
        return { content: [{ type: 'text', text: 'No tasks available.' }] }
      }

      const task = todoTasks[0]
      task.status = 'in_progress'
      store.set('tasks', tasks)
      mainWindow.webContents.send('store:updateTasks')

      return { content: [{ type: 'text', text: JSON.stringify(task, null, 2) }] }
    }

    if (name === 'update_task_status') {
      const task = tasks.find(t => t.id === args.taskId)
      if (!task) return { isError: true, content: [{ type: 'text', text: 'Task not found' }] }

      task.status = args.status
      store.set('tasks', tasks)
      mainWindow.webContents.send('store:updateTasks')

      return { content: [{ type: 'text', text: 'Task updated successfully.' }] }
    }
```

- [ ] **Step 2: Commit**

```bash
git add electron/mcpServer.ts
git commit -m "feat(orchestrator): link kanban store to mcp server"
```

---

### Task 4: Implement Agent Spawner & Integration

**Files:**

- Modify: `electron/mcpServer.ts`
- Modify: `electron/main.ts`

- [ ] **Step 1: Finish tool implementation for `spawn_agents`**
      Add the final block inside `handleToolCall`:

```typescript
    if (name === 'spawn_agents') {
      const shell = process.platform === 'win32' ? 'powershell.exe' : (process.env.SHELL || '/bin/zsh')
      const instruction = args.instruction || 'You are an Athena Swarm Worker. Use the get_next_task MCP tool to pull work and update_task_status to complete it.'
      const agentCmd = `claude -p "${instruction}"`

      for (let i = 0; i < args.count; i++) {
        const ptyId = `worker-${Date.now()}-${i}`
        ptyMgr.spawn(ptyId, args.cwd, shell, agentCmd, mainWindow)
      }
      return { content: [{ type: 'text', text: `Spawned ${args.count} workers successfully.` }] }
    }

    return { isError: true, content: [{ type: 'text', text: 'Unknown tool.' }] }
  } catch (err: any) {
    return { isError: true, content: [{ type: 'text', text: err.message }] }
  }
}
```

- [ ] **Step 2: Start server in `electron/main.ts`**
      In `electron/main.ts`, inside the `app.whenReady().then(...)` block (near where `browserManager` and `swarmCoordinator` are initialized), import and call it:

```typescript
const { initMcpServer } = await import('./mcpServer')
if (mainWindow) initMcpServer(mainWindow)
```

- [ ] **Step 3: Commit**

```bash
git add electron/mcpServer.ts electron/main.ts
git commit -m "feat(orchestrator): finalize spawner and preload mcp server"
```
