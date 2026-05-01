#!/bin/bash
cat << 'INNER_EOF' > electron/ptyManager_new.ts
import { BrowserWindow, app } from 'electron'
import * as pty from 'node-pty'
import { platform } from 'os'

const sessions = new Map<string, pty.IPty>()
const historyChunks = new Map<string, string[]>()
const historySize = new Map<string, number>()
const MAX_HISTORY_BYTES = 100_000

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

export function spawn(
  id: string,
  cwd: string,
  shell: string,
  agentCmd: string | undefined,
  mainWindow: BrowserWindow
): void {
  if (sessions.has(id)) {
    kill(id)
  }

  const shellPath = shell || getDefaultShell()
  const isWin = platform() === 'win32'
  const shellArgs = isWin ? [] : ['-l']

  const isAthena = id.includes('__athena__')
  const customEnv = isAthena ? { ...process.env, CI: '1', TERM: 'dumb', FORCE_COLOR: '0', NO_COLOR: '1' } : process.env

  const ptyProcess = pty.spawn(shellPath, shellArgs, {
    name: isAthena ? 'dumb' : 'xterm-256color',
    cols: 80,
    rows: 24,
    cwd,
    env: customEnv as Record<string, string>
  })

  sessions.set(id, ptyProcess)
  historyChunks.set(id, []) // Clear previous history chunks
  historySize.set(id, 0) // Clear previous history size

  ptyProcess.onData((data) => {
    let chunks = historyChunks.get(id) || []
    let size = historySize.get(id) || 0
    chunks.push(data)
    size += data.length

    while (size > MAX_HISTORY_BYTES && chunks.length > 0) {
      const removed = chunks.shift()!
      size -= removed.length
    }
    historyChunks.set(id, chunks)
    historySize.set(id, size)

    // Check ready state pattern
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

export function kill(id: string): void {
  const session = sessions.get(id)
  if (session) {
    session.kill()
    sessions.delete(id)
  }
}

export async function getCwd(id: string): Promise<string | null> {
  const session = sessions.get(id)
  if (!session) return null

  if (platform() !== 'win32') {
    try {
      const { execFileSync } = require('child_process')
      const result = execFileSync('lsof', ['-a', '-p', String(session.pid), '-d', 'cwd', '-F', 'n'], { encoding: 'utf8' })
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
  if (sessions.size === 0) return;
  for (const session of sessions.values()) {
    try {
      session.write('\x03'); // Send Ctrl+C to interrupt any running tasks first
      setTimeout(() => {
        try {
          session.write('/exit\r');
        } catch { } // ignore
      }, 50);
    } catch { } // ignore dead sessions
  }
  // Allow minimum buffer time for processes to detect and save to disk
  await new Promise(resolve => setTimeout(resolve, 800));
}
INNER_EOF
mv electron/ptyManager_new.ts electron/ptyManager.ts
