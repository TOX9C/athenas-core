import * as net from 'net'
import type { PluginEvent, PluginEventPayload, PluginEventType } from './types'

const RECONNECT_INTERVAL_MS = 2000
const MAX_RECONNECT_ATTEMPTS = 10
const REQUEST_TIMEOUT_MS = 30000

interface PendingRequest {
  resolve: (result: any) => void
  reject: (error: Error) => void
  timer: ReturnType<typeof setTimeout>
}

export class McpConnection {
  private socket: net.Socket | null = null
  private buffer = ''
  private requestId = 0
  private pending = new Map<string, PendingRequest>()
  private eventHandlers = new Map<PluginEventType, Array<(payload: PluginEventPayload) => void>>()
  private connected = false
  private reconnectAttempts = 0
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null
  private token: string
  private sessionId: string | null = null
  private port: number
  private host: string

  constructor(token: string, port: number = 4545, host: string = '127.0.0.1') {
    this.token = token
    this.port = port
    this.host = host
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection({ port: this.port, host: this.host }, () => {
        this.socket = socket
        this.connected = true
        this.reconnectAttempts = 0
        this.initialize().then(resolve).catch(reject)
      })

      socket.on('data', (chunk) => {
        this.buffer += chunk.toString()
        const lines = this.buffer.split('\n')
        this.buffer = lines.pop() || ''
        for (const line of lines) {
          if (!line.trim()) continue
          try {
            const msg = JSON.parse(line)
            this.handleMessage(msg)
          } catch {}
        }
      })

      socket.on('close', () => {
        this.connected = false
        this.socket = null
        this.rejectAllPending(new Error('Connection closed'))
        this.scheduleReconnect()
      })

      socket.on('error', (err) => {
        this.connected = false
        reject(err)
      })
    })
  }

  private async initialize(): Promise<void> {
    const result = await this.sendRequest('initialize', {
      token: this.token,
      protocolVersion: '2024-11-05',
      clientInfo: { name: 'athena-plugin-client', version: '1.0.0' },
    })
    this.sessionId = result.sessionId || null
    this.sendNotification('notifications/initialized', {})
  }

  private async sendRequest(method: string, params: any): Promise<any> {
    return new Promise((resolve, reject) => {
      if (!this.socket || !this.connected) {
        reject(new Error('Not connected'))
        return
      }

      const id = String(++this.requestId)
      const msg = JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n'
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`Request timed out: ${method}`))
      }, REQUEST_TIMEOUT_MS)

      this.pending.set(id, { resolve, reject, timer })
      this.socket.write(msg)
    })
  }

  private sendNotification(method: string, params: any): void {
    if (!this.socket || !this.connected) return
    const msg = JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n'
    this.socket.write(msg)
  }

  private handleMessage(msg: any): void {
    if (msg.id && this.pending.has(msg.id)) {
      const pending = this.pending.get(msg.id)!
      clearTimeout(pending.timer)
      this.pending.delete(msg.id)
      if (msg.error) {
        pending.reject(new Error(msg.error.message || 'MCP error'))
      } else {
        pending.resolve(msg.result)
      }
      return
    }

    if (msg.method && msg.method.startsWith('notifications/')) {
      const eventType = msg.params?.type as PluginEventType
      if (eventType && this.eventHandlers.has(eventType)) {
        const handlers = this.eventHandlers.get(eventType)!
        for (const handler of handlers) {
          try {
            handler(msg.params?.payload || {})
          } catch {}
        }
      }
    }
  }

  private rejectAllPending(error: Error): void {
    for (const pending of this.pending.values()) {
      clearTimeout(pending.timer)
      pending.reject(error)
    }
    this.pending.clear()
  }

  private scheduleReconnect(): void {
    if (this.reconnectAttempts >= MAX_RECONNECT_ATTEMPTS) return
    this.reconnectTimer = setTimeout(() => {
      this.reconnectAttempts++
      this.connect().catch(() => {})
    }, RECONNECT_INTERVAL_MS)
  }

  async callTool(name: string, arguments_: Record<string, unknown>): Promise<any> {
    return this.sendRequest('tools/call', { name, arguments: arguments_ })
  }

  async listTools(): Promise<any[]> {
    const result = await this.sendRequest('tools/list', {})
    return result.tools || []
  }

  onEvent(type: PluginEventType, handler: (payload: PluginEventPayload) => void): () => void {
    if (!this.eventHandlers.has(type)) {
      this.eventHandlers.set(type, [])
    }
    this.eventHandlers.get(type)!.push(handler)
    return () => {
      const handlers = this.eventHandlers.get(type)
      if (handlers) {
        const idx = handlers.indexOf(handler)
        if (idx >= 0) handlers.splice(idx, 1)
      }
    }
  }

  disconnect(): void {
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer)
      this.reconnectTimer = null
    }
    this.reconnectAttempts = MAX_RECONNECT_ATTEMPTS
    if (this.socket) {
      this.socket.end()
      this.socket = null
    }
    this.connected = false
    this.rejectAllPending(new Error('Disconnected'))
  }

  isConnected(): boolean {
    return this.connected
  }

  getSessionId(): string | null {
    return this.sessionId
  }
}

export function createMcpConnection(token: string, port?: number, host?: string): McpConnection {
  return new McpConnection(token, port, host)
}
