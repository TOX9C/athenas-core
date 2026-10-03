import * as net from 'net'
import { randomUUID } from 'node:crypto'

interface TcpSession {
  id: string
  socket: net.Socket
  alive: boolean
  lastActivityAt: number
}

export class TcpTransport {
  private server: net.Server | null = null
  private sessions = new Map<string, TcpSession>()
  private messageHandler: ((message: unknown, sessionId: string) => void) | null = null
  private closeHandler: ((sessionId: string) => void) | null = null
  private port: number
  private host: string
  private staleCheckInterval: ReturnType<typeof setInterval> | null = null

  constructor(port: number = 4545, host: string = '127.0.0.1') {
    this.port = port
    this.host = host
  }

  onMessage(handler: (message: unknown, sessionId: string) => void): void {
    this.messageHandler = handler
  }

  onClose(handler: (sessionId: string) => void): void {
    this.closeHandler = handler
  }

  start(): Promise<void> {
    return new Promise((resolve, reject) => {
      this.server = net.createServer((socket) => {
        const sessionId = randomUUID()
        const session: TcpSession = {
          id: sessionId,
          socket,
          alive: true,
          lastActivityAt: Date.now(),
        }
        this.sessions.set(sessionId, session)

        let buffer = ''
        socket.on('data', (chunk) => {
          buffer += chunk.toString()
          const lines = buffer.split('\n')
          buffer = lines.pop() || ''

          for (const line of lines) {
            if (!line.trim()) continue
            try {
              const message = JSON.parse(line)
              session.lastActivityAt = Date.now()
              this.messageHandler?.(message, sessionId)
            } catch (err) {
              console.debug(
                `[mcp-server] tcp session ${sessionId}: ignoring malformed JSON message`,
                err,
              )
            }
          }
        })

        socket.on('close', () => {
          this.sessions.delete(sessionId)
          this.closeHandler?.(sessionId)
        })

        socket.on('error', () => {
          this.sessions.delete(sessionId)
          this.closeHandler?.(sessionId)
        })
      })

      this.server.on('error', (err) => {
        reject(new Error(`TCP server failed to start on ${this.host}:${this.port}: ${err.message}`))
      })

      this.server.listen(this.port, this.host, () => {
        this.staleCheckInterval = setInterval(() => {
          const now = Date.now()
          for (const [id, session] of this.sessions) {
            if (now - session.lastActivityAt > 300_000) {
              session.socket.destroy()
              this.sessions.delete(id)
              this.closeHandler?.(id)
            }
          }
        }, 60_000)

        resolve()
      })
    })
  }

  send(sessionId: string, message: unknown): boolean {
    const session = this.sessions.get(sessionId)
    if (!session || session.socket.destroyed) {
      return false
    }
    // Outbound traffic is activity too: long-running tool calls (e.g.
    // request_input blocked up to 600s) must not be reaped while their
    // response is still being produced.
    session.lastActivityAt = Date.now()
    session.socket.write(JSON.stringify(message) + '\n')
    return true
  }

  broadcast(message: unknown): void {
    const payload = JSON.stringify(message) + '\n'
    for (const session of this.sessions.values()) {
      if (!session.socket.destroyed) {
        session.lastActivityAt = Date.now()
        session.socket.write(payload)
      }
    }
  }

  /** Terminate a single session (e.g. rejected auth handshake). */
  close(sessionId: string): void {
    const session = this.sessions.get(sessionId)
    if (!session) return
    session.socket.destroy()
    this.sessions.delete(sessionId)
  }

  getSessionCount(): number {
    return this.sessions.size
  }

  getSessionIds(): string[] {
    return Array.from(this.sessions.keys())
  }

  async stop(): Promise<void> {
    if (this.staleCheckInterval) {
      clearInterval(this.staleCheckInterval)
      this.staleCheckInterval = null
    }

    for (const session of this.sessions.values()) {
      session.socket.destroy()
    }
    this.sessions.clear()

    return new Promise((resolve) => {
      if (this.server) {
        this.server.close(() => resolve())
      } else {
        resolve()
      }
    })
  }
}
