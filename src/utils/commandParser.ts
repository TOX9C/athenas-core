export interface CommandBlockEvent {
  type: 'command-start' | 'command-end' | 'output'
  command?: string
  output?: string
  exitCode?: number
}

const PROMPT_PATTERNS = [
  /[\$%#❯›»]\s*$/,
  /\]\s*[\$%#]\s*$/,
  /^[\w.-]+@[\w.-]+[:\s]/,
  /^➜\s/,
  /^\([\w.-]+\)\s*[\$%#]\s*$/,
]

export function isPromptLine(line: string): boolean {
  const trimmed = line.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '').trimEnd()
  return PROMPT_PATTERNS.some((p) => p.test(trimmed))
}

export class CommandParser {
  private buffer = ''
  private currentCommand: string | null = null
  private outputLines: string[] = []
  private onEvent: (event: CommandBlockEvent) => void
  private promptSeen = false

  constructor(onEvent: (event: CommandBlockEvent) => void) {
    this.onEvent = onEvent
  }

  feed(data: string): void {
    this.buffer += data
    const lines = this.buffer.split('\n')
    this.buffer = lines.pop() ?? ''

    for (const line of lines) {
      this.processLine(line)
    }

    if (this.buffer && isPromptLine(this.buffer)) {
      this.finishCurrentCommand()
      this.promptSeen = true
      this.buffer = ''
    }
  }

  private processLine(line: string): void {
    if (isPromptLine(line)) {
      this.finishCurrentCommand()
      this.promptSeen = true
      return
    }

    if (this.promptSeen && this.currentCommand === null) {
      const cleaned = line.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '').trim()
      if (cleaned) {
        this.currentCommand = cleaned
        this.outputLines = []
        this.promptSeen = false
        this.onEvent({ type: 'command-start', command: cleaned })
      }
      return
    }

    if (this.currentCommand !== null) {
      this.outputLines.push(line)
      this.onEvent({ type: 'output', output: line })
    }
  }

  private finishCurrentCommand(): void {
    if (this.currentCommand !== null) {
      this.onEvent({
        type: 'command-end',
        command: this.currentCommand,
        output: this.outputLines.join('\n'),
      })
      this.currentCommand = null
      this.outputLines = []
    }
  }

  reset(): void {
    this.buffer = ''
    this.currentCommand = null
    this.outputLines = []
    this.promptSeen = false
  }
}
