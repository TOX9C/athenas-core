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
    send({ jsonrpc: '2.0', id: req.id, result: { protocolVersion: '2024-11-05', capabilities: { tools: {} }, serverInfo: { name: 'athena-orchestrator', version: '1.0.0' } } })
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
  return { content: [{ type: 'text', text: 'Not implemented yet' }] }
}