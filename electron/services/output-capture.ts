import { BrowserWindow, ipcMain } from 'electron'
import type { AgentType } from '../../src/types/workspace'
import {
  appendOutput,
  registerPane,
  unregisterPane,
  initOutputBufferService,
  shutdownOutputBufferService,
} from './output-buffer-service'

let mainWindowRef: BrowserWindow | null = null
let initialized = false

export function onPtySpawn(paneId: string, agentType: string = 'shell'): void {
  registerPane(paneId, agentType)
}

export function onPtyData(paneId: string, data: string): void {
  appendOutput(paneId, data)
}

export function onPtyExit(paneId: string): void {
  unregisterPane(paneId)
}

export function captureStderr(childPaneId: string, data: string): void {
  appendOutput(childPaneId, data)
}

export async function initOutputCapture(mainWindow: BrowserWindow): Promise<void> {
  if (initialized) return
  mainWindowRef = mainWindow
  initialized = true

  await initOutputBufferService(mainWindow)

  ipcMain.handle(
    'output-capture:registerPane',
    async (_event, paneId: string, agentType?: string) => {
      registerPane(paneId, agentType || 'shell')
      return { success: true }
    },
  )
}

export function shutdownOutputCapture(): void {
  shutdownOutputBufferService()
  initialized = false
  mainWindowRef = null
}
