import type {
  OutputEntry,
  OutputReadOptions,
  OutputSinceOptions,
  OutputBufferConfig,
  StreamSubscription,
} from './types/index.js'

const DEFAULT_MAX_LINES = 10_000

export class OutputBufferManager {
  private buffers = new Map<string, OutputEntry[]>()
  private lineCounters = new Map<string, number>()
  private maxLinesPerPane: number
  private subscriptions = new Map<string, Set<StreamSubscription>>()

  constructor(config?: Partial<OutputBufferConfig>) {
    this.maxLinesPerPane = config?.maxLinesPerPane ?? DEFAULT_MAX_LINES
  }

  append(paneId: string, content: string, isStderr: boolean = false): OutputEntry {
    let buffer = this.buffers.get(paneId)
    if (!buffer) {
      buffer = []
      this.buffers.set(paneId, buffer)
      this.lineCounters.set(paneId, 0)
    }

    const lineNumber = (this.lineCounters.get(paneId) ?? 0) + 1
    this.lineCounters.set(paneId, lineNumber)

    const entry: OutputEntry = {
      timestamp: Date.now(),
      lineNumber,
      content,
      isStderr,
    }

    buffer.push(entry)

    if (buffer.length > this.maxLinesPerPane) {
      buffer.splice(0, buffer.length - this.maxLinesPerPane)
    }

    this.notifySubscribers(paneId, entry)

    return entry
  }

  read(paneId: string, options?: OutputReadOptions): OutputEntry[] {
    const buffer = this.buffers.get(paneId)
    if (!buffer) return []

    let entries = [...buffer]

    if (options?.sinceTimestamp != null) {
      entries = entries.filter((e) => e.timestamp >= options.sinceTimestamp!)
    }

    if (options?.lines != null && options.lines > 0) {
      const start = Math.max(0, entries.length - options.lines)
      entries = entries.slice(start)
    }

    return entries
  }

  readSince(paneId: string, options?: OutputSinceOptions): OutputEntry[] {
    const buffer = this.buffers.get(paneId)
    if (!buffer) return []

    let entries = [...buffer]

    if (options?.sinceTimestamp != null) {
      entries = entries.filter((e) => e.timestamp > options.sinceTimestamp!)
    }

    if (options?.sinceLine != null) {
      entries = entries.filter((e) => e.lineNumber > options.sinceLine!)
    }

    return entries
  }

  clear(paneId: string): void {
    this.buffers.delete(paneId)
    this.lineCounters.delete(paneId)
  }

  getPaneIds(): string[] {
    return Array.from(this.buffers.keys())
  }

  getLineCount(paneId: string): number {
    return this.lineCounters.get(paneId) ?? 0
  }

  getBufferLength(paneId: string): number {
    return this.buffers.get(paneId)?.length ?? 0
  }

  subscribe(paneId: string, onChunk: (entry: OutputEntry) => void): StreamSubscription {
    const sub: StreamSubscription = {
      id: `sub-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
      paneId,
      onChunk,
      active: true,
    }

    let subs = this.subscriptions.get(paneId)
    if (!subs) {
      subs = new Set()
      this.subscriptions.set(paneId, subs)
    }
    subs.add(sub)

    return sub
  }

  unsubscribe(subscription: StreamSubscription): void {
    subscription.active = false
    const subs = this.subscriptions.get(subscription.paneId)
    if (subs) {
      subs.delete(subscription)
      if (subs.size === 0) {
        this.subscriptions.delete(subscription.paneId)
      }
    }
  }

  getActiveSubscriptions(paneId: string): number {
    return this.subscriptions.get(paneId)?.size ?? 0
  }

  private notifySubscribers(paneId: string, entry: OutputEntry): void {
    const subs = this.subscriptions.get(paneId)
    if (!subs) return

    for (const sub of subs) {
      if (sub.active) {
        try {
          sub.onChunk(entry)
        } catch {
          // subscriber error — skip
        }
      }
    }
  }
}
