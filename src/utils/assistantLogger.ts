export type LogLevel = 'debug' | 'info' | 'warn' | 'error' | 'critical'
export type AssistantAction =
  | 'chat_send'
  | 'chat_receive'
  | 'chat_error'
  | 'chat_retry'
  | 'panel_open'
  | 'panel_close'
  | 'agent_spawn'
  | 'agent_close'
  | 'agent_command'
  | 'agent_exit'
  | 'circuit_open'
  | 'circuit_close'
  | 'circuit_half_open'
  | 'health_check'
  | 'health_degraded'
  | 'health_recovery'
  | 'state_reset'
  | 'provider_call'
  | 'provider_error'
  | 'provider_timeout'
  | 'tool_execute'
  | 'tool_error'
  | 'tool_success'
  | 'session_start'
  | 'session_end'
  | 'recovery_attempt'
  | 'recovery_success'
  | 'recovery_failure'

export interface LogEntry {
  id: string
  timestamp: number
  level: LogLevel
  action: AssistantAction
  message: string
  correlationId?: string
  provider?: string
  durationMs?: number
  error?: {
    name: string
    message: string
    code?: string
    stack?: string
  }
  meta?: Record<string, unknown>
}

const LOG_LEVEL_PRIORITY: Record<LogLevel, number> = {
  debug: 0,
  info: 1,
  warn: 2,
  error: 3,
  critical: 4,
}

const MAX_LOG_ENTRIES = 500
const MIN_LEVEL: LogLevel = 'debug'

let correlationCounter = 0

function generateId(): string {
  return `log-${Date.now()}-${++correlationCounter}`
}

export function createCorrelationId(): string {
  return `corr-${Date.now()}-${++correlationCounter}`
}

function isErrorLike(
  value: unknown,
): value is { name?: string; message?: string; code?: string; stack?: string } {
  return typeof value === 'object' && value !== null && ('message' in value || 'stack' in value)
}

function serializeError(err: unknown): LogEntry['error'] {
  if (!isErrorLike(err)) {
    return { name: 'UnknownError', message: String(err) }
  }
  return {
    name: err.name || 'Error',
    message: err.message || 'Unknown error',
    code: (err as any).code ? String((err as any).code) : undefined,
    stack: err.stack,
  }
}

type LogListener = (entry: LogEntry) => void

const listeners = new Set<LogListener>()

export function onLog(listener: LogListener): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

const entries: LogEntry[] = []

function addEntry(entry: LogEntry): void {
  entries.push(entry)
  if (entries.length > MAX_LOG_ENTRIES) {
    entries.splice(0, entries.length - MAX_LOG_ENTRIES)
  }
  for (const listener of listeners) {
    try {
      listener(entry)
    } catch {}
  }
  if (entry.level === 'error' || entry.level === 'critical') {
    console.error(`[Athena:${entry.action}] ${entry.message}`, entry.error || '')
  } else if (entry.level === 'warn') {
    console.warn(`[Athena:${entry.action}] ${entry.message}`)
  }
}

function log(
  level: LogLevel,
  action: AssistantAction,
  message: string,
  opts?: {
    correlationId?: string
    provider?: string
    durationMs?: number
    error?: unknown
    meta?: Record<string, unknown>
  },
): LogEntry {
  if (LOG_LEVEL_PRIORITY[level] < LOG_LEVEL_PRIORITY[MIN_LEVEL]) {
    const entry: LogEntry = { id: generateId(), timestamp: Date.now(), level, action, message }
    return entry
  }

  const entry: LogEntry = {
    id: generateId(),
    timestamp: Date.now(),
    level,
    action,
    message,
    correlationId: opts?.correlationId,
    provider: opts?.provider,
    durationMs: opts?.durationMs,
    error: opts?.error ? serializeError(opts.error) : undefined,
    meta: opts?.meta,
  }

  addEntry(entry)
  return entry
}

export const assistantLogger = {
  debug: (action: AssistantAction, message: string, opts?: Parameters<typeof log>[3]) =>
    log('debug', action, message, opts),
  info: (action: AssistantAction, message: string, opts?: Parameters<typeof log>[3]) =>
    log('info', action, message, opts),
  warn: (action: AssistantAction, message: string, opts?: Parameters<typeof log>[3]) =>
    log('warn', action, message, opts),
  error: (action: AssistantAction, message: string, opts?: Parameters<typeof log>[3]) =>
    log('error', action, message, opts),
  critical: (action: AssistantAction, message: string, opts?: Parameters<typeof log>[3]) =>
    log('critical', action, message, opts),

  getEntries: (opts?: {
    level?: LogLevel
    action?: AssistantAction
    since?: number
    limit?: number
  }): LogEntry[] => {
    let result = entries
    if (opts?.level)
      result = result.filter((e) => LOG_LEVEL_PRIORITY[e.level] >= LOG_LEVEL_PRIORITY[opts.level!])
    if (opts?.action) result = result.filter((e) => e.action === opts.action)
    if (opts?.since) result = result.filter((e) => e.timestamp >= opts.since!)
    if (opts?.limit) result = result.slice(-opts.limit)
    return result
  },

  getErrorCount: (sinceMs: number): number => {
    const cutoff = Date.now() - sinceMs
    return entries.filter(
      (e) => e.timestamp >= cutoff && (e.level === 'error' || e.level === 'critical'),
    ).length
  },

  clear: (): void => {
    entries.length = 0
  },
}
