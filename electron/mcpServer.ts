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
  {
    name: 'notify',
    description:
      'Send a notification to Athena. Use this to surface important information, warnings, or completion messages to the user.',
    inputSchema: {
      type: 'object',
      properties: {
        level: {
          type: 'string',
          enum: ['info', 'warning', 'error', 'success'],
          description: 'Notification severity',
        },
        title: { type: 'string', description: 'Short title for the notification' },
        message: { type: 'string', description: 'Detailed message body' },
        metadata: { type: 'object', description: 'Optional structured metadata' },
      },
      required: ['message'],
    },
  },
  {
    name: 'status_update',
    description:
      'Update your current working status in Athena. Use this to indicate what you are doing, report progress, or signal that you need input.',
    inputSchema: {
      type: 'object',
      properties: {
        status: {
          type: 'string',
          enum: [
            'idle',
            'thinking',
            'working',
            'waiting_for_input',
            'completed',
            'error',
            'cancelled',
          ],
          description: 'Current agent status',
        },
        message: { type: 'string', description: 'Human-readable status description' },
        progress: {
          type: 'object',
          properties: {
            current: { type: 'number' },
            total: { type: 'number' },
            label: { type: 'string' },
          },
          description: 'Progress indicator for long-running tasks',
        },
      },
      required: ['status'],
    },
  },
  {
    name: 'get_output',
    description:
      'Read captured terminal output from an agent pane. Returns line-numbered, timestamped output entries. Use this to inspect what an agent has printed — useful for monitoring, debugging, or feeding output into other tools.',
    inputSchema: {
      type: 'object',
      properties: {
        paneId: { type: 'string', description: 'The pane ID to read output from.' },
        limit: {
          type: 'number',
          description: 'Maximum number of lines to return. Defaults to 100.',
        },
        sinceLine: {
          type: 'number',
          description:
            'Only return lines with lineNum greater than this value (cursor-based pagination).',
        },
        sinceTime: {
          type: 'number',
          description: 'Only return lines with timestamp greater than this Unix ms value.',
        },
      },
      required: ['paneId'],
    },
  },
  {
    name: 'list_agent_panes',
    description:
      'List all agent panes with captured output available. Returns pane IDs, agent types, line counts, and activity timestamps.',
    inputSchema: {
      type: 'object',
      properties: {},
    },
  },
  {
    name: 'athena_forward_output',
    description:
      'Forward agent stdout/stderr output to Athena. Used by plugins to stream terminal output back to the Athena UI in batched form.',
    inputSchema: {
      type: 'object',
      properties: {
        entries: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              channel: {
                type: 'string',
                enum: ['stdout', 'stderr'],
                description: 'Output stream channel',
              },
              text: { type: 'string', description: 'Output text content' },
              timestamp: { type: 'number', description: 'Unix ms timestamp' },
            },
            required: ['channel', 'text'],
          },
          description: 'Batch of output entries to forward',
        },
        sessionId: {
          type: 'string',
          description: 'Optional session ID for correlating output to a specific agent session',
        },
      },
      required: ['entries'],
    },
  },
  {
    name: 'send_message_to_agent',
    description:
      'Send a message to another agent via the agent communications channel. Enables inter-agent coordination.',
    inputSchema: {
      type: 'object',
      properties: {
        target_agent_id: { type: 'string', description: 'The agent ID to send the message to.' },
        message: { type: 'string', description: 'The message content to send.' },
        message_type: {
          type: 'string',
          enum: ['instruction', 'query', 'result', 'notification'],
          description: 'Type of message being sent.',
        },
      },
      required: ['target_agent_id', 'message'],
    },
  },
  {
    name: 'read_agent_messages',
    description:
      'List all connected agent sessions. Use this to discover which agents are available for inter-agent communication.',
    inputSchema: {
      type: 'object',
      properties: {
        agent_id: { type: 'string', description: 'Optional agent ID to filter sessions.' },
      },
    },
  },
  {
    name: 'code_search',
    description:
      'Search the codebase for a pattern using ripgrep. Returns matching file paths, line numbers, and surrounding context. Supports regex, file type filtering, and glob patterns.',
    inputSchema: {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'The search pattern (supports regex). Required.' },
        path: { type: 'string', description: 'The directory to search in. Required.' },
        glob: {
          type: 'string',
          description: 'File glob filter (e.g., "*.ts", "*.{js,jsx}"). Optional.',
        },
        type: {
          type: 'string',
          description: 'File type filter (e.g., "ts", "py", "rust"). Optional.',
        },
        case_sensitive: {
          type: 'boolean',
          description: 'Whether the search should be case sensitive. Defaults to false.',
        },
        max_results: {
          type: 'number',
          description: 'Maximum number of results to return. Defaults to 50.',
        },
        context_lines: {
          type: 'number',
          description: 'Number of context lines around each match. Defaults to 2.',
        },
      },
      required: ['pattern', 'path'],
    },
  },
  {
    name: 'search_files',
    description:
      'Search the codebase for a pattern using ripgrep with enhanced edge-case handling. Returns structured matches with file paths, line numbers, columns, and context. Handles missing rg binary, binary files, and result truncation gracefully.',
    inputSchema: {
      type: 'object',
      properties: {
        pattern: { type: 'string', description: 'The search pattern (supports regex). Required.' },
        path: { type: 'string', description: 'The directory to search in. Required.' },
        glob: {
          type: 'string',
          description: 'File glob filter (e.g., "*.ts", "*.{js,jsx}"). Optional.',
        },
        type: {
          type: 'string',
          description: 'File type filter (e.g., "ts", "py", "rust"). Optional.',
        },
        case_sensitive: {
          type: 'boolean',
          description: 'Whether the search should be case sensitive. Defaults to false.',
        },
        max_results: {
          type: 'number',
          description: 'Maximum number of results to return. Defaults to 100, hard cap 500.',
        },
        context_lines: {
          type: 'number',
          description: 'Number of context lines around each match. Defaults to 2.',
        },
      },
      required: ['pattern', 'path'],
    },
  },
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
  ;(app as any).on('agent:stalled', ({ agentId }: { agentId: string }) => {
    broadcastNotification('notifications/message/level', {
      level: 'warning',
      message: `Agent ${agentId} has stalled (waiting for input).`,
    })
  })
  ;(app as any).on('agent:exited', ({ id, exitCode }: { id: string; exitCode: number }) => {
    broadcastNotification('notifications/message/level', {
      level: 'info',
      message: `Agent ${id} exited with code ${exitCode}.`,
    })
  })
}

async function handleRequest(socket: net.Socket, mainWindow: BrowserWindow, req: any) {
  const send = (res: any) => socket.write(JSON.stringify(res) + '\n')

  if (req.method === 'initialize') {
    if (req.params?.token !== SESSION_TOKEN) {
      send({
        jsonrpc: '2.0',
        id: req.id,
        error: { code: -32600, message: 'Invalid or missing auth token' },
      })
      socket.end()
      return
    }
    send({
      jsonrpc: '2.0',
      id: req.id,
      result: {
        protocolVersion: '2024-11-05',
        capabilities: { tools: {} },
        serverInfo: { name: 'athena-orchestrator', version: '1.0.0' },
      },
    })
    activeClients.add(socket)
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
  try {
    if (name === 'notify') {
      const { pushNotification } = await import('./services/notification-service')
      const level = args.level || 'info'
      const eventType =
        level === 'success'
          ? ('success' as const)
          : level === 'error'
            ? ('error' as const)
            : level === 'warning'
              ? ('warning' as const)
              : ('info' as const)
      pushNotification({
        type: eventType,
        title: args.title || 'Agent Notification',
        message: args.message || '',
        source: 'mcp',
        timestamp: Date.now(),
        metadata: args.metadata,
        actions: args.actions,
      })
      mainWindow.webContents.send('plugin:event', {
        id: `evt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        type: 'notification',
        source: { sessionId: 'mcp', paneId: null, agentType: 'unknown', agentId: null },
        payload: {
          level: args.level,
          message: args.message,
          title: args.title,
          metadata: args.metadata,
          actions: args.actions,
        },
        timestamp: Date.now(),
      })
      broadcastNotification('notifications/message/level', {
        level,
        message: args.message,
        title: args.title,
      })
      return { content: [{ type: 'text', text: 'Notification delivered.' }] }
    }

    if (name === 'status_update') {
      mainWindow.webContents.send('plugin:event', {
        id: `evt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        type: 'status_update',
        source: { sessionId: 'mcp', paneId: null, agentType: 'unknown', agentId: null },
        payload: {
          status: args.status,
          message: args.message,
          progress: args.progress,
          artifacts: args.artifacts,
        },
        timestamp: Date.now(),
      })

      if (args.status === 'completed') {
        const { pushNotification } = await import('./services/notification-service')
        pushNotification({
          type: 'success',
          title: 'Task Complete',
          message: args.message || 'Agent reported task completion.',
          source: 'mcp',
          timestamp: Date.now(),
        })
      }

      if (args.status === 'error') {
        const { pushNotification } = await import('./services/notification-service')
        pushNotification({
          type: 'error',
          title: 'Agent Error',
          message: args.message || 'Agent reported an error.',
          source: 'mcp',
          timestamp: Date.now(),
        })
      }

      broadcastNotification('notifications/message/level', {
        level: 'info',
        message: args.message || `Status: ${args.status}`,
      })
      return { content: [{ type: 'text', text: `Status updated to: ${args.status}` }] }
    }

    if (name === 'request_input') {
      return {
        content: [
          {
            type: 'text',
            text: 'Input request received. (Phase 2 — blocking input not yet available. Please use environment variables or config files for now.)',
          },
        ],
      }
    }

    if (name === 'control_pause' || name === 'control_resume' || name === 'control_cancel') {
      return {
        isError: true,
        content: [{ type: 'text', text: `Tool '${name}' is not yet available (Phase 2).` }],
      }
    }

    if (name === 'get_output') {
      const { getOutput } = await import('./services/output-buffer-service')
      const lines = getOutput(args.paneId, {
        limit: args.limit || 100,
        sinceLine: args.sinceLine,
        sinceTime: args.sinceTime,
      })
      if (lines.length === 0) {
        return {
          content: [
            {
              type: 'text',
              text: `No output captured for pane '${args.paneId}'. The pane may not exist or has not produced output yet.`,
            },
          ],
        }
      }
      const formatted = lines.map((l) => `[${l.lineNum}] ${l.text}`).join('\n')
      return { content: [{ type: 'text', text: formatted }] }
    }

    if (name === 'list_agent_panes') {
      const { getAgentList } = await import('./services/output-buffer-service')
      const agents = getAgentList()
      if (agents.length === 0) {
        return { content: [{ type: 'text', text: 'No agent panes with captured output.' }] }
      }
      const formatted = agents
        .map(
          (a) =>
            `${a.paneId} (${a.agentType}) — ${a.lineCount} lines, last activity: ${new Date(a.lastActivityAt).toISOString()}`,
        )
        .join('\n')
      return { content: [{ type: 'text', text: formatted }] }
    }

    const store = await getStore()
    const tasks: any[] = (store.get('tasks') as any[]) || []

    if (name === 'create_tasks') {
      const newTasks = args.tasks.map((t: any) => ({
        id: randomUUID(),
        spaceId: args.spaceId,
        title: t.title,
        description: t.description || '',
        status: 'todo',
        order: Date.now(),
        createdAt: Date.now(),
      }))
      store.set('tasks', [...tasks, ...newTasks])
      // Trigger UI sync
      mainWindow.webContents.send('store:updateTasks')
      return { content: [{ type: 'text', text: `Created ${newTasks.length} tasks.` }] }
    }

    if (name === 'get_next_task') {
      const todoTasks = tasks.filter((t) => t.status === 'todo').sort((a, b) => a.order - b.order)
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
      const task = tasks.find((t) => t.id === args.taskId)
      if (!task) return { isError: true, content: [{ type: 'text', text: 'Task not found' }] }

      task.status = args.status
      store.set('tasks', tasks)
      mainWindow.webContents.send('store:updateTasks')

      return { content: [{ type: 'text', text: 'Task updated successfully.' }] }
    }

    if (name === 'spawn_agents') {
      const shell =
        process.platform === 'win32' ? 'powershell.exe' : process.env.SHELL || '/bin/zsh'
      const instruction =
        args.instruction ||
        'You are an Athena Swarm Worker. Use the get_next_task MCP tool to pull work and update_task_status to complete it.'
      const agentCmd = `claude -p "${instruction}"`

      for (let i = 0; i < args.count; i++) {
        const ptyId = `worker-${Date.now()}-${i}`
        ptyMgr.spawnAgent(ptyId, args.cwd, shell, agentCmd, mainWindow, 'claude')
      }
      return { content: [{ type: 'text', text: `Spawned ${args.count} workers successfully.` }] }
    }

    if (name === 'athena_forward_output') {
      const entries = args.entries || []
      const sessionId = args.sessionId || ''

      mainWindow.webContents.send('plugin:event', {
        id: `evt-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        type: 'output_forwarded',
        source: { sessionId, paneId: null, agentType: 'unknown', agentId: null },
        payload: { entries, sessionId },
        timestamp: Date.now(),
      })

      return { content: [{ type: 'text', text: `Forwarded ${entries.length} output entries.` }] }
    }

    if (name === 'send_message_to_agent') {
      const { sendToAgent } = await import('./services/agent-comms')
      const sent = sendToAgent(args.target_agent_id, 'agent/message', {
        message: args.message,
        message_type: args.message_type || 'instruction',
        timestamp: Date.now(),
      })
      if (!sent) {
        return {
          isError: true,
          content: [
            { type: 'text', text: `Agent '${args.target_agent_id}' not found or disconnected.` },
          ],
        }
      }
      return {
        content: [{ type: 'text', text: `Message sent to agent '${args.target_agent_id}'.` }],
      }
    }

    if (name === 'read_agent_messages') {
      const { getAgentSessions } = await import('./services/agent-comms')
      let sessions = getAgentSessions()
      if (args.agent_id) {
        sessions = sessions.filter((s) => s.agentId === args.agent_id || s.id === args.agent_id)
      }
      if (sessions.length === 0) {
        return { content: [{ type: 'text', text: 'No agent sessions found.' }] }
      }
      const formatted = sessions
        .map(
          (s) =>
            `${s.agentId} [${s.status}] — plugin: ${s.pluginId}, connected: ${new Date(s.connectedAt).toISOString()}, last active: ${new Date(s.lastActivityAt).toISOString()}`,
        )
        .join('\n')
      return { content: [{ type: 'text', text: formatted }] }
    }

    if (name === 'code_search') {
      const { searchCode } = await import('./search')
      const result = await searchCode({
        pattern: args.pattern,
        path: args.path,
        glob: args.glob,
        type: args.type,
        caseSensitive: args.case_sensitive || false,
        maxResults: args.max_results || 50,
        contextLines: args.context_lines ?? 2,
      })

      if (result.matches.length === 0) {
        return {
          content: [
            {
              type: 'text',
              text: `No matches found for pattern "${args.pattern}" in ${args.path}.`,
            },
          ],
        }
      }

      const formatted = result.matches
        .map((m) => {
          let output = `${m.filePath}:${m.lineNumber}:${m.column}: ${m.lineText}`
          if (m.contextBefore.length > 0) {
            output =
              m.contextBefore
                .map((l, i) => `  ${m.lineNumber - m.contextBefore.length + i}: ${l}`)
                .join('\n') +
              '\n' +
              output
          }
          if (m.contextAfter.length > 0) {
            output +=
              '\n' + m.contextAfter.map((l, i) => `  ${m.lineNumber + 1 + i}: ${l}`).join('\n')
          }
          return output
        })
        .join('\n\n')

      const header = `Found ${result.stats.totalMatches} matches in ${result.stats.filesMatched} files${result.truncated ? ' (truncated)' : ''}:\n\n`

      return { content: [{ type: 'text', text: header + formatted }] }
    }

    if (name === 'search_files') {
      const { searchRipgrep } = await import('./search')
      const result = await searchRipgrep({
        pattern: args.pattern,
        path: args.path,
        glob: args.glob,
        type: args.type,
        caseSensitive: args.case_sensitive || false,
        maxResults: args.max_results || 100,
        contextLines: args.context_lines ?? 2,
      })

      if (result.error) {
        return { isError: true, content: [{ type: 'text', text: result.error }] }
      }

      if (result.matches.length === 0) {
        return {
          content: [
            {
              type: 'text',
              text: `No matches found for pattern "${args.pattern}" in ${args.path}.`,
            },
          ],
        }
      }

      const formatted = result.matches
        .map((m) => {
          let output = `${m.filePath}:${m.lineNumber}:${m.column}: ${m.lineText}`
          if (m.contextBefore.length > 0) {
            output =
              m.contextBefore
                .map((l, i) => `  ${m.lineNumber - m.contextBefore.length + i}: ${l}`)
                .join('\n') +
              '\n' +
              output
          }
          if (m.contextAfter.length > 0) {
            output +=
              '\n' + m.contextAfter.map((l, i) => `  ${m.lineNumber + 1 + i}: ${l}`).join('\n')
          }
          return output
        })
        .join('\n\n')

      const header = `Found ${result.stats.totalMatches} matches in ${result.stats.filesMatched} files${result.truncated ? ' (truncated — increase max_results for more)' : ''}:\n\n`

      return { content: [{ type: 'text', text: header + formatted }] }
    }

    return { isError: true, content: [{ type: 'text', text: 'Unknown tool.' }] }
  } catch (err: any) {
    return { isError: true, content: [{ type: 'text', text: err.message }] }
  }
}
