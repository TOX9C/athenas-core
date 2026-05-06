import { app, BrowserWindow, ipcMain, shell, dialog } from 'electron'
import { join } from 'path'

let mainWindow: BrowserWindow | null = null

function createWindow(): void {
  mainWindow = new BrowserWindow({
    width: 1400,
    height: 900,
    minWidth: 900,
    minHeight: 600,
    frame: false,
    titleBarStyle: 'hidden',
    trafficLightPosition: { x: 12, y: 12 },
    backgroundColor: '#0a0a0a',
    webPreferences: {
      preload: join(__dirname, '../preload/index.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false,
    },
  })

  mainWindow.webContents.setWindowOpenHandler(({ url }) => {
    shell.openExternal(url)
    return { action: 'deny' }
  })

  if (process.env.ELECTRON_RENDERER_URL) {
    mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL)
  } else {
    mainWindow.loadFile(join(__dirname, '../renderer/index.html'))
  }
}

// Window control IPC
ipcMain.on('window:minimize', () => mainWindow?.minimize())
ipcMain.on('window:maximize', () => {
  if (mainWindow?.isMaximized()) {
    mainWindow.unmaximize()
  } else {
    mainWindow?.maximize()
  }
})
ipcMain.on('window:close', () => mainWindow?.close())
ipcMain.handle('window:isMaximized', () => mainWindow?.isMaximized() ?? false)
ipcMain.handle('window:platform', () => process.platform)

// File system IPC
ipcMain.handle('fs:showOpenDialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openDirectory'],
  })
  if (result.canceled || result.filePaths.length === 0) return null
  return result.filePaths[0]
})

ipcMain.handle('fs:showImageDialog', async () => {
  if (!mainWindow) return null
  const result = await dialog.showOpenDialog(mainWindow, {
    properties: ['openFile', 'multiSelections'],
    filters: [{ name: 'Images', extensions: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp'] }],
  })
  if (result.canceled || result.filePaths.length === 0) return null
  return result.filePaths
})

ipcMain.handle('fs:readFileAsBase64', async (_event, path: string) => {
  try {
    const { readFile } = await import('fs/promises')
    const ext = path.split('.').pop()?.toLowerCase() || 'png'
    const mimeMap: Record<string, string> = {
      png: 'image/png',
      jpg: 'image/jpeg',
      jpeg: 'image/jpeg',
      gif: 'image/gif',
      webp: 'image/webp',
      bmp: 'image/bmp',
    }
    const buffer = await readFile(path)
    return {
      data: Buffer.from(buffer).toString('base64'),
      mediaType: mimeMap[ext] || 'image/png',
    }
  } catch (err: unknown) {
    return {
      data: null,
      mediaType: null,
      error: err instanceof Error ? err.message : 'Failed to read file',
    }
  }
})

ipcMain.handle('fs:readTree', async (_event, dir: string) => {
  try {
    const { readTree } = await import('./fileSystem')
    return await readTree(dir)
  } catch (err: unknown) {
    return { success: false, error: err instanceof Error ? err.message : String(err) }
  }
})

ipcMain.handle('fs:readFile', async (_event, filePath: string) => {
  try {
    const { readFileContent } = await import('./fileSystem')
    return await readFileContent(filePath)
  } catch (err: unknown) {
    return { success: false, error: err instanceof Error ? err.message : String(err) }
  }
})

ipcMain.handle('fs:getDirectories', async (_event, dirPath: string) => {
  try {
    const { getDirectories } = await import('./fileSystem')
    return await getDirectories(dirPath)
  } catch (err: unknown) {
    return { success: false, error: err instanceof Error ? err.message : String(err) }
  }
})

ipcMain.handle('fs:writeFile', async (_event, filePath: string, content: string) => {
  try {
    const { writeFileContent } = await import('./fileSystem')
    await writeFileContent(filePath, content)
    return { success: true }
  } catch (err: unknown) {
    return { success: false, error: err instanceof Error ? err.message : String(err) }
  }
})

ipcMain.handle('fs:exists', async (_event, filePath: string) => {
  try {
    const fs = await import('fs/promises')
    await fs.access(filePath)
    return true
  } catch {
    return false
  }
})

// Search IPC (ripgrep)
ipcMain.handle(
  'fs:search',
  async (
    _event,
    options: {
      pattern: string
      path: string
      glob?: string
      type?: string
      caseSensitive?: boolean
      maxResults?: number
      contextLines?: number
    },
  ) => {
    try {
      const { searchCode } = await import('./search')
      return await searchCode(options)
    } catch (err: unknown) {
      return { success: false, error: err instanceof Error ? err.message : String(err) }
    }
  },
)

ipcMain.handle(
  'fs:searchFiles',
  async (
    _event,
    directory: string,
    pattern: string,
    options?: { glob?: string; type?: string; maxResults?: number },
  ) => {
    try {
      const { searchFiles } = await import('./search')
      return await searchFiles(directory, pattern, options)
    } catch (err: unknown) {
      return { success: false, error: err instanceof Error ? err.message : String(err) }
    }
  },
)

ipcMain.handle(
  'search:ripgrep',
  async (
    _event,
    options: {
      pattern: string
      path: string
      glob?: string
      type?: string
      caseSensitive?: boolean
      maxResults?: number
      contextLines?: number
    },
  ) => {
    try {
      const { searchRipgrep } = await import('./search')
      return await searchRipgrep(options)
    } catch (err: unknown) {
      return {
        matches: [],
        truncated: false,
        stats: { filesMatched: 0, totalMatches: 0 },
        error: err instanceof Error ? err.message : String(err),
      }
    }
  },
)

// File watching IPC
import type { FSWatcher } from 'chokidar'

// Debounce helper
function debounce(func: Function, wait: number) {
  let timeout: ReturnType<typeof setTimeout> | null = null
  return function (...args: any[]) {
    if (timeout !== null) {
      clearTimeout(timeout)
    }
    timeout = setTimeout(() => {
      func(...args)
    }, wait)
  }
}

const activeWatchers = new Map<string, FSWatcher>()

ipcMain.on('fs:watchDir', async (_event, dir: string) => {
  if (activeWatchers.has(dir)) return
  try {
    const chokidar = (await import('chokidar')).default
    const watcher = chokidar.watch(dir, {
      ignored: /(node_modules|\.git|dist|out|\.cache)/,
      persistent: true,
      ignoreInitial: true,
    })

    const emitChange = debounce((path: string) => {
      mainWindow?.webContents.send(`fs:change:${dir}`, { path: dir })
    }, 300)

    watcher.on('all', (_event, path) => emitChange(path))
    activeWatchers.set(dir, watcher)
  } catch {
    // directory doesn't exist or can't be watched
  }
})

ipcMain.on('fs:unwatchDir', (_event, dir: string) => {
  const watcher = activeWatchers.get(dir)
  if (watcher) {
    watcher.close()
    activeWatchers.delete(dir)
  }
})

// Store IPC (electron-store)
import { getStore } from './storeUtil'

ipcMain.handle('store:get', async (_event, key: string) => {
  const store = await getStore()
  return store.get(key)
})

ipcMain.handle('store:set', async (_event, key: string, value: any) => {
  const store = await getStore()
  store.set(key, value)
})

// Athena Orchestrator IPC
import { athenaOrchestrator, type ImageData } from './athenaOrchestrator'
ipcMain.handle(
  'athena:chat',
  async (_event, message: string, sessionId?: string, images?: ImageData[]) => {
    try {
      if (sessionId && sessionId !== athenaOrchestrator.getCurrentSessionId()) {
        const sessionStore = await import('./sessionStore')
        const session = await sessionStore.getSession(sessionId)
        if (session) {
          const VALID_MEDIA_TYPES = new Set(['image/jpeg', 'image/png', 'image/gif', 'image/webp'])
          athenaOrchestrator.setSessionContext(
            session.messages.map((m) => {
              const rawImages = (m as any).images as
                | Array<{ base64?: string; mediaType?: string }>
                | undefined
              const validImages = rawImages
                ? rawImages
                    .filter(
                      (img) => img.base64 && img.mediaType && VALID_MEDIA_TYPES.has(img.mediaType),
                    )
                    .map((img) => ({
                      base64: img.base64!,
                      mediaType: img.mediaType as ImageData['mediaType'],
                    }))
                : undefined
              return {
                role: m.role === 'user' ? ('user' as const) : ('assistant' as const),
                content: m.content,
                images: validImages && validImages.length > 0 ? validImages : undefined,
              }
            }),
          )
          athenaOrchestrator.setCurrentSessionId(sessionId)
        } else {
          athenaOrchestrator.clearContext()
          athenaOrchestrator.setCurrentSessionId(sessionId)
        }
      } else if (!sessionId) {
        athenaOrchestrator.clearContext()
      }
      return await athenaOrchestrator.sendMessage(message, images)
    } catch (err: unknown) {
      if (err instanceof Error) {
        return `Error: ${err.message}`
      }
      return 'Error: An unknown error occurred'
    }
  },
)

// Session IPC
ipcMain.handle('session:create', async (_event, title?: string) => {
  try {
    const { createSession } = await import('./sessionStore')
    return await createSession(title)
  } catch (err) {
    console.error('[session:create] Error:', err)
    throw err
  }
})

ipcMain.handle('session:get', async (_event, id: string) => {
  const { getSessionWithImages } = await import('./sessionStore')
  return await getSessionWithImages(id)
})

ipcMain.handle('session:update', async (_event, id: string, updates: any) => {
  const { updateSession } = await import('./sessionStore')
  return await updateSession(id, updates)
})

ipcMain.handle('session:addMessage', async (_event, sessionId: string, message: any) => {
  const { addMessageToSession } = await import('./sessionStore')
  return await addMessageToSession(sessionId, message)
})

ipcMain.handle('session:delete', async (_event, id: string) => {
  const { deleteSession } = await import('./sessionStore')
  return await deleteSession(id)
})

ipcMain.handle('session:list', async () => {
  const { listSessions } = await import('./sessionStore')
  return await listSessions()
})

app.whenReady().then(async () => {
  const ptyMgr = await import('./ptyManager')

  // Register plugin IPC handlers eagerly before window creation
  // to prevent "No handler registered for plugin:list" race condition
  const { registerPluginIpcHandlers } = await import('./services/plugin-manager')
  registerPluginIpcHandlers()

  ipcMain.handle(
    'pty:spawn',
    async (_event, id: string, cwd: string, shell: string, agentCmd?: string) => {
      try {
        if (!mainWindow) return { success: false, error: 'No main window' }
        ptyMgr.spawn(id, cwd, shell, agentCmd, mainWindow)
        return { success: true }
      } catch (err: unknown) {
        if (err instanceof Error) {
          return { success: false, error: err.message }
        }
        return { success: false, error: 'An unknown error occurred' }
      }
    },
  )

  ipcMain.handle(
    'pty:spawnAgent',
    async (
      _event,
      id: string,
      cwd: string,
      shell: string,
      agentCmd: string | undefined,
      agentType: string,
      paneId?: string,
      sessionId?: string,
    ) => {
      try {
        if (!mainWindow) return { success: false, error: 'No main window' }
        ptyMgr.spawnAgent(id, cwd, shell, agentCmd, mainWindow, agentType as any, paneId, sessionId)
        return { success: true }
      } catch (err: unknown) {
        if (err instanceof Error) {
          return { success: false, error: err.message }
        }
        return { success: false, error: 'An unknown error occurred' }
      }
    },
  )

  ipcMain.handle('pty:getHistory', async (_event, id: string) => {
    return ptyMgr.getHistory(id)
  })

  ipcMain.handle('pty:hasSession', async (_event, id: string) => {
    return ptyMgr.hasSession(id)
  })

  ipcMain.handle('pty:isReady', async (_event, id: string) => {
    return ptyMgr.isReady(id)
  })

  ipcMain.handle('pty:getCwd', async (_event, id: string) => {
    return ptyMgr.getCwd(id)
  })

  ipcMain.on('pty:write', (_event, id: string, data: string) => {
    ptyMgr.write(id, data)
  })

  ipcMain.on('pty:resize', (_event, id: string, cols: number, rows: number) => {
    ptyMgr.resize(id, cols, rows)
  })

  ipcMain.on('pty:kill', (_event, id: string) => {
    ptyMgr.kill(id)
  })

  createWindow()

  const { initBrowserManager } = await import('./browserManager')
  if (mainWindow) initBrowserManager(mainWindow)

  const { initSwarmCoordinator } = await import('./swarmCoordinator')
  if (mainWindow) initSwarmCoordinator(mainWindow)

  const { initMcpServer } = await import('./mcpServer')
  if (mainWindow) initMcpServer(mainWindow)

  const { initPluginManager, initAgentComms, shutdownAgentComms } = await import('./services')
  if (mainWindow) await initPluginManager(mainWindow)
  if (mainWindow) await initAgentComms(mainWindow)

  const { initOutputCapture, onPtySpawn, onPtyData, onPtyExit, shutdownOutputCapture } =
    await import('./services/output-capture')
  if (mainWindow) await initOutputCapture(mainWindow)

  const ptyMgrHooks = await import('./ptyManager')
  ptyMgrHooks.setOutputCaptureHooks({
    onSpawn: onPtySpawn,
    onData: onPtyData,
    onExit: onPtyExit,
  })

  const { setShellIntegrationHooks } = await import('./ptyManager')
  setShellIntegrationHooks({
    onEvent: (event) => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send(`shell-integration:${event.paneId}`, event)
      }
    },
  })

  const { initPluginHost } = await import('./pluginHost')
  if (mainWindow) await initPluginHost(mainWindow)

  ipcMain.on('plugin:respondToInput', async (_event, requestId: string, response: string) => {
    try {
      const { respondToInputRequest } = await import('./services/agent-comms')
      respondToInputRequest(requestId, response)
    } catch {}
  })

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

let isQuitting = false
app.on('before-quit', async (event) => {
  if (!isQuitting) {
    event.preventDefault()
    isQuitting = true
    try {
      const ptyMgr = await import('./ptyManager')
      await ptyMgr.gracefulShutdown()
    } catch (e) {
      // Ignore cleanup errors during teardown
    }
    try {
      const { shutdownAgentComms } = await import('./services')
      await shutdownAgentComms()
    } catch (e) {
      // Ignore cleanup errors during teardown
    }
    try {
      const { shutdownOutputCapture } = await import('./services/output-capture')
      shutdownOutputCapture()
    } catch (e) {
      // Ignore cleanup errors during teardown
    }
    app.quit() // Actually quit after the shutdown sequence completes
  }
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
