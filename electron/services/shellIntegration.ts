export type ShellIntegrationSequence =
  | { type: 'prompt'; data?: string }
  | { type: 'command'; data: string }
  | { type: 'commandStart' }
  | { type: 'commandExecuted' }
  | { type: 'commandFinished'; exitCode: number }
  | { type: 'cwd'; data: string }
  | { type: 'property'; key: string; value: string }

const OSC_PREFIX = '\x1b]633;'
const BEL = '\x07'
const ST = '\x1b\\'

export interface ParsedSequence {
  sequence: ShellIntegrationSequence
  rawLength: number
}

export class Osc633Parser {
  private buffer = ''

  feed(data: string): ParsedSequence[] {
    this.buffer += data
    const results: ParsedSequence[] = []

    while (this.buffer.length > 0) {
      const oscStart = this.buffer.indexOf(OSC_PREFIX)
      if (oscStart === -1) {
        if (this.buffer.length > 10000) {
          this.buffer = this.buffer.slice(-4096)
        }
        break
      }

      if (oscStart > 0) {
        this.buffer = this.buffer.slice(oscStart)
      }

      const payloadStart = OSC_PREFIX.length

      const belIdx = this.buffer.indexOf(BEL, payloadStart)
      const stIdx = this.buffer.indexOf(ST, payloadStart)

      let terminatorIdx = -1
      let terminatorLen = 0

      if (belIdx !== -1 && (stIdx === -1 || belIdx < stIdx)) {
        terminatorIdx = belIdx
        terminatorLen = 1
      } else if (stIdx !== -1) {
        terminatorIdx = stIdx
        terminatorLen = 2
      }

      if (terminatorIdx === -1) {
        if (this.buffer.length > 100000) {
          const nextEsc = this.buffer.indexOf('\x1b', 1)
          if (nextEsc !== -1) {
            this.buffer = this.buffer.slice(nextEsc)
            continue
          }
          this.buffer = ''
          break
        }
        break
      }

      const payload = this.buffer.substring(payloadStart, terminatorIdx)
      const rawLength = terminatorIdx + terminatorLen
      this.buffer = this.buffer.slice(rawLength)

      const seq = parsePayload(payload)
      if (seq) {
        results.push({ sequence: seq, rawLength })
      }
    }

    return results
  }

  reset(): void {
    this.buffer = ''
  }
}

export function parseOsc633(data: string): ParsedSequence[] {
  const parser = new Osc633Parser()
  return parser.feed(data)
}

function parsePayload(payload: string): ShellIntegrationSequence | null {
  const semiIdx = payload.indexOf(';')
  const command = semiIdx === -1 ? payload : payload.substring(0, semiIdx)
  const rest = semiIdx === -1 ? '' : payload.substring(semiIdx + 1)

  switch (command) {
    case 'A':
      return { type: 'prompt' }
    case 'B':
      return { type: 'command', data: rest }
    case 'C':
      return { type: 'commandStart' }
    case 'D': {
      const code = rest === '' ? 0 : parseInt(rest, 10)
      return { type: 'commandFinished', exitCode: isNaN(code) ? 0 : code }
    }
    case 'E':
      return { type: 'commandExecuted' }
    case 'P':
      return { type: 'cwd', data: rest }
    case 'Is':
      return { type: 'property', key: 'icon', value: rest }
    case 'Set':
    case 'S': {
      const eqIdx = rest.indexOf('=')
      if (eqIdx === -1) return { type: 'property', key: rest, value: '' }
      return { type: 'property', key: rest.substring(0, eqIdx), value: rest.substring(eqIdx + 1) }
    }
    default:
      return null
  }
}

export function stripOsc633(data: string): string {
  let result = ''
  let pos = 0

  while (pos < data.length) {
    const oscStart = data.indexOf(OSC_PREFIX, pos)
    if (oscStart === -1) {
      result += data.substring(pos)
      break
    }

    result += data.substring(pos, oscStart)
    const payloadStart = oscStart + OSC_PREFIX.length

    const belIdx = data.indexOf(BEL, payloadStart)
    const stIdx = data.indexOf(ST, payloadStart)

    if (belIdx !== -1 && (stIdx === -1 || belIdx < stIdx)) {
      pos = belIdx + 1
    } else if (stIdx !== -1) {
      pos = stIdx + 2
    } else {
      result += data.substring(oscStart)
      break
    }
  }

  return result
}

export interface CommandTracker {
  activeCommand: string | null
  activeStartTime: number | null
  activeStartNotified: boolean
  pendingCommandText: string | null
  currentCwd: string | null
  lastExitCode: number | null
}

export function createCommandTracker(): CommandTracker {
  return {
    activeCommand: null,
    activeStartTime: null,
    activeStartNotified: false,
    pendingCommandText: null,
    currentCwd: null,
    lastExitCode: null,
  }
}

export function processSequences(
  tracker: CommandTracker,
  sequences: ParsedSequence[],
  paneId: string,
): ShellIntegrationEvent[] {
  const events: ShellIntegrationEvent[] = []

  for (const { sequence } of sequences) {
    switch (sequence.type) {
      case 'prompt':
        tracker.activeCommand = null
        tracker.activeStartTime = null
        tracker.activeStartNotified = false
        tracker.pendingCommandText = null
        events.push({ type: 'prompt', paneId, timestamp: Date.now() })
        break

      case 'command':
        tracker.pendingCommandText = sequence.data
        break

      case 'commandStart':
        tracker.activeCommand = tracker.pendingCommandText || ''
        tracker.activeStartTime = Date.now()
        tracker.activeStartNotified = true
        tracker.pendingCommandText = null
        events.push({
          type: 'commandStart',
          paneId,
          command: tracker.activeCommand,
          cwd: tracker.currentCwd || undefined,
          timestamp: tracker.activeStartTime,
        })
        break

      case 'commandExecuted':
        if (tracker.activeCommand !== null) {
          events.push({
            type: 'commandExecuted',
            paneId,
            command: tracker.activeCommand,
            cwd: tracker.currentCwd || undefined,
            timestamp: Date.now(),
          })
        }
        break

      case 'commandFinished':
        tracker.lastExitCode = sequence.exitCode
        events.push({
          type: 'commandFinished',
          paneId,
          exitCode: sequence.exitCode,
          command: tracker.activeCommand || '',
          cwd: tracker.currentCwd || undefined,
          timestamp: Date.now(),
          duration: tracker.activeStartTime ? Date.now() - tracker.activeStartTime : undefined,
        })
        tracker.activeCommand = null
        tracker.activeStartTime = null
        tracker.activeStartNotified = false
        tracker.pendingCommandText = null
        break

      case 'cwd':
        tracker.currentCwd = sequence.data
        events.push({
          type: 'cwd',
          paneId,
          cwd: sequence.data,
          timestamp: Date.now(),
        })
        break

      case 'property':
        events.push({
          type: 'property',
          paneId,
          key: sequence.key,
          value: sequence.value,
          timestamp: Date.now(),
        })
        break
    }
  }

  return events
}

export interface ShellIntegrationEvent {
  type: 'prompt' | 'commandStart' | 'commandExecuted' | 'commandFinished' | 'cwd' | 'property'
  paneId: string
  timestamp: number
  command?: string
  exitCode?: number
  cwd?: string
  duration?: number
  key?: string
  value?: string
}
