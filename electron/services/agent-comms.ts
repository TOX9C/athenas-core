import * as net from 'net'
import { BrowserWindow, ipcMain } from 'electron'
import { randomUUID } from 'crypto'
import {
  initNotificationService,
  pushNotification,
  type NotificationEvent,
} from './notification-service'
import { getPluginById, setPluginStatus } from './plugin-manager'

export type AgentMessageType =
  | 'notification'
  | 'status_update'
  | 'input_request'
  | 'error'
  | 'completion'
  | 'heartbeat'
  | 'register'

export interface AgentMessage {
  jsonrpc: '2.0'
  id?: string
  method: string
  params: {
    pluginId?: string
    agentId?: string
    type?: AgentMessageType
    level?: 'info' | 'warning' | 'error' | 'needs_input' | 'task_complete'
    title?: string
    message?: string
    data?: Record<string, unknown>
    status?: string
    prompt?: string
    requestId?: string
  }
}

export interface AgentSession {
  id: string
  pluginId: string
  agentId: string
  socket: net.Socket
  connectedAt: number
  lastActivityAt: number
  status: 'active' | 'idle' | 'waiting_input' | 'disconnected'
}

const SESSION_TOKEN = randomUUID()
const activeSockets = new Set<net.Socket>()
const sessions = new Map<string, AgentSession>()
const pendingInputRequests = new Map<
  string,
  { resolve: (value: string) => void; reject: (reason: Error) => void }
>()

let mainWindowRef: BrowserWindow | null = null
let serverInstance: net.Server | null = null

export function getCommsToken(): string {
  return SESSION_TOKEN
}

export function getAgentSessions(): Omit<AgentSession, 'socket'>[] {
  return Array.from(sessions.values()).map((s) => ({
    id: s.id,
    pluginId: s.pluginId,
    agentId: s.agentId,
    connectedAt: s.connectedAt,
    lastActivityAt: s.lastActivityAt,
    status: s.status,
  }))
}

function emitToRenderer(channel: string, data: unknown): void {
  mainWindowRef?.webContents.send(channel, data)
}

function sendToSocket(socket: net.Socket, payload: object): void {
  if (socket.destroyed) return
  socket.write(JSON.stringify({ jsonrpc: '2.0', ...payload }) + '\n')
}

function findSessionByAgentId(agentId: string): AgentSession | undefined {
  for (const session of sessions.values()) {
    if (session.agentId === agentId) return session
  }
  return undefined
}

async function handleIncomingMessage(socket: net.Socket, msg: AgentMessage): Promise<void> {
  const { method, params } = msg

  if (method === 'initialize') {
    if (params?.data?.['token'] !== SESSION_TOKEN) {
      sendToSocket(socket, {
        id: msg.id,
        error: { code: -32600, message: 'Invalid or missing auth token' },
      })
      socket.end()
      return
    }

    const sessionId = randomUUID()
    const pluginId = (params?.data?.['pluginId'] as string) || 'unknown'
    const agentId = (params?.data?.['agentId'] as string) || `agent-${sessionId.slice(0, 8)}`

    const session: AgentSession = {
      id: sessionId,
      pluginId,
      agentId,
      socket,
      connectedAt: Date.now(),
      lastActivityAt: Date.now(),
      status: 'active',
    }

    sessions.set(sessionId, session)
    activeSockets.add(socket)

    sendToSocket(socket, {
      id: msg.id,
      result: {
        sessionId,
        agentId,
        protocolVersion: '1.0.0',
        capabilities: ['notification', 'status_update', 'input_request', 'error', 'completion'],
      },
    })

    emitToRenderer('agents:connected', {
      sessionId,
      pluginId,
      agentId,
      connectedAt: session.connectedAt,
    })

    const plugin = getPluginById(pluginId)
    if (plugin) {
      pushNotification({
        type: 'info',
        title: 'Agent Connected',
        message: `Agent "${agentId}" from plugin "${plugin.manifest.name}" has connected.`,
        source: pluginId,
        agentId,
        timestamp: Date.now(),
      })
    }
    return
  }

  if (method === 'notifications/message') {
    const agentId = params?.agentId
    const session = agentId ? findSessionByAgentId(agentId) : findSessionBySocket(socket)
    if (session) session.lastActivityAt = Date.now()

    const notifType = (params?.level as NotificationEvent['type']) || 'info'
    const notif: NotificationEvent = {
      type: notifType,
      title: params?.title || 'Agent Notification',
      message: params?.message || '',
      source: session?.pluginId || 'unknown',
      agentId: session?.agentId,
      data: params?.data,
      timestamp: Date.now(),
    }

    pushNotification(notif)

    if (session) {
      session.status = notifType === 'needs_input' ? 'waiting_input' : 'active'
      emitToRenderer('agents:statusUpdate', {
        sessionId: session.id,
        agentId: session.agentId,
        status: session.status,
        data: params?.data,
      })
    }

    if (msg.id) {
      sendToSocket(socket, { id: msg.id, result: { acknowledged: true } })
    }
    return
  }

  if (method === 'agents/status') {
    const session = findSessionBySocket(socket)
    if (session) {
      session.lastActivityAt = Date.now()
      session.status = (params?.status as AgentSession['status']) || 'active'

      emitToRenderer('agents:statusUpdate', {
        sessionId: session.id,
        agentId: session.agentId,
        status: session.status,
        data: params?.data,
      })

      if (params?.status === 'waiting_input' && params?.prompt) {
        pushNotification({
          type: 'needs_input',
          title: 'Input Required',
          message: params.prompt,
          source: session.pluginId,
          agentId: session.agentId,
          data: { requestId: params.requestId },
          timestamp: Date.now(),
        })
      }
    }

    if (msg.id) {
      sendToSocket(socket, { id: msg.id, result: { acknowledged: true } })
    }
    return
  }

  if (method === 'agents/requestInput') {
    const session = findSessionBySocket(socket)
    if (!session) {
      sendToSocket(socket, { id: msg.id, error: { code: -32000, message: 'Not initialized' } })
      return
    }

    session.lastActivityAt = Date.now()
    session.status = 'waiting_input'

    const requestId = params?.requestId || randomUUID()

    pushNotification({
      type: 'needs_input',
      title: params?.title || 'Input Request',
      message: params?.prompt || 'Agent requires input.',
      source: session.pluginId,
      agentId: session.agentId,
      data: { requestId },
      timestamp: Date.now(),
    })

    emitToRenderer('agents:inputRequested', {
      sessionId: session.id,
      agentId: session.agentId,
      requestId,
      prompt: params?.prompt || '',
    })

    if (msg.id) {
      new Promise<string>((resolve, reject) => {
        pendingInputRequests.set(requestId, { resolve, reject })
      })
        .then((response) => {
          sendToSocket(socket, { id: msg.id, result: { input: response } })
          session.status = 'active'
          session.lastActivityAt = Date.now()
          emitToRenderer('agents:statusUpdate', {
            sessionId: session.id,
            agentId: session.agentId,
            status: 'active',
          })
        })
        .catch((err) => {
          sendToSocket(socket, { id: msg.id, error: { code: -32000, message: err.message } })
        })
    }
    return
  }

  if (method === 'agents/heartbeat') {
    const session = findSessionBySocket(socket)
    if (session) {
      session.lastActivityAt = Date.now()
    }
    if (msg.id) {
      sendToSocket(socket, { id: msg.id, result: { ts: Date.now() } })
    }
    return
  }

  if (msg.id) {
    sendToSocket(socket, {
      id: msg.id,
      error: { code: -32601, message: `Method not found: ${method}` },
    })
  }
}

function findSessionBySocket(socket: net.Socket): AgentSession | undefined {
  for (const session of sessions.values()) {
    if (session.socket === socket) return session
  }
  return undefined
}

function cleanupSession(socket: net.Socket): void {
  const session = findSessionBySocket(socket)
  if (session) {
    session.status = 'disconnected'
    emitToRenderer('agents:disconnected', {
      sessionId: session.id,
      agentId: session.agentId,
      pluginId: session.pluginId,
    })

    pushNotification({
      type: 'info',
      title: 'Agent Disconnected',
      message: `Agent "${session.agentId}" has disconnected.`,
      source: session.pluginId,
      agentId: session.agentId,
      timestamp: Date.now(),
    })

    sessions.delete(session.id)
  }
  activeSockets.delete(socket)
}

export async function initAgentComms(
  mainWindow: BrowserWindow,
  port: number = 4546,
): Promise<void> {
  mainWindowRef = mainWindow

  await initNotificationService(mainWindow)

  serverInstance = net.createServer((socket) => {
    let buffer = ''

    socket.on('data', (chunk) => {
      buffer += chunk.toString()
      const lines = buffer.split('\n')
      buffer = lines.pop() || ''

      for (const line of lines) {
        if (!line.trim()) continue
        try {
          const msg = JSON.parse(line) as AgentMessage
          handleIncomingMessage(socket, msg).catch(() => {})
        } catch {
          // parse error
        }
      }
    })

    socket.on('close', () => cleanupSession(socket))
    socket.on('error', () => cleanupSession(socket))
  })

  serverInstance.listen(port, '127.0.0.1')

  // IPC: Get all agent sessions
  ipcMain.handle('agents:list', async () => {
    return getAgentSessions()
  })

  // IPC: Get specific agent status
  ipcMain.handle('agents:getStatus', async (_event, agentId: string) => {
    const session = findSessionByAgentId(agentId)
    if (!session) return null
    return {
      id: session.id,
      pluginId: session.pluginId,
      agentId: session.agentId,
      status: session.status,
      connectedAt: session.connectedAt,
      lastActivityAt: session.lastActivityAt,
    }
  })

  // IPC: Respond to an input request
  ipcMain.handle('agents:respondInput', async (_event, requestId: string, response: string) => {
    const pending = pendingInputRequests.get(requestId)
    if (!pending) return { success: false, error: 'Request not found or already resolved' }

    pending.resolve(response)
    pendingInputRequests.delete(requestId)
    return { success: true }
  })

  // IPC: Cancel an input request
  ipcMain.handle('agents:cancelInput', async (_event, requestId: string) => {
    const pending = pendingInputRequests.get(requestId)
    if (!pending) return { success: false, error: 'Request not found' }

    pending.reject(new Error('Input request cancelled by user'))
    pendingInputRequests.delete(requestId)
    return { success: true }
  })

  // IPC: Send message directly to an agent
  ipcMain.handle(
    'agents:sendMessage',
    async (_event, agentId: string, method: string, params: Record<string, unknown>) => {
      const session = findSessionByAgentId(agentId)
      if (!session) return { success: false, error: 'Agent not found' }

      const id = randomUUID()
      sendToSocket(session.socket, { id, method, params })
      return { success: true, messageId: id }
    },
  )

  // IPC: Disconnect an agent
  ipcMain.handle('agents:disconnect', async (_event, agentId: string) => {
    const session = findSessionByAgentId(agentId)
    if (!session) return { success: false, error: 'Agent not found' }

    session.socket.end()
    cleanupSession(session.socket)
    return { success: true }
  })

  // IPC: Get comms token (for spawning agents that need to connect back)
  ipcMain.handle('agents:getToken', async () => {
    return SESSION_TOKEN
  })

  // IPC: Get comms port
  ipcMain.handle('agents:getPort', async () => {
    return port
  })

  // Stalled agent detection (similar to swarm coordinator)
  const STALL_TIMEOUT = 90_000
  const CHECK_INTERVAL = 15_000

  setInterval(() => {
    const now = Date.now()
    for (const session of sessions.values()) {
      if (
        session.status !== 'disconnected' &&
        session.status !== 'waiting_input' &&
        now - session.lastActivityAt > STALL_TIMEOUT
      ) {
        session.status = 'idle'
        emitToRenderer('agents:statusUpdate', {
          sessionId: session.id,
          agentId: session.agentId,
          status: 'idle',
          data: { reason: 'stalled', lastActivityAt: session.lastActivityAt },
        })
      }
    }
  }, CHECK_INTERVAL)
}

export function broadcastToAgents(method: string, params: Record<string, unknown>): void {
  const payload = JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n'
  for (const socket of activeSockets) {
    if (!socket.destroyed) {
      socket.write(payload)
    }
  }
}

export function respondToInputRequest(requestId: string, response: string): boolean {
  const pending = pendingInputRequests.get(requestId)
  if (!pending) return false
  pending.resolve(response)
  pendingInputRequests.delete(requestId)
  return true
}

export function sendToAgent(
  agentId: string,
  method: string,
  params: Record<string, unknown>,
): boolean {
  const session = findSessionByAgentId(agentId)
  if (!session || session.socket.destroyed) return false
  sendToSocket(session.socket, { method, params })
  return true
}

export async function shutdownAgentComms(): Promise<void> {
  for (const socket of activeSockets) {
    try {
      socket.end()
    } catch {}
  }
  activeSockets.clear()
  sessions.clear()

  for (const pending of pendingInputRequests.values()) {
    pending.reject(new Error('Server shutting down'))
  }
  pendingInputRequests.clear()

  if (serverInstance) {
    await new Promise<void>((resolve) => serverInstance!.close(() => resolve()))
    serverInstance = null
  }
}
