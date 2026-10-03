import { McpConnection } from './connection'
import type { OutputForwarderConfig, OutputEntry, OutputBatch, OutputChannel } from './types'

const DEFAULT_BATCH_INTERVAL_MS = 100
const DEFAULT_BATCH_MAX_LINES = 50
const MAX_BUFFER_SIZE = 5000

export class OutputForwarder {
  private connection: McpConnection
  private batchTimer: ReturnType<typeof setTimeout> | null = null
  private currentBatch: OutputEntry[] = []
  private reconnectBuffer: OutputEntry[] = []
  private config: Required<
    Pick<OutputForwarderConfig, 'batchIntervalMs' | 'batchMaxLines' | 'bufferOnReconnect'>
  > &
    Pick<OutputForwarderConfig, 'sessionId'>
  private started = false

  constructor(config: OutputForwarderConfig) {
    this.connection = new McpConnection(config.token, config.port, config.host)
    this.config = {
      batchIntervalMs: config.batchIntervalMs ?? DEFAULT_BATCH_INTERVAL_MS,
      batchMaxLines: config.batchMaxLines ?? DEFAULT_BATCH_MAX_LINES,
      bufferOnReconnect: config.bufferOnReconnect ?? true,
      sessionId: config.sessionId,
    }
  }

  async start(): Promise<void> {
    if (this.started) return
    await this.connection.connect()
    this.started = true
    this.scheduleFlush()

    if (this.reconnectBuffer.length > 0) {
      const batch: OutputBatch = {
        entries: this.reconnectBuffer.slice(),
        sessionId: this.config.sessionId,
      }
      try {
        await this.sendBatch(batch)
        this.reconnectBuffer = []
      } catch {
        this.reclaimEntries(batch.entries)
      }
    }
  }

  push(channel: OutputChannel, text: string): void {
    if (!text) return
    const entry: OutputEntry = {
      channel,
      text,
      timestamp: Date.now(),
      sessionId: this.config.sessionId,
    }

    if (!this.connection.isConnected() && this.config.bufferOnReconnect) {
      if (this.reconnectBuffer.length < MAX_BUFFER_SIZE) {
        this.reconnectBuffer.push(entry)
      }
      return
    }

    this.currentBatch.push(entry)
    if (this.currentBatch.length >= this.config.batchMaxLines) {
      this.flush()
    }
  }

  pushStdout(text: string): void {
    this.push('stdout', text)
  }

  pushStderr(text: string): void {
    this.push('stderr', text)
  }

  private scheduleFlush(): void {
    if (this.batchTimer) return
    this.batchTimer = setTimeout(() => {
      this.batchTimer = null
      this.flush()
      if (this.started) this.scheduleFlush()
    }, this.config.batchIntervalMs)
  }

  private flush(): void {
    if (this.currentBatch.length === 0) return
    const batch: OutputBatch = {
      entries: this.currentBatch,
      sessionId: this.config.sessionId,
    }
    this.currentBatch = []
    this.sendBatch(batch).catch(() => {
      this.reclaimEntries(batch.entries)
    })
  }

  private reclaimEntries(entries: OutputEntry[]): void {
    if (!this.config.bufferOnReconnect) return
    // Re-append only what still fits; when the buffer is full, drop.
    const room = Math.max(0, MAX_BUFFER_SIZE - this.reconnectBuffer.length)
    if (room === 0) return
    this.reconnectBuffer.push(...entries.slice(-room))
  }

  private async sendBatch(batch: OutputBatch): Promise<void> {
    if (!this.connection.isConnected()) {
      throw new Error('Cannot forward output: MCP connection is not connected')
    }
    await this.connection.callTool('athena_forward_output', {
      entries: batch.entries.map((e) => ({
        channel: e.channel,
        text: e.text,
        timestamp: e.timestamp,
      })),
      sessionId: batch.sessionId,
    })
  }

  async stop(): Promise<void> {
    this.started = false
    if (this.batchTimer) {
      clearTimeout(this.batchTimer)
      this.batchTimer = null
    }
    this.flush()
    this.connection.disconnect()
  }

  isActive(): boolean {
    return this.started && this.connection.isConnected()
  }

  getBufferSize(): number {
    return this.reconnectBuffer.length
  }
}

export function createOutputForwarder(config: OutputForwarderConfig): OutputForwarder {
  return new OutputForwarder(config)
}

export function hookStreamToForwarder(
  stream: NodeJS.ReadableStream,
  forwarder: OutputForwarder,
  channel: OutputChannel,
): () => void {
  const handler = (chunk: Buffer | string) => {
    forwarder.push(channel, chunk.toString())
  }
  stream.on('data', handler)
  return () => {
    stream.off('data', handler)
  }
}

/**
 * Wrap a WriteStream's `write()` (e.g. `process.stdout`) so output is mirrored
 * into the forwarder. Use this — not `hookStreamToForwarder` — for process
 * stdio: WriteStreams never emit `data` events.
 */
export function pipeWriteStreamToForwarder(
  stream: NodeJS.WriteStream,
  forwarder: OutputForwarder,
  channel: OutputChannel,
): () => void {
  const originalWrite = stream.write.bind(stream)
  const patched = function (chunk: unknown, ...rest: unknown[]): boolean {
    if (typeof chunk === 'string') {
      forwarder.push(channel, chunk)
    } else if (chunk instanceof Buffer || chunk instanceof Uint8Array) {
      forwarder.push(channel, chunk.toString())
    }
    return (originalWrite as (...a: unknown[]) => boolean)(chunk, ...rest)
  }
  stream.write = patched as typeof stream.write
  return () => {
    stream.write = originalWrite as typeof stream.write
  }
}
