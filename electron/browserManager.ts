import { BrowserWindow, WebContentsView, ipcMain } from 'electron'

let browserView: WebContentsView | null = null
let mainWindowRef: BrowserWindow | null = null

export function initBrowserManager(mainWindow: BrowserWindow): void {
  mainWindowRef = mainWindow

  ipcMain.on(
    'browser:show',
    (_event, bounds: { x: number; y: number; width: number; height: number }) => {
      if (!mainWindowRef) return

      if (browserView) {
        mainWindowRef.contentView.removeChildView(browserView)
        browserView.webContents.close()
      }

      browserView = new WebContentsView({
        webPreferences: {
          contextIsolation: true,
          nodeIntegration: false,
        },
      })

      mainWindowRef.contentView.addChildView(browserView)
      browserView.setBounds({
        x: Math.round(bounds.x),
        y: Math.round(bounds.y),
        width: Math.round(bounds.width),
        height: Math.round(bounds.height),
      })

      browserView.webContents.loadURL('https://www.google.com')

      browserView.webContents.on('did-navigate', (_e, url) => {
        mainWindowRef?.webContents.send('browser:urlChange', url)
      })

      browserView.webContents.on('did-navigate-in-page', (_e, url) => {
        mainWindowRef?.webContents.send('browser:urlChange', url)
      })

      browserView.webContents.on('page-title-updated', (_e, title) => {
        mainWindowRef?.webContents.send('browser:titleChange', title)
      })
    },
  )

  ipcMain.on('browser:hide', () => {
    if (browserView && mainWindowRef) {
      mainWindowRef.contentView.removeChildView(browserView)
      browserView.webContents.close()
      browserView = null
    }
  })

  ipcMain.on('browser:navigate', (_event, url: string) => {
    if (!browserView) return
    let finalUrl = url.trim()
    if (!finalUrl.startsWith('http://') && !finalUrl.startsWith('https://')) {
      finalUrl = 'https://' + finalUrl
    }
    try {
      new URL(finalUrl)
      browserView.webContents.loadURL(finalUrl)
    } catch {
      mainWindowRef?.webContents.send('browser:titleChange', `Could not load: ${url}`)
    }
  })

  ipcMain.on('browser:back', () => {
    if (browserView?.webContents.canGoBack()) {
      browserView.webContents.goBack()
    }
  })

  ipcMain.on('browser:forward', () => {
    if (browserView?.webContents.canGoForward()) {
      browserView.webContents.goForward()
    }
  })

  ipcMain.on('browser:reload', () => {
    browserView?.webContents.reload()
  })

  ipcMain.on(
    'browser:setBounds',
    (_event, bounds: { x: number; y: number; width: number; height: number }) => {
      if (browserView) {
        browserView.setBounds({
          x: Math.round(bounds.x),
          y: Math.round(bounds.y),
          width: Math.round(bounds.width),
          height: Math.round(bounds.height),
        })
      }
    },
  )
}
