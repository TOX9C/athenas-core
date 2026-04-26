import { BrowserWindow, app } from 'electron'
import * as pty from 'node-pty'
import { platform } from 'os'

const sessions = new Map<string, pty.IPty>()
const history = new Map<string, string>()
const MAX_HISTORY_BYTES = 100_000

function getDefaultShell(): string {
  if (platform() === 'win32') return 'powershell.exe'
  return process.env.SHELL || '/bin/zsh'
}

export function getHistory(id: string): string {
  return history.get(id) || ''
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
  history.set(id, '') // Clear previous history

  ptyProcess.onData((data) => {
    const current = history.get(id) || ''
    const updated = current + data
    history.set(id, updated.length > MAX_HISTORY_BYTES ? updated.slice(-MAX_HISTORY_BYTES) : updated)
    mainWindow.webContents.send(`pty:data:${id}`, data)
  })

  ptyProcess.onExit(({ exitCode }) => {
    sessions.delete(id)
    history.delete(id)
    mainWindow.webContents.send(`pty:exit:${id}`, exitCode)
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
