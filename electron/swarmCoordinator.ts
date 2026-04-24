import { BrowserWindow, ipcMain } from 'electron'
import { readFile, writeFile, mkdir, access } from 'fs/promises'
import { join } from 'path'
import { watch, FSWatcher } from 'fs'

let mainWindowRef: BrowserWindow | null = null
let stateWatcher: FSWatcher | null = null
let pollInterval: ReturnType<typeof setInterval> | null = null

export function initSwarmCoordinator(mainWindow: BrowserWindow): void {
  mainWindowRef = mainWindow

  ipcMain.handle('swarm:readState', async (_event, dir: string) => {
    try {
      const statePath = join(dir, '.ade', 'swarm-state.json')
      const content = await readFile(statePath, 'utf-8')
      return JSON.parse(content)
    } catch {
      return null
    }
  })

  ipcMain.handle('swarm:writeState', async (_event, dir: string, state: any) => {
    try {
      const adeDir = join(dir, '.ade')
      try { await access(adeDir) } catch { await mkdir(adeDir, { recursive: true }) }
      const statePath = join(adeDir, 'swarm-state.json')
      await writeFile(statePath, JSON.stringify(state, null, 2), 'utf-8')
      return { success: true }
    } catch (err: any) {
      return { success: false, error: err.message }
    }
  })

  ipcMain.handle('swarm:sendMessage', async (_event, dir: string, from: string, to: string, msg: string) => {
    try {
      const mailboxDir = join(dir, '.ade', 'mailbox')
      try { await access(mailboxDir) } catch { await mkdir(mailboxDir, { recursive: true }) }

      const mailboxPath = join(mailboxDir, `${to}.json`)
      let messages: any[] = []
      try {
        const content = await readFile(mailboxPath, 'utf-8')
        messages = JSON.parse(content)
      } catch {
        // file doesn't exist yet
      }

      messages.push({
        id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        from,
        to,
        content: msg,
        timestamp: Date.now(),
        read: false,
      })

      await writeFile(mailboxPath, JSON.stringify(messages, null, 2), 'utf-8')
      return { success: true }
    } catch (err: any) {
      return { success: false, error: err.message }
    }
  })

  ipcMain.handle('swarm:readMailbox', async (_event, dir: string, agentId: string) => {
    try {
      const mailboxPath = join(dir, '.ade', 'mailbox', `${agentId}.json`)
      const content = await readFile(mailboxPath, 'utf-8')
      return JSON.parse(content)
    } catch {
      return []
    }
  })

  ipcMain.on('swarm:watchState', (_event, dir: string) => {
    if (stateWatcher) {
      stateWatcher.close()
      stateWatcher = null
    }
    if (pollInterval) {
      clearInterval(pollInterval)
      pollInterval = null
    }

    const statePath = join(dir, '.ade', 'swarm-state.json')

    pollInterval = setInterval(async () => {
      try {
        const content = await readFile(statePath, 'utf-8')
        const state = JSON.parse(content)

        const now = Date.now()
        let modified = false
        for (const agent of state.agents ?? []) {
          if (
            agent.status !== 'done' &&
            agent.status !== 'stalled' &&
            agent.lastActionAt &&
            now - agent.lastActionAt > 90_000
          ) {
            agent.status = 'stalled'
            modified = true
          }
        }

        if (modified) {
          await writeFile(statePath, JSON.stringify(state, null, 2), 'utf-8')
        }

        mainWindowRef?.webContents.send('swarm:stateChange', state)
      } catch {
        // state file doesn't exist or is invalid
      }
    }, 5000)
  })

  ipcMain.on('swarm:stopWatch', () => {
    if (stateWatcher) {
      stateWatcher.close()
      stateWatcher = null
    }
    if (pollInterval) {
      clearInterval(pollInterval)
      pollInterval = null
    }
  })
}
