import { BrowserWindow, ipcMain } from 'electron'

export interface OutputLine {
  paneId: string
  lineNum: number
  timestamp: number
  text: string
}

interface PaneBuffer {
  paneId: string
  lines: OutputLine[]
  lineCounter: number
  totalBytes: number
  createdAt: number
  lastActivityAt: number
  agentType: string
  subscribers: Set<(line: OutputLine) => void>
}

const MAX_LINES_PER_PANE = 5000
const MAX_TOTAL_BYTES_PER_PANE = 2_000_000
const MAX_SUBSCRIBER_CALLBACKS = 20

let mainWindowRef: BrowserWindow | null = null
const buffers = new Map<string, PaneBuffer>()

function createBuffer(paneId: string, agentType: string = 'shell'): PaneBuffer {
  const existing = buffers.get(paneId)
  if (existing) return existing

  const buf: PaneBuffer = {
    paneId,
    lines: [],
    lineCounter: 0,
    totalBytes: 0,
    createdAt: Date.now(),
    lastActivityAt: Date.now(),
    agentType,
    subscribers: new Set(),
  }
  buffers.set(paneId, buf)
  return buf
}

function trimBuffer(buf: PaneBuffer): void {
  while (buf.lines.length > MAX_LINES_PER_PANE) {
    const removed = buf.lines.shift()
    if (removed) buf.totalBytes -= removed.text.length
  }
  while (buf.totalBytes > MAX_TOTAL_BYTES_PER_PANE && buf.lines.length > 0) {
    const removed = buf.lines.shift()
    if (removed) buf.totalBytes -= removed.text.length
  }
}

function stripAnsi(text: string): string {
  return text
    .replace(/\x1b\][^\x07]*\x07/g, '')
    .replace(/\x1b\][^\x1b]*\x1b\\/g, '')
    .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')
    .replace(/\x1b[()][0-9A-B]/g, '')
    .replace(/\x1b\[\?[0-9]+[hl]/g, '')
    .replace(/\r\n/g, '\n')
    .replace(/\r/g, '')
}

export function appendOutput(paneId: string, rawData: string, agentType?: string): void {
  let buf = buffers.get(paneId)
  if (!buf) {
    buf = createBuffer(paneId, agentType || 'shell')
  }

  buf.lastActivityAt = Date.now()
  if (agentType && buf.agentType === 'shell') {
    buf.agentType = agentType
  }

  const stripped = stripAnsi(rawData)
  const rawLines = stripped.split('\n')

  for (const rawLine of rawLines) {
    if (rawLine.length === 0 && rawLines.length > 1) continue

    buf.lineCounter++
    const line: OutputLine = {
      paneId,
      lineNum: buf.lineCounter,
      timestamp: Date.now(),
      text: rawLine,
    }
    buf.lines.push(line)
    buf.totalBytes += rawLine.length

    for (const cb of buf.subscribers) {
      try {
        cb(line)
      } catch {}
    }
  }

  trimBuffer(buf)
}

export function registerPane(paneId: string, agentType: string = 'shell'): void {
  createBuffer(paneId, agentType)
  mainWindowRef?.webContents.send('output-capture:paneRegistered', { paneId, agentType })
}

export function unregisterPane(paneId: string): void {
  const buf = buffers.get(paneId)
  if (buf) {
    buf.subscribers.clear()
  }
  buffers.delete(paneId)
  mainWindowRef?.webContents.send('output-capture:paneUnregistered', { paneId })
}

export function getOutput(
  paneId: string,
  options?: {
    limit?: number
    offset?: number
    sinceLine?: number
    sinceTime?: number
    raw?: boolean
  },
): OutputLine[] {
  const buf = buffers.get(paneId)
  if (!buf) return []

  let result = [...buf.lines]

  if (options?.sinceLine) {
    result = result.filter((l) => l.lineNum > options.sinceLine!)
  }

  if (options?.sinceTime) {
    result = result.filter((l) => l.timestamp > options.sinceTime!)
  }

  if (options?.offset) {
    result = result.slice(options.offset)
  }

  if (options?.limit) {
    result = result.slice(0, options.limit)
  }

  return result
}

export function getOutputSince(paneId: string, since: number): OutputLine[] {
  return getOutput(paneId, { sinceTime: since })
}

export function getAgentList(): Array<{
  paneId: string
  agentType: string
  lineCount: number
  createdAt: number
  lastActivityAt: number
}> {
  const result: Array<{
    paneId: string
    agentType: string
    lineCount: number
    createdAt: number
    lastActivityAt: number
  }> = []

  for (const buf of buffers.values()) {
    result.push({
      paneId: buf.paneId,
      agentType: buf.agentType,
      lineCount: buf.lines.length,
      createdAt: buf.createdAt,
      lastActivityAt: buf.lastActivityAt,
    })
  }

  return result
}

export function subscribeToPane(paneId: string, callback: (line: OutputLine) => void): () => void {
  const buf = buffers.get(paneId)
  if (!buf) return () => {}

  if (buf.subscribers.size >= MAX_SUBSCRIBER_CALLBACKS) {
    const oldest = buf.subscribers.values().next().value
    if (oldest) buf.subscribers.delete(oldest)
  }

  buf.subscribers.add(callback)
  return () => buf.subscribers.delete(callback)
}

export function getPaneBufferInfo(paneId: string): {
  paneId: string
  agentType: string
  lineCount: number
  totalLines: number
  totalBytes: number
  createdAt: number
  lastActivityAt: number
} | null {
  const buf = buffers.get(paneId)
  if (!buf) return null
  return {
    paneId: buf.paneId,
    agentType: buf.agentType,
    lineCount: buf.lines.length,
    totalLines: buf.lineCounter,
    totalBytes: buf.totalBytes,
    createdAt: buf.createdAt,
    lastActivityAt: buf.lastActivityAt,
  }
}

export function clearPaneBuffer(paneId: string): boolean {
  const buf = buffers.get(paneId)
  if (!buf) return false
  buf.lines.length = 0
  buf.totalBytes = 0
  return true
}

export async function initOutputBufferService(mainWindow: BrowserWindow): Promise<void> {
  mainWindowRef = mainWindow

  ipcMain.handle(
    'output-capture:read',
    async (
      _event,
      paneId: string,
      options?: {
        limit?: number
        offset?: number
        sinceLine?: number
        sinceTime?: number
      },
    ) => {
      return getOutput(paneId, options)
    },
  )

  ipcMain.handle('output-capture:list-agents', async () => {
    return getAgentList()
  })

  ipcMain.handle('output-capture:getInfo', async (_event, paneId: string) => {
    return getPaneBufferInfo(paneId)
  })

  ipcMain.handle('output-capture:clear', async (_event, paneId: string) => {
    return clearPaneBuffer(paneId)
  })

  ipcMain.handle('output-capture:subscribe', async (_event, paneId: string) => {
    const subscriptionId = `sub-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`

    const callback = (line: OutputLine) => {
      if (!mainWindowRef || mainWindowRef.isDestroyed()) return
      mainWindowRef.webContents.send('output-capture:line', { subscriptionId, line })
    }

    const unsubscribe = subscribeToPane(paneId, callback)

    const cleanup = () => {
      unsubscribe()
    }

    ;(ipcMain as any).once(`output-capture:unsubscribe:${subscriptionId}`, () => {
      cleanup()
    })

    return { subscriptionId }
  })

  ipcMain.on('output-capture:unsubscribe', (_event, subscriptionId: string) => {
    ;(ipcMain as any).emit(`output-capture:unsubscribe:${subscriptionId}`)
  })
}

export function shutdownOutputBufferService(): void {
  for (const buf of buffers.values()) {
    buf.subscribers.clear()
  }
  buffers.clear()
  mainWindowRef = null
}
