export interface CommandBlock {
  id: string
  command: string
  output: string
  exitCode: number | null
  startedAt: number
  finishedAt: number | null
  duration: number | null
  collapsed: boolean
}

export interface PtySession {
  paneId: string
  pid?: number
  status: 'idle' | 'running' | 'exited' | 'error'
  blocks: CommandBlock[]
  errorMessage?: string
  cwd?: string | null
  lastCommand?: string | null
  lastExitCode?: number | null
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

export interface ShellCwdChangedEvent {
  paneId: string
  cwd: string
  timestamp: number
}

export interface ShellCommandStartedEvent {
  paneId: string
  command: string
  cwd?: string
  timestamp: number
}

export interface ShellCommandExitedEvent {
  paneId: string
  command: string
  exitCode: number
  cwd?: string
  duration?: number
  timestamp: number
}
