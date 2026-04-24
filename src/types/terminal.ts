export interface CommandBlock {
  id: string
  command: string
  output: string
  exitCode: number | null
  startedAt: number
  finishedAt: number | null
  collapsed: boolean
}

export interface PtySession {
  paneId: string
  pid?: number
  status: 'idle' | 'running' | 'exited' | 'error'
  blocks: CommandBlock[]
  errorMessage?: string
}
