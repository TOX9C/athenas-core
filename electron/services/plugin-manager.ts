import { BrowserWindow, ipcMain, app } from 'electron'
import { randomUUID } from 'crypto'
import { getStore } from '../storeUtil'

export interface PluginManifest {
  id: string
  name: string
  version: string
  description: string
  author: string
  entryPoint: string
  permissions: PluginPermission[]
  mcpConfig?: {
    command: string
    args: string[]
    env?: Record<string, string>
  }
}

export type PluginPermission = 'terminal' | 'filesystem' | 'notifications' | 'clipboard' | 'network'

export type PluginStatus = 'installed' | 'enabled' | 'disabled' | 'error'

export interface PluginEntry {
  manifest: PluginManifest
  status: PluginStatus
  installedAt: number
  lastEnabledAt?: number
  config: Record<string, unknown>
  error?: string
}

interface PluginRegistry {
  [pluginId: string]: PluginEntry
}

let mainWindowRef: BrowserWindow | null = null
let registry: PluginRegistry = {}

const STORE_KEY = 'plugins:registry'

async function loadRegistry(): Promise<PluginRegistry> {
  const store = await getStore()
  const saved = store.get(STORE_KEY) as PluginRegistry | undefined
  if (saved && typeof saved === 'object') {
    registry = saved
  }
  return registry
}

async function persistRegistry(): Promise<void> {
  const store = await getStore()
  store.set(STORE_KEY, registry)
}

function emitRegistryChange(): void {
  mainWindowRef?.webContents.send('plugin:registryUpdated', getPublicRegistry())
}

function getPublicRegistry(): Record<
  string,
  {
    name: string
    version: string
    status: PluginStatus
    description: string
    author: string
    config: Record<string, unknown>
    error?: string
  }
> {
  const out: Record<string, any> = {}
  for (const [id, entry] of Object.entries(registry)) {
    out[id] = {
      name: entry.manifest.name,
      version: entry.manifest.version,
      status: entry.status,
      description: entry.manifest.description,
      author: entry.manifest.author,
      config: entry.config,
      error: entry.error,
    }
  }
  return out
}

// Register all IPC handlers eagerly (before window creation)
// to prevent "No handler registered" race condition when
// renderer mounts and calls plugin:list before initPluginManager completes
export function registerPluginIpcHandlers(): void {
  ipcMain.handle('plugin:list', async () => {
    return getPublicRegistry()
  })

  ipcMain.handle('plugin:get', async (_event, pluginId: string) => {
    const entry = registry[pluginId]
    if (!entry) return null
    return {
      ...entry.manifest,
      id: pluginId,
      status: entry.status,
      config: entry.config,
      error: entry.error,
    }
  })

  ipcMain.handle('plugin:register', async (_event, manifest: PluginManifest) => {
    const id = manifest.id || randomUUID()
    if (registry[id] && registry[id].status !== 'disabled') {
      return { success: false, error: 'Plugin already registered and active. Disable first.' }
    }

    registry[id] = {
      manifest: { ...manifest, id },
      status: 'installed',
      installedAt: Date.now(),
      config: {},
    }

    await persistRegistry()
    emitRegistryChange()

    mainWindowRef?.webContents.send('plugin:registered', { id, name: manifest.name })

    return { success: true, id }
  })

  ipcMain.handle('plugin:unregister', async (_event, pluginId: string) => {
    if (!registry[pluginId]) {
      return { success: false, error: 'Plugin not found' }
    }

    const wasEnabled = registry[pluginId].status === 'enabled'
    delete registry[pluginId]

    await persistRegistry()
    emitRegistryChange()

    if (wasEnabled) {
      mainWindowRef?.webContents.send('plugin:disabled', { id: pluginId })
    }

    return { success: true }
  })

  ipcMain.handle('plugin:enable', async (_event, pluginId: string) => {
    const entry = registry[pluginId]
    if (!entry) return { success: false, error: 'Plugin not found' }
    if (entry.status === 'enabled') return { success: true }

    entry.status = 'enabled'
    entry.lastEnabledAt = Date.now()
    entry.error = undefined

    await persistRegistry()
    emitRegistryChange()

    mainWindowRef?.webContents.send('plugin:enabled', { id: pluginId, name: entry.manifest.name })

    return { success: true }
  })

  ipcMain.handle('plugin:disable', async (_event, pluginId: string) => {
    const entry = registry[pluginId]
    if (!entry) return { success: false, error: 'Plugin not found' }
    if (entry.status === 'disabled') return { success: true }

    const prev = entry.status
    entry.status = 'disabled'

    await persistRegistry()
    emitRegistryChange()

    if (prev === 'enabled') {
      mainWindowRef?.webContents.send('plugin:disabled', { id: pluginId })
    }

    return { success: true }
  })

  ipcMain.handle('plugin:getConfig', async (_event, pluginId: string) => {
    const entry = registry[pluginId]
    if (!entry) return null
    return entry.config
  })

  ipcMain.handle(
    'plugin:setConfig',
    async (_event, pluginId: string, config: Record<string, unknown>) => {
      const entry = registry[pluginId]
      if (!entry) return { success: false, error: 'Plugin not found' }

      entry.config = { ...entry.config, ...config }

      await persistRegistry()
      emitRegistryChange()

      mainWindowRef?.webContents.send('plugin:configured', { id: pluginId, config: entry.config })

      return { success: true }
    },
  )

  ipcMain.handle('plugin:setError', async (_event, pluginId: string, error: string) => {
    const entry = registry[pluginId]
    if (!entry) return { success: false, error: 'Plugin not found' }

    entry.status = 'error'
    entry.error = error

    await persistRegistry()
    emitRegistryChange()

    mainWindowRef?.webContents.send('plugin:error', { id: pluginId, error })

    return { success: true }
  })
}

export async function initPluginManager(mainWindow: BrowserWindow): Promise<void> {
  mainWindowRef = mainWindow
  await loadRegistry()
}

export function getPluginRegistry(): PluginRegistry {
  return registry
}

export function getEnabledPlugins(): PluginEntry[] {
  return Object.values(registry).filter((e) => e.status === 'enabled')
}

export function getPluginById(id: string): PluginEntry | undefined {
  return registry[id]
}

export function setPluginStatus(id: string, status: PluginStatus, error?: string): void {
  const entry = registry[id]
  if (!entry) return
  entry.status = status
  if (error) entry.error = error
  else entry.error = undefined
  persistRegistry().catch(() => {})
  emitRegistryChange()
}
