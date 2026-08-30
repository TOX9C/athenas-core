import { WebSocketServer, WebSocket } from 'ws'
import { randomUUID } from 'node:crypto'

interface WsSession {
  id: string
  socket: WebSocket
  alive: boolean
}

export class WebSocketTransport {
  private wss: WebSocketServer
  private sessions = new Map<string, WsSession>()
  private messageHandler: ((message: unknown, sessionId: string) => void) | null = null
  private closeHandler: ((sessionId: string) => void) | null = null
  private pingInterval: ReturnType<typeof setInterval> | null = null

  constructor(port: number = 4546, host: string = '127.0.0.1') {
    this.wss = new WebSocketServer({ port, host })
  }

  onMessage(handler: (message: unknown, sessionId: string) => void): void {
    this.messageHandler = handler
  }

  onClose(handler: (sessionId: string) => void): void {
    this.closeHandler = handler
  }

  start(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.wss.on('error', (err) => {
        reject(new Error(`WebSocket server failed to start: ${err.message}`))
      })

      this.wss.on('listening', () => {
        this.pingInterval = setInterval(() => {
          for (const [id, session] of this.sessions) {
            if (!session.alive) {
              session.socket.terminate()
              this.sessions.delete(id)
              this.closeHandler?.(id)
            } else {
              session.alive = false
              session.socket.ping()
            }
          }
        }, 30_000)

        resolve()
      })

      this.wss.on('connection', (ws, _req) => {
        const sessionId = randomUUID()
        const session: WsSession = { id: sessionId, socket: ws, alive: true }
        this.sessions.set(sessionId, session)

        ws.on('pong', () => {
          if (this.sessions.has(sessionId)) {
            this.sessions.get(sessionId)!.alive = true
          }
        })

        ws.on('message', (raw) => {
          try {
            const message = JSON.parse(raw.toString())
            this.messageHandler?.(message, sessionId)
          } catch {
            // ignore malformed messages
          }
        })

        ws.on('close', () => {
          this.sessions.delete(sessionId)
          this.closeHandler?.(sessionId)
        })

        ws.on('error', () => {
          this.sessions.delete(sessionId)
          this.closeHandler?.(sessionId)
        })
      })
    })
  }

  send(sessionId: string, message: unknown): boolean {
    const session = this.sessions.get(sessionId)
    if (!session || session.socket.readyState !== WebSocket.OPEN) {
      return false
    }
    session.socket.send(JSON.stringify(message))
    return true
  }

  broadcast(message: unknown): void {
    const payload = JSON.stringify(message)
    for (const session of this.sessions.values()) {
      if (session.socket.readyState === WebSocket.OPEN) {
        session.socket.send(payload)
      }
    }
  }

  getSessionCount(): number {
    return this.sessions.size
  }

  async stop(): Promise<void> {
    if (this.pingInterval) {
      clearInterval(this.pingInterval)
      this.pingInterval = null
    }

    for (const session of this.sessions.values()) {
      session.socket.close(1001, 'Server shutting down')
    }
    this.sessions.clear()

    return new Promise((resolve) => {
      this.wss.close(() => resolve())
    })
  }
}
