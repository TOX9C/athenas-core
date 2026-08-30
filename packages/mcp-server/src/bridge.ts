import { WebSocket } from 'ws'
import type { Data } from 'ws'
import type {
  AgentStatus,
  AthenaNotification,
  InputRequest,
  InputResponse,
  StatusUpdate,
  ErrorReport,
  CompletionReport,
  AthenaAppState,
  AgentState,
} from './types/index.js'

export type EventHandler = (event: string, data: unknown) => void

export interface BridgeConfig {
  athenaHost: string
  athenaPort: number
  authToken?: string
}

type PendingInput = {
  resolve: (response: InputResponse) => void
  reject: (error: Error) => void
  timer: ReturnType<typeof setTimeout> | null
}

export class AthenaBridge {
  private config: BridgeConfig
  private connected = false
  private socket: WebSocket | null = null
  private pendingInputs = new Map<string, PendingInput>()
  private eventHandlers = new Set<EventHandler>()
  private agentState = new Map<string, AgentState>()
  private notificationBuffer: AthenaNotification[] = []
  /** Drop-oldest cap for notifications buffered while disconnected. */
  private static readonly MAX_BUFFERED_NOTIFICATIONS = 500
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null

  constructor(config: BridgeConfig) {
    this.config = config
  }

  async connect(): Promise<void> {
    if (this.connected) return

    return new Promise((resolve, reject) => {
      const url = `ws://${this.config.athenaHost}:${this.config.athenaPort}`
      const ws = new WebSocket(
        url,
        this.config.authToken
          ? { headers: { Authorization: `Bearer ${this.config.authToken}` } }
          : undefined,
      )

      ws.on('open', () => {
        this.socket = ws
        this.simulateOpenForTest()
        this.flushNotificationBuffer()
        resolve()
      })

      ws.on('message', (raw: Data) => {
        try {
          const msg = JSON.parse(raw.toString())
          this.handleMessage(msg)
        } catch {
          // ignore malformed messages
        }
      })

      ws.on('close', () => {
        this.connected = false
        this.socket = null
        this.scheduleReconnect()
      })

      ws.on('error', (err: Error) => {
        if (!this.connected) {
          reject(new Error(`Failed to connect to Athena at ${url}: ${err.message}`))
        }
      })
    })
  }

  async disconnect(): Promise<void> {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    if (this.socket) {
      this.socket.close()
      this.socket = null
    }
    this.connected = false

    for (const [id, pending] of this.pendingInputs) {
      pending.resolve({ value: '', cancelled: true, timedOut: false })
      this.pendingInputs.delete(id)
    }
  }

  isConnected(): boolean {
    return this.connected
  }

  onEvent(handler: EventHandler): () => void {
    this.eventHandlers.add(handler)
    return () => this.eventHandlers.delete(handler)
  }

  async sendNotification(notification: AthenaNotification): Promise<void> {
    const payload = {
      type: 'athena:notification' as const,
      data: { ...notification, timestamp: notification.timestamp ?? Date.now() },
    }

    if (this.connected && this.socket) {
      this.socket.send(JSON.stringify(payload))
    } else {
      this.notificationBuffer.push(notification)
      if (this.notificationBuffer.length > AthenaBridge.MAX_BUFFERED_NOTIFICATIONS) {
        // Drop-oldest: bounded memory during long disconnects.
        this.notificationBuffer.shift()
      }
    }

    this.emit('notification', notification)
  }

  async requestInput(request: InputRequest): Promise<InputResponse> {
    const requestId = `input-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`

    return new Promise<InputResponse>((resolve, reject) => {
      const timeout = request.timeout ?? 120_000

      const timer = setTimeout(() => {
        this.pendingInputs.delete(requestId)
        resolve({ value: '', cancelled: false, timedOut: true })
      }, timeout)

      this.pendingInputs.set(requestId, { resolve, reject, timer })

      const payload = {
        type: 'athena:requestInput' as const,
        data: {
          requestId,
          prompt: request.prompt,
          defaultResponse: request.defaultResponse ?? '',
          agentId: request.agentId ?? 'unknown',
        },
      }

      if (this.connected && this.socket) {
        this.socket.send(JSON.stringify(payload))
      } else {
        clearTimeout(timer)
        this.pendingInputs.delete(requestId)
        resolve({ value: request.defaultResponse ?? '', cancelled: true, timedOut: true })
      }
    })
  }

  async updateStatus(update: StatusUpdate): Promise<void> {
    const existing = this.agentState.get(update.agentId)
    const updated: AgentState = {
      id: update.agentId,
      type: existing?.type ?? 'unknown',
      role: existing?.role,
      status: update.status,
      cwd: existing?.cwd,
      pid: existing?.pid,
      startedAt: existing?.startedAt ?? Date.now(),
      lastActivityAt: Date.now(),
      message: update.message,
      progress: update.progress,
    }
    this.agentState.set(update.agentId, updated)

    const payload = {
      type: 'athena:statusUpdate' as const,
      data: updated,
    }

    if (this.connected && this.socket) {
      this.socket.send(JSON.stringify(payload))
    }

    this.emit('statusUpdate', updated)
  }

  async reportError(report: ErrorReport): Promise<void> {
    this.agentState.set(report.agentId, {
      ...(this.agentState.get(report.agentId) ?? {
        id: report.agentId,
        type: 'unknown',
        status: 'error' as AgentStatus,
      }),
      status: 'error',
      lastActivityAt: Date.now(),
      message: report.error,
    })

    const payload = {
      type: 'athena:error' as const,
      data: report,
    }

    if (this.connected && this.socket) {
      this.socket.send(JSON.stringify(payload))
    }

    this.emit('error', report)
  }

  async reportCompletion(report: CompletionReport): Promise<void> {
    this.agentState.set(report.agentId, {
      ...(this.agentState.get(report.agentId) ?? {
        id: report.agentId,
        type: 'unknown',
        status: 'done' as AgentStatus,
      }),
      status: 'done',
      lastActivityAt: Date.now(),
      message: report.summary,
    })

    const payload = {
      type: 'athena:completion' as const,
      data: report,
    }

    if (this.connected && this.socket) {
      this.socket.send(JSON.stringify(payload))
    }

    this.emit('completion', report)
  }

  getAgentState(agentId: string): AgentState | undefined {
    return this.agentState.get(agentId)
  }

  getAllAgentStates(): AgentState[] {
    return Array.from(this.agentState.values())
  }

  getAppState(): Partial<AthenaAppState> {
    return {
      agents: this.getAllAgentStates(),
      theme: 'void',
      activePanel: 'terminals',
    }
  }

  private handleMessage(msg: { type?: string; data?: unknown }): void {
    if (msg.type === 'athena:inputResponse' && msg.data) {
      const data = msg.data as { requestId: string; value: string; cancelled: boolean }
      const pending = this.pendingInputs.get(data.requestId)
      if (pending) {
        if (pending.timer) clearTimeout(pending.timer)
        this.pendingInputs.delete(data.requestId)
        pending.resolve({
          value: data.value,
          cancelled: data.cancelled ?? false,
          timedOut: false,
        })
      }
    }

    if (msg.type === 'athena:appState' && msg.data) {
      const state = msg.data as AthenaAppState
      for (const agent of state.agents ?? []) {
        this.agentState.set(agent.id, agent)
      }
      this.emit('appState', state)
    }
  }

  private emit(event: string, data: unknown): void {
    for (const handler of this.eventHandlers) {
      try {
        handler(event, data)
      } catch {
        // handler error
      }
    }
  }

  private flushNotificationBuffer(): void {
    while (this.notificationBuffer.length > 0) {
      const notification = this.notificationBuffer[0]
      const payload = {
        type: 'athena:notification' as const,
        data: notification,
      }
      try {
        this.socket?.send(JSON.stringify(payload))
      } catch {
        // Socket rejected the send (closing/closed): leave the entry at the
        // head of the buffer so the next flush retries it instead of losing it.
        return
      }
      this.notificationBuffer.shift()
    }
  }

  /** Next reconnect wait. Exponential with a 30 s ceiling (F15): a dead
   * host no longer generates an attempt every 5 s forever, while a healthy
   * one recovers on the first attempt after a single blip. */
  private static readonly RECONNECT_BASE_MS = 5_000
  private static readonly RECONNECT_MAX_MS = 30_000
  private reconnectDelayMs = AthenaBridge.RECONNECT_BASE_MS

  /** @internal Runs the socket-open handler body. Tests drive this instead
   * of a live WebSocketServer to verify the backoff reset. */
  simulateOpenForTest(): void {
    this.connected = true
    this.reconnectDelayMs = AthenaBridge.RECONNECT_BASE_MS
  }

  private scheduleReconnect(): void {
    if (this.reconnectTimer) return
    const delay = this.reconnectDelayMs
    this.reconnectDelayMs = Math.min(delay * 2, AthenaBridge.RECONNECT_MAX_MS)
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null
      this.connect().catch(() => {
        this.scheduleReconnect()
      })
    }, delay)
  }
}
