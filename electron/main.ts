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
      sandbox: false
    }
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
    properties: ['openDirectory']
  })
  if (result.canceled || result.filePaths.length === 0) return null
  return result.filePaths[0]
})

ipcMain.handle('fs:readTree', async (_event, dir: string) => {
  try {
    const { readTree } = await import('./fileSystem')
    return await readTree(dir)
  } catch (err: any) {
    return { success: false, error: err.message }
  }
})

ipcMain.handle('fs:readFile', async (_event, filePath: string) => {
  try {
    const { readFileContent } = await import('./fileSystem')
    return await readFileContent(filePath)
  } catch (err: any) {
    return { success: false, error: err.message }
  }
})

ipcMain.handle('fs:writeFile', async (_event, filePath: string, content: string) => {
  try {
    const { writeFileContent } = await import('./fileSystem')
    await writeFileContent(filePath, content)
    return { success: true }
  } catch (err: any) {
    return { success: false, error: err.message }
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

// Store IPC (electron-store)
let storeInstance: any = null
async function getStore() {
  if (!storeInstance) {
    const { default: Store } = await import('electron-store')
    storeInstance = new Store()
  }
  return storeInstance
}

ipcMain.handle('store:get', async (_event, key: string) => {
  const store = await getStore()
  return store.get(key)
})

ipcMain.handle('store:set', async (_event, key: string, value: any) => {
  const store = await getStore()
  store.set(key, value)
})

app.whenReady().then(async () => {
  const ptyMgr = await import('./ptyManager')

  ipcMain.handle('pty:spawn', async (_event, id: string, cwd: string, shell: string, agentCmd?: string) => {
    try {
      if (!mainWindow) return { success: false, error: 'No main window' }
      ptyMgr.spawn(id, cwd, shell, agentCmd, mainWindow)
      return { success: true }
    } catch (err: any) {
      return { success: false, error: err.message }
    }
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

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow()
    }
  })
})

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit()
  }
})
