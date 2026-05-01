export type CircuitState = 'closed' | 'open' | 'half_open'

export interface CircuitBreakerConfig {
  failureThreshold: number
  resetTimeoutMs: number
  halfOpenMaxAttempts: number
  monitoringWindowMs: number
  onStateChange?: (from: CircuitState, to: CircuitState) => void
}

export interface CircuitBreakerStats {
  state: CircuitState
  failureCount: number
  successCount: number
  lastFailureAt: number | null
  lastSuccessAt: number | null
  lastStateChangeAt: number
  totalTrips: number
  consecutiveFailures: number
  nextRetryAt: number | null
}

const DEFAULT_CONFIG: CircuitBreakerConfig = {
  failureThreshold: 5,
  resetTimeoutMs: 30_000,
  halfOpenMaxAttempts: 1,
  monitoringWindowMs: 60_000,
}

interface FailureRecord {
  timestamp: number
  error: unknown
}

export class CircuitBreaker {
  private config: CircuitBreakerConfig
  private state: CircuitState = 'closed'
  private failures: FailureRecord[] = []
  private consecutiveFailures = 0
  private successCount = 0
  private lastFailureAt: number | null = null
  private lastSuccessAt: number | null = null
  private lastStateChangeAt = Date.now()
  private totalTrips = 0
  private halfOpenAttempts = 0
  private openedAt: number | null = null

  constructor(config: Partial<CircuitBreakerConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config }
  }

  private transitionTo(newState: CircuitState): void {
    if (this.state === newState) return
    const oldState = this.state
    this.state = newState
    this.lastStateChangeAt = Date.now()

    if (newState === 'open') {
      this.openedAt = Date.now()
      this.totalTrips++
      this.halfOpenAttempts = 0
    } else if (newState === 'half_open') {
      this.halfOpenAttempts = 0
    } else if (newState === 'closed') {
      this.failures = []
      this.consecutiveFailures = 0
      this.openedAt = null
    }

    this.config.onStateChange?.(oldState, newState)
  }

  private pruneFailures(): void {
    const cutoff = Date.now() - this.config.monitoringWindowMs
    this.failures = this.failures.filter((f) => f.timestamp >= cutoff)
  }

  private checkState(): void {
    if (this.state === 'open') {
      const elapsed = this.openedAt ? Date.now() - this.openedAt : 0
      if (elapsed >= this.config.resetTimeoutMs) {
        this.transitionTo('half_open')
      }
    }
  }

  canExecute(): boolean {
    this.checkState()
    this.pruneFailures()

    switch (this.state) {
      case 'closed':
        return true
      case 'open':
        return false
      case 'half_open':
        return this.halfOpenAttempts < this.config.halfOpenMaxAttempts
    }
  }

  recordSuccess(): void {
    this.successCount++
    this.lastSuccessAt = Date.now()

    if (this.state === 'half_open') {
      this.transitionTo('closed')
    }
  }

  recordFailure(error: unknown): void {
    const failure: FailureRecord = { timestamp: Date.now(), error }
    this.failures.push(failure)
    this.consecutiveFailures++
    this.lastFailureAt = Date.now()

    if (this.state === 'half_open') {
      this.halfOpenAttempts++
      this.transitionTo('open')
      return
    }

    if (this.state === 'closed') {
      this.pruneFailures()
      if (
        this.failures.length >= this.config.failureThreshold ||
        this.consecutiveFailures >= this.config.failureThreshold
      ) {
        this.transitionTo('open')
      }
    }
  }

  async execute<T>(fn: () => Promise<T>): Promise<T> {
    if (!this.canExecute()) {
      const retryAt = this.openedAt ? this.openedAt + this.config.resetTimeoutMs : null
      const error = new CircuitOpenError(`Circuit breaker is ${this.state}`, this.state, retryAt)
      throw error
    }

    if (this.state === 'half_open') {
      this.halfOpenAttempts++
    }

    try {
      const result = await fn()
      this.recordSuccess()
      return result
    } catch (err) {
      this.recordFailure(err)
      throw err
    }
  }

  reset(): void {
    this.transitionTo('closed')
  }

  getStats(): CircuitBreakerStats {
    this.checkState()
    this.pruneFailures()

    return {
      state: this.state,
      failureCount: this.failures.length,
      successCount: this.successCount,
      lastFailureAt: this.lastFailureAt,
      lastSuccessAt: this.lastSuccessAt,
      lastStateChangeAt: this.lastStateChangeAt,
      totalTrips: this.totalTrips,
      consecutiveFailures: this.consecutiveFailures,
      nextRetryAt:
        this.state === 'open' && this.openedAt ? this.openedAt + this.config.resetTimeoutMs : null,
    }
  }

  getState(): CircuitState {
    this.checkState()
    return this.state
  }
}

export class CircuitOpenError extends Error {
  public readonly circuitState: CircuitState
  public readonly retryAt: number | null

  constructor(message: string, state: CircuitState, retryAt: number | null) {
    super(message)
    this.name = 'CircuitOpenError'
    this.circuitState = state
    this.retryAt = retryAt
  }
}

export function createAssistantCircuitBreaker(
  onStateChange?: (from: CircuitState, to: CircuitState) => void,
): CircuitBreaker {
  return new CircuitBreaker({
    failureThreshold: 5,
    resetTimeoutMs: 30_000,
    halfOpenMaxAttempts: 2,
    monitoringWindowMs: 60_000,
    onStateChange,
  })
}
