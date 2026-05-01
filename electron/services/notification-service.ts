import { BrowserWindow, ipcMain, Notification } from 'electron'
import { getStore } from '../storeUtil'

export type NotificationType =
  | 'info'
  | 'warning'
  | 'error'
  | 'success'
  | 'needs_input'
  | 'task_complete'
  | 'task_error'

export interface NotificationEvent {
  type: NotificationType
  title: string
  message: string
  source: string
  agentId?: string
  data?: Record<string, unknown>
  timestamp: number
  metadata?: Record<string, unknown>
  actions?: Array<{ id: string; label: string }>
  requestId?: string
}

interface NotificationRecord extends NotificationEvent {
  id: string
  read: boolean
  dismissedAt?: number
}

const MAX_HISTORY = 500
let mainWindowRef: BrowserWindow | null = null
const history: NotificationRecord[] = []

const STORE_KEY = 'notifications:history'

async function loadHistory(): Promise<void> {
  const store = await getStore()
  const saved = store.get(STORE_KEY) as NotificationRecord[] | undefined
  if (Array.isArray(saved)) {
    history.length = 0
    const trimmed = saved.slice(-MAX_HISTORY)
    history.push(...trimmed)
  }
}

async function persistHistory(): Promise<void> {
  const store = await getStore()
  const trimmed = history.slice(-MAX_HISTORY)
  store.set(STORE_KEY, trimmed)
}

function emitToRenderer(channel: string, data: unknown): void {
  mainWindowRef?.webContents.send(channel, data)
}

function showSystemNotification(event: NotificationEvent): void {
  if (!Notification.isSupported()) return

  const levelIcon: Record<NotificationType, string> = {
    info: '💡',
    warning: '⚠️',
    error: '❌',
    success: '✅',
    needs_input: '🔑',
    task_complete: '✅',
    task_error: '❌',
  }

  const n = new Notification({
    title: `${levelIcon[event.type] || ''} ${event.title}`.trim(),
    body: event.message,
    silent: event.type === 'info',
  })

  n.on('click', () => {
    mainWindowRef?.focus()
    emitToRenderer('notifications:clicked', { id: history[history.length - 1]?.id, ...event })
  })

  n.show()
}

export function pushNotification(event: NotificationEvent): NotificationRecord {
  const record: NotificationRecord = {
    id: `notif-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
    ...event,
    read: false,
  }

  history.push(record)
  if (history.length > MAX_HISTORY) {
    history.splice(0, history.length - MAX_HISTORY)
  }

  persistHistory().catch(() => {})

  emitToRenderer('notifications:new', record)

  if (event.type !== 'info') {
    showSystemNotification(event)
  }

  return record
}

export async function initNotificationService(mainWindow: BrowserWindow): Promise<void> {
  mainWindowRef = mainWindow
  await loadHistory()

  ipcMain.handle(
    'notifications:history',
    async (
      _event,
      options?: { limit?: number; unreadOnly?: boolean; type?: NotificationType; source?: string },
    ) => {
      let results = [...history]

      if (options?.unreadOnly) {
        results = results.filter((n) => !n.read)
      }

      if (options?.type) {
        results = results.filter((n) => n.type === options.type)
      }

      if (options?.source) {
        results = results.filter((n) => n.source === options.source)
      }

      const limit = options?.limit || 100
      return results.slice(-limit).reverse()
    },
  )

  ipcMain.handle('notifications:getCount', async () => {
    return {
      total: history.length,
      unread: history.filter((n) => !n.read).length,
      byType: {
        info: history.filter((n) => n.type === 'info').length,
        warning: history.filter((n) => n.type === 'warning').length,
        error: history.filter((n) => n.type === 'error').length,
        success: history.filter((n) => n.type === 'success').length,
        needs_input: history.filter((n) => n.type === 'needs_input').length,
      },
    }
  })

  ipcMain.handle('notifications:markRead', async (_event, notificationId: string) => {
    const record = history.find((n) => n.id === notificationId)
    if (!record) return { success: false, error: 'Notification not found' }

    record.read = true
    persistHistory().catch(() => {})
    emitToRenderer('notifications:updated', record)

    return { success: true }
  })

  ipcMain.handle('notifications:markAllRead', async () => {
    let count = 0
    for (const n of history) {
      if (!n.read) {
        n.read = true
        count++
      }
    }
    persistHistory().catch(() => {})
    emitToRenderer('notifications:allRead', { count })

    return { success: true, count }
  })

  ipcMain.handle('notifications:dismiss', async (_event, notificationId: string) => {
    const idx = history.findIndex((n) => n.id === notificationId)
    if (idx === -1) return { success: false, error: 'Notification not found' }

    history[idx].dismissedAt = Date.now()
    history.splice(idx, 1)
    persistHistory().catch(() => {})
    emitToRenderer('notifications:dismissed', { id: notificationId })

    return { success: true }
  })

  ipcMain.handle('notifications:clearAll', async () => {
    const count = history.length
    history.length = 0
    persistHistory().catch(() => {})
    emitToRenderer('notifications:cleared', { count })

    return { success: true, count }
  })

  ipcMain.handle('notifications:push', async (_event, event: NotificationEvent) => {
    const record = pushNotification(event)
    return { success: true, id: record.id }
  })
}

export function getNotificationHistory(): NotificationRecord[] {
  return [...history]
}

export function getUnreadCount(): number {
  return history.filter((n) => !n.read).length
}
