import * as net from 'net'
import { BrowserWindow, app } from 'electron'
import * as ptyMgr from './ptyManager'
import { randomUUID } from 'crypto'

const SESSION_TOKEN = randomUUID()

const activeClients = new Set<net.Socket>()

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

export function getMcpToken(): string {
  return SESSION_TOKEN
}

let storeInstance: any = null
async function getStore() {
  if (!storeInstance) {
    const { default: Store } = await import('electron-store')
    storeInstance = new Store()
  }
  return storeInstance
}

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
            required: ['title']
          }
        }
      },
      required: ['spaceId', 'tasks']
    }
  },
  {
    name: 'get_next_task',
    description: 'Pull the next available To Do task from the board.',
    inputSchema: { type: 'object', properties: {} }
  },
  {
    name: 'update_task_status',
    description: 'Update the status of a specific task.',
    inputSchema: {
      type: 'object',
      properties: {
        taskId: { type: 'string' },
        status: { type: 'string', enum: ['todo', 'in_progress', 'in_review', 'complete'] }
      },
      required: ['taskId', 'status']
    }
  },
  {
    name: 'spawn_agents',
    description: 'Spawn new terminal worker agents.',
    inputSchema: {
      type: 'object',
      properties: {
        count: { type: 'number' },
        cwd: { type: 'string' },
        instruction: { type: 'string' }
      },
      required: ['count', 'cwd']
    }
  }
]

export function initMcpServer(mainWindow: BrowserWindow): void {
  const server = net.createServer((socket) => {
    socket.on('close', () => activeClients.delete(socket))
    socket.on('error', () => activeClients.delete(socket))
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

async function handleRequest(socket: net.Socket, mainWindow: BrowserWindow, req: any) {
  const send = (res: any) => socket.write(JSON.stringify(res) + '\n')

  if (req.method === 'initialize') {
    if (req.params?.token !== SESSION_TOKEN) {
      send({ jsonrpc: '2.0', id: req.id, error: { code: -32600, message: 'Invalid or missing auth token' } })
      socket.end()
      return
    }
    send({ jsonrpc: '2.0', id: req.id, result: { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'athena-orchestrator', version: '1.0.0' } } })
    activeClients.add(socket)
  } else if (req.method === 'notifications/initialized') {
    // no-op
  } else if (req.method === 'tools/list') {
    send({ jsonrpc: '2.0', id: req.id, result: { tools: TOOLS } })
  } else if (req.method === 'tools/call') {
    send({ jsonrpc: '2.0', id: req.id, result: await handleToolCall(mainWindow, req.params.name, req.params.arguments) })
  }
}

// Next task will fill handleToolCall
async function handleToolCall(mainWindow: BrowserWindow, name: string, args: any) {
  try {
    const store = await getStore()
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

    if (name === 'spawn_agents') {
      const shell = process.platform === 'win32' ? 'powershell.exe' : (process.env.SHELL || '/bin/zsh')
      const instruction = args.instruction || 'You are an Athena Swarm Worker. Use the get_next_task MCP tool to pull work and update_task_status to complete it.'

      const proxyPath = require('path').join(__dirname, '../../bin/mcp-proxy.js')
      const mcpConfig = JSON.stringify({ athena: { command: 'node', args: [proxyPath], env: { ATHENA_MCP_TOKEN: SESSION_TOKEN } } })
      const mcpEnv = `export CLAUDE_MCP_SERVERS='${mcpConfig.replace(/'/g, "'\\''")}';`
      const agentCmd = `${mcpEnv} claude -p "${instruction}"`

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