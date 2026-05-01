export type HealthStatus = 'healthy' | 'degraded' | 'unhealthy' | 'offline'
export type HealthCheckType = 'provider' | 'session' | 'circuit' | 'heartbeat'

export interface HealthCheckResult {
  type: HealthCheckType
  status: HealthStatus
  message: string
  timestamp: number
  latencyMs?: number
  details?: Record<string, unknown>
}

export interface AssistantHealthSnapshot {
  overall: HealthStatus
  checks: HealthCheckResult[]
  lastCheckedAt: number
  consecutiveFailures: number
  lastHealthyAt: number | null
  lastUnhealthyAt: number | null
  uptimeMs: number
  totalRequests: number
  totalFailures: number
  totalSuccesses: number
}

type HealthChangeListener = (snapshot: AssistantHealthSnapshot) => void

const listeners = new Set<HealthChangeListener>()
let snapshot: AssistantHealthSnapshot = createInitialSnapshot()
let startedAt = Date.now()

function createInitialSnapshot(): AssistantHealthSnapshot {
  return {
    overall: 'offline',
    checks: [],
    lastCheckedAt: 0,
    consecutiveFailures: 0,
    lastHealthyAt: null,
    lastUnhealthyAt: null,
    uptimeMs: 0,
    totalRequests: 0,
    totalFailures: 0,
    totalSuccesses: 0,
  }
}

function computeOverall(checks: HealthCheckResult[]): HealthStatus {
  if (checks.length === 0) return 'offline'
  const statuses = checks.map((c) => c.status)
  if (statuses.some((s) => s === 'unhealthy' || s === 'offline')) return 'unhealthy'
  if (statuses.some((s) => s === 'degraded')) return 'degraded'
  return 'healthy'
}

function notifyListeners(): void {
  for (const listener of listeners) {
    try {
      listener(snapshot)
    } catch {}
  }
}

export const assistantHealth = {
  start(): void {
    startedAt = Date.now()
    snapshot = createInitialSnapshot()
    snapshot.overall = 'healthy'
    snapshot.lastHealthyAt = Date.now()
  },

  recordCheck(result: HealthCheckResult): void {
    snapshot.checks = snapshot.checks.filter((c) => c.type !== result.type)
    snapshot.checks.push(result)
    snapshot.lastCheckedAt = Date.now()
    snapshot.overall = computeOverall(snapshot.checks)

    if (result.status === 'healthy') {
      snapshot.consecutiveFailures = 0
      snapshot.lastHealthyAt = Date.now()
    } else {
      snapshot.consecutiveFailures++
      snapshot.lastUnhealthyAt = Date.now()
    }

    snapshot.uptimeMs = Date.now() - startedAt
    notifyListeners()
  },

  recordRequest(success: boolean): void {
    snapshot.totalRequests++
    if (success) {
      snapshot.totalSuccesses++
    } else {
      snapshot.totalFailures++
    }
    snapshot.uptimeMs = Date.now() - startedAt
  },

  getSnapshot(): AssistantHealthSnapshot {
    return { ...snapshot, checks: [...snapshot.checks] }
  },

  getStatus(): HealthStatus {
    return snapshot.overall
  },

  isHealthy(): boolean {
    return snapshot.overall === 'healthy' || snapshot.overall === 'degraded'
  },

  shouldRecover(): boolean {
    return snapshot.consecutiveFailures >= 3 || snapshot.overall === 'unhealthy'
  },

  reset(): void {
    snapshot = createInitialSnapshot()
    snapshot.overall = 'healthy'
    snapshot.lastHealthyAt = Date.now()
    notifyListeners()
  },

  onHealthChange(listener: HealthChangeListener): () => void {
    listeners.add(listener)
    return () => listeners.delete(listener)
  },

  createProviderCheck(provider: string, latencyMs: number, error?: string): HealthCheckResult {
    let status: HealthStatus = 'healthy'
    let message = `Provider ${provider} responding normally`
    if (error) {
      status = latencyMs > 10000 ? 'unhealthy' : 'degraded'
      message = `Provider ${provider} error: ${error}`
    } else if (latencyMs > 10000) {
      status = 'degraded'
      message = `Provider ${provider} slow response (${latencyMs}ms)`
    }
    return {
      type: 'provider',
      status,
      message,
      timestamp: Date.now(),
      latencyMs,
      details: { provider, error },
    }
  },

  createCircuitCheck(circuitState: string, tripCount: number): HealthCheckResult {
    let status: HealthStatus = 'healthy'
    let message = `Circuit breaker closed`
    if (circuitState === 'open') {
      status = 'unhealthy'
      message = `Circuit breaker OPEN (${tripCount} trips)`
    } else if (circuitState === 'half_open') {
      status = 'degraded'
      message = `Circuit breaker HALF-OPEN (probing)`
    }
    return {
      type: 'circuit',
      status,
      message,
      timestamp: Date.now(),
      details: { circuitState, tripCount },
    }
  },

  createSessionCheck(activeSessions: number, stalledSessions: number): HealthCheckResult {
    let status: HealthStatus = 'healthy'
    let message = `${activeSessions} active sessions`
    if (stalledSessions > 0) {
      status = stalledSessions > 2 ? 'unhealthy' : 'degraded'
      message = `${stalledSessions} stalled of ${activeSessions} sessions`
    }
    return {
      type: 'session',
      status,
      message,
      timestamp: Date.now(),
      details: { activeSessions, stalledSessions },
    }
  },

  createHeartbeatCheck(responsive: boolean, latencyMs?: number): HealthCheckResult {
    const status: HealthStatus = responsive ? 'healthy' : 'unhealthy'
    const message = responsive
      ? `Assistant responsive${latencyMs ? ` (${latencyMs}ms)` : ''}`
      : 'Assistant not responding'
    return { type: 'heartbeat', status, message, timestamp: Date.now(), latencyMs }
  },
}
