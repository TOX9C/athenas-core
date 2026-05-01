import { BrowserWindow, ipcMain } from 'electron'
import { randomUUID } from 'crypto'
import type {
  PluginEvent,
  PluginEventType,
  PluginCapability,
  PluginEventPayload,
} from '../src/types/plugin'
import type { AgentType } from '../src/types/workspace'
import { DEFAULT_CAPABILITIES } from '../src/types/plugin'
import { pushNotification } from './services/notification-service'
import { getPluginById, setPluginStatus } from './services/plugin-manager'

interface PluginSession {
  id: string
  pluginId: string
  agentType: AgentType
  agentId: string | null
  paneId: string | null
  capabilities: PluginCapability[]
  connectedAt: number
  lastActivityAt: number
  status: 'active' | 'idle' | 'waiting_input' | 'disconnected'
}

const sessions = new Map<string, PluginSession>()
const eventSubscriptions = new Map<PluginEventType, Set<string>>()
let mainWindowRef: BrowserWindow | null = null

function emitToRenderer(channel: string, data: unknown): void {
  mainWindowRef?.webContents.send(channel, data)
}

function scopedCapabilities(agentType: AgentType, requested?: string[]): PluginCapability[] {
  const allowed = DEFAULT_CAPABILITIES[agentType] || ['notifications', 'status']
  if (!requested) return allowed
  return requested.filter((c): c is PluginCapability => allowed.includes(c as PluginCapability))
}

export function registerSession(params: {
  pluginId: string
  agentType: AgentType
  agentId?: string
  paneId?: string
  requestedCapabilities?: string[]
}): PluginSession {
  const id = randomUUID()
  const capabilities = scopedCapabilities(params.agentType, params.requestedCapabilities)

  const session: PluginSession = {
    id,
    pluginId: params.pluginId,
    agentType: params.agentType,
    agentId: params.agentId || `agent-${id.slice(0, 8)}`,
    paneId: params.paneId || null,
    capabilities,
    connectedAt: Date.now(),
    lastActivityAt: Date.now(),
    status: 'active',
  }

  sessions.set(id, session)
  emitToRenderer('pluginHost:sessionRegistered', sanitizeSession(session))
  return session
}

export function getSession(id: string): PluginSession | undefined {
  return sessions.get(id)
}

export function getSessionByAgentId(agentId: string): PluginSession | undefined {
  for (const session of sessions.values()) {
    if (session.agentId === agentId) return session
  }
  return undefined
}

export function removeSession(id: string): void {
  const session = sessions.get(id)
  if (!session) return
  session.status = 'disconnected'
  emitToRenderer('pluginHost:sessionRemoved', { id, agentId: session.agentId })
  sessions.delete(id)
}

export function emitPluginEvent(event: Omit<PluginEvent, 'id' | 'timestamp'>): PluginEvent {
  const full: PluginEvent = {
    id: `evt-${randomUUID().slice(0, 12)}`,
    ...event,
    timestamp: Date.now(),
  }

  emitToRenderer('plugin:event', full)

  if (event.type === 'notification' || event.type === 'needs_input') {
    pushNotification({
      type:
        event.payload.level === 'error'
          ? 'error'
          : event.payload.level === 'warning'
            ? 'warning'
            : 'info',
      title: event.payload.title || 'Plugin Event',
      message: event.payload.message || '',
      source: event.source.agentId || 'plugin',
      agentId: event.source.agentId || undefined,
      data: event.payload.metadata,
      timestamp: Date.now(),
    })
  }

  if (event.type === 'task_complete') {
    pushNotification({
      type: 'task_complete',
      title: event.payload.taskTitle || 'Task Complete',
      message: event.payload.result || '',
      source: event.source.agentId || 'plugin',
      agentId: event.source.agentId || undefined,
      timestamp: Date.now(),
    })
  }

  if (event.type === 'task_error') {
    pushNotification({
      type: 'error',
      title: event.payload.taskTitle || 'Task Error',
      message: event.payload.error || 'Unknown error',
      source: event.source.agentId || 'plugin',
      agentId: event.source.agentId || undefined,
      timestamp: Date.now(),
    })
  }

  return full
}

export function subscribeSession(sessionId: string, eventTypes: PluginEventType[]): void {
  for (const type of eventTypes) {
    if (!eventSubscriptions.has(type)) {
      eventSubscriptions.set(type, new Set())
    }
    eventSubscriptions.get(type)!.add(sessionId)
  }
}

export function getSubscribers(eventType: PluginEventType): PluginSession[] {
  const ids = eventSubscriptions.get(eventType)
  if (!ids) return []
  const result: PluginSession[] = []
  for (const id of ids) {
    const session = sessions.get(id)
    if (session && session.status !== 'disconnected') {
      result.push(session)
    }
  }
  return result
}

export function updateSessionStatus(
  sessionId: string,
  status: PluginSession['status'],
  data?: Record<string, unknown>,
): void {
  const session = sessions.get(sessionId)
  if (!session) return
  session.status = status
  session.lastActivityAt = Date.now()
  emitToRenderer('pluginHost:sessionStatusUpdate', {
    sessionId,
    agentId: session.agentId,
    status,
    data,
  })
}

function sanitizeSession(session: PluginSession): any {
  return {
    id: session.id,
    pluginId: session.pluginId,
    agentType: session.agentType,
    agentId: session.agentId,
    paneId: session.paneId,
    capabilities: session.capabilities,
    connectedAt: session.connectedAt,
    lastActivityAt: session.lastActivityAt,
    status: session.status,
  }
}

export async function initPluginHost(mainWindow: BrowserWindow): Promise<void> {
  mainWindowRef = mainWindow

  ipcMain.handle('pluginHost:listSessions', async () => {
    return Array.from(sessions.values()).map(sanitizeSession)
  })

  ipcMain.handle('pluginHost:getSession', async (_event, sessionId: string) => {
    const session = sessions.get(sessionId)
    return session ? sanitizeSession(session) : null
  })

  ipcMain.handle(
    'pluginHost:emitEvent',
    async (_event, event: Omit<PluginEvent, 'id' | 'timestamp'>) => {
      return emitPluginEvent(event)
    },
  )

  ipcMain.handle(
    'pluginHost:subscribe',
    async (_event, sessionId: string, eventTypes: PluginEventType[]) => {
      subscribeSession(sessionId, eventTypes)
      return { success: true }
    },
  )

  ipcMain.handle(
    'pluginHost:updateStatus',
    async (_event, sessionId: string, status: string, data?: Record<string, unknown>) => {
      updateSessionStatus(sessionId, status as PluginSession['status'], data)
      return { success: true }
    },
  )

  ipcMain.handle('pluginHost:unregisterSession', async (_event, sessionId: string) => {
    removeSession(sessionId)
    return { success: true }
  })

  ipcMain.handle('pluginHost:discoverPlugins', async (_event, projectRoot?: string) => {
    const { discoverAll } = await import('../plugins/shared/setup')
    return discoverAll(projectRoot)
  })

  ipcMain.handle(
    'pluginHost:setupPlugin',
    async (
      _event,
      agentType: 'opencode' | 'claude-code',
      options: { token: string; projectRoot?: string; global?: boolean },
    ) => {
      const { setupOpenCode, setupClaudeCode } = await import('../plugins/shared/setup')
      const { getMcpToken } = await import('./mcpServer')
      const token = options.token || getMcpToken()
      if (agentType === 'opencode') {
        return setupOpenCode({ token, projectRoot: options.projectRoot, global: options.global })
      }
      return setupClaudeCode({ token, projectRoot: options.projectRoot, global: options.global })
    },
  )

  ipcMain.handle(
    'pluginHost:removePlugin',
    async (_event, agentType: 'opencode' | 'claude-code', projectRoot?: string) => {
      const { removeMcpEntry } = await import('../plugins/shared/setup')
      return removeMcpEntry(agentType, projectRoot)
    },
  )

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
        emitToRenderer('pluginHost:sessionStatusUpdate', {
          sessionId: session.id,
          agentId: session.agentId,
          status: 'idle',
          data: { reason: 'stalled' },
        })
      }
    }
  }, CHECK_INTERVAL)
}

export function getPluginHostSessions(): PluginSession[] {
  return Array.from(sessions.values())
}
