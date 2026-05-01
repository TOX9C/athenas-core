import { BrowserWindow, app } from 'electron'
import * as pty from 'node-pty'
import { platform } from 'os'
import type { AgentType } from '../src/types/workspace'
import {
  Osc633Parser,
  createCommandTracker,
  processSequences,
  stripOsc633,
  type CommandTracker,
  type ShellIntegrationEvent,
} from './services/shellIntegration'
import {
  getShellIntegrationScript,
  isShellIntegrationCompatible,
  buildShellIntegrationEnv,
} from './services/shellHooks'

const sessions = new Map<string, pty.IPty>()
const historyChunks = new Map<string, string[]>()
const historySize = new Map<string, number>()
const sessionAgentTypes = new Map<string, string>()
const sessionShells = new Map<string, string>()
const commandTrackers = new Map<string, CommandTracker>()
const oscParsers = new Map<string, Osc633Parser>()
const MAX_HISTORY_BYTES = 100_000

let outputCaptureHooks: {
  onSpawn: (paneId: string, agentType: string) => void
  onData: (paneId: string, data: string) => void
  onExit: (paneId: string) => void
} | null = null

let shellIntegrationHooks: {
  onEvent: (event: ShellIntegrationEvent) => void
} | null = null

export function setOutputCaptureHooks(hooks: {
  onSpawn: (paneId: string, agentType: string) => void
  onData: (paneId: string, data: string) => void
  onExit: (paneId: string) => void
}): void {
  outputCaptureHooks = hooks
}

export function setShellIntegrationHooks(hooks: {
  onEvent: (event: ShellIntegrationEvent) => void
}): void {
  shellIntegrationHooks = hooks
}

const READY_PATTERNS = [
  /\$\s*$/,
  /❯\s*$/,
  />\s*$/,
  />>>\s*$/,
  /% \s*$/,
  /\? $/,
  /╰─+>\s*$/,
  /\(y\/n\)\s*$/i,
]

function getDefaultShell(): string {
  if (platform() === 'win32') return 'powershell.exe'
  return process.env.SHELL || '/bin/zsh'
}

export function getHistory(id: string): string {
  const chunks = historyChunks.get(id)
  return chunks ? chunks.join('') : ''
}

export function hasSession(id: string): boolean {
  return sessions.has(id)
}

function emitShellEvent(mainWindow: BrowserWindow, event: ShellIntegrationEvent): void {
  if (mainWindow.isDestroyed()) return

  mainWindow.webContents.send(`shell-integration:${event.paneId}`, event)

  switch (event.type) {
    case 'cwd':
      mainWindow.webContents.send('shell-cwd-changed', {
        paneId: event.paneId,
        cwd: event.cwd,
        timestamp: event.timestamp,
      })
      break
    case 'commandStart':
      mainWindow.webContents.send('shell-command-started', {
        paneId: event.paneId,
        command: event.command,
        cwd: event.cwd,
        timestamp: event.timestamp,
      })
      break
    case 'commandFinished':
      mainWindow.webContents.send('shell-command-exited', {
        paneId: event.paneId,
        command: event.command,
        exitCode: event.exitCode,
        cwd: event.cwd,
        duration: event.duration,
        timestamp: event.timestamp,
      })
      break
  }
}

export function spawn(
  id: string,
  cwd: string,
  shell: string,
  agentCmd: string | undefined,
  mainWindow: BrowserWindow,
): void {
  if (sessions.has(id)) {
    kill(id)
  }

  const shellPath = shell || getDefaultShell()
  const isWin = platform() === 'win32'
  const shellArgs = isWin ? [] : ['-l']

  const isAthena = id.includes('__athena__')
  const canIntegrate = !isAthena && !isWin && isShellIntegrationCompatible(shellPath)
  let spawnEnv: Record<string, string> = isAthena
    ? { ...process.env, CI: '1', TERM: 'dumb', FORCE_COLOR: '0', NO_COLOR: '1' }
    : { ...process.env }

  if (canIntegrate) {
    const siEnv = buildShellIntegrationEnv(shellPath)
    spawnEnv = { ...spawnEnv, ...siEnv }
  }

  const ptyProcess = pty.spawn(shellPath, shellArgs, {
    name: isAthena ? 'dumb' : 'xterm-256color',
    cols: 80,
    rows: 24,
    cwd,
    env: spawnEnv,
  })

  sessions.set(id, ptyProcess)
  sessionShells.set(id, shellPath)
  historyChunks.set(id, [])
  historySize.set(id, 0)

  if (canIntegrate) {
    commandTrackers.set(id, createCommandTracker())
    oscParsers.set(id, new Osc633Parser())
  }

  const agentType = sessionAgentTypes.get(id) || 'shell'
  if (outputCaptureHooks) {
    outputCaptureHooks.onSpawn(id, agentType)
  }

  if (canIntegrate) {
    const siScript = getShellIntegrationScript(shellPath)
    setTimeout(() => {
      if (sessions.has(id)) {
        ptyProcess.write(siScript + '\n')
      }
    }, 100)
  }

  ptyProcess.onData((data) => {
    const chunks = historyChunks.get(id) || []
    let size = historySize.get(id) || 0
    chunks.push(data)
    size += data.length

    while (size > MAX_HISTORY_BYTES && chunks.length > 0) {
      const removed = chunks.shift()!
      size -= removed.length
    }
    historyChunks.set(id, chunks)
    historySize.set(id, size)

    if (outputCaptureHooks) {
      outputCaptureHooks.onData(id, data)
    }

    if (canIntegrate && commandTrackers.has(id) && oscParsers.has(id)) {
      const parser = oscParsers.get(id)!
      const parsed = parser.feed(data)
      if (parsed.length > 0) {
        const tracker = commandTrackers.get(id)!
        const events = processSequences(tracker, parsed, id)
        for (const event of events) {
          emitShellEvent(mainWindow, event)
          if (shellIntegrationHooks) {
            shellIntegrationHooks.onEvent(event)
          }
        }
      }
    }

    const stripped = data
      .replace(/\x1b\].*?(?:\x07|\x1b\\)/g, '')
      .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')
      .replace(/\x1b[()][0-9A-B]/g, '')
      .replace(/\r/g, '')
    const lines = stripped.split('\n').filter((l) => l.trim().length > 0)
    const lastLine = lines[lines.length - 1]?.trimEnd() ?? ''

    if (lastLine) {
      if (READY_PATTERNS.some((re) => re.test(lastLine))) {
        if (!mainWindow.isDestroyed()) {
          mainWindow.webContents.send(`pty:ready:${id}`)
        }
      }
    }

    if (!mainWindow.isDestroyed()) {
      mainWindow.webContents.send(`pty:data:${id}`, data)
    }
  })

  ptyProcess.onExit(({ exitCode }) => {
    sessions.delete(id)
    historyChunks.delete(id)
    historySize.delete(id)
    sessionAgentTypes.delete(id)
    sessionShells.delete(id)
    commandTrackers.delete(id)
    oscParsers.delete(id)
    if (outputCaptureHooks) {
      outputCaptureHooks.onExit(id)
    }
    if (!mainWindow.isDestroyed()) {
      mainWindow.webContents.send(`pty:exit:${id}`, exitCode)
    }
    app.emit('agent:exited', { id, exitCode })
  })

  if (agentCmd) {
    setTimeout(() => {
      if (sessions.has(id)) {
        ptyProcess.write(agentCmd + '\r')
      }
    }, 1000)
  }
}

export function spawnAgent(
  id: string,
  cwd: string,
  shell: string,
  agentCmd: string | undefined,
  mainWindow: BrowserWindow,
  agentType: AgentType,
  paneId?: string,
  sessionId?: string,
): void {
  sessionAgentTypes.set(id, agentType)
  const { buildSpawnPrefix } = require('./agentMcpConfig') as {
    buildSpawnPrefix: typeof import('./agentMcpConfig').buildSpawnPrefix
  }
  const prefixedCmd = agentCmd
    ? `${buildSpawnPrefix(agentType, paneId, sessionId)}${agentCmd}`
    : undefined
  spawn(id, cwd, shell, prefixedCmd, mainWindow)
}

export function write(id: string, data: string): void {
  sessions.get(id)?.write(data)
}

export function resize(id: string, cols: number, rows: number): void {
  try {
    sessions.get(id)?.resize(cols, rows)
  } catch {
    // ignore resize errors for dead sessions
  }
}

export function getCommandTracker(id: string): CommandTracker | undefined {
  return commandTrackers.get(id)
}

export function kill(id: string): void {
  const session = sessions.get(id)
  if (session) {
    session.kill()
    sessions.delete(id)
    sessionShells.delete(id)
    commandTrackers.delete(id)
    oscParsers.delete(id)
  }
}

export async function getCwd(id: string): Promise<string | null> {
  const tracker = commandTrackers.get(id)
  if (tracker?.currentCwd) return tracker.currentCwd

  const session = sessions.get(id)
  if (!session) return null

  if (platform() !== 'win32') {
    try {
      const { execFileSync } = require('child_process')
      const result = execFileSync(
        'lsof',
        ['-a', '-p', String(session.pid), '-d', 'cwd', '-F', 'n'],
        { encoding: 'utf8' },
      )
      const lines = result.split('\n')
      const dirLine = lines.find((l: string) => l.startsWith('n'))
      if (dirLine) return dirLine.substring(1)
    } catch {
      return null
    }
  }
  return null
}

export async function gracefulShutdown(): Promise<void> {
  if (sessions.size === 0) return
  for (const session of sessions.values()) {
    try {
      session.write('\x03')
      setTimeout(() => {
        try {
          session.write('/exit\r')
        } catch {}
      }, 50)
    } catch {}
  }
  await new Promise((resolve) => setTimeout(resolve, 800))
}
