import { describe, it, expect, vi, beforeEach } from 'vitest'
import type { PluginManifest, PluginEntry, PluginStatus } from '../plugin-manager'

const mockStore = {
  _data: {} as Record<string, unknown>,
  get(key: string) {
    return this._data[key]
  },
  set(key: string, value: unknown) {
    this._data[key] = value
  },
  _reset() {
    this._data = {}
  },
}

vi.mock('../../storeUtil', () => ({
  getStore: () => Promise.resolve(mockStore),
}))

vi.mock('electron', () => {
  const send = vi.fn()
  return {
    app: { whenReady: () => Promise.resolve() },
    BrowserWindow: vi.fn(function (this: any) {
      this.webContents = { send }
      return this
    }),
    ipcMain: {
      handle: vi.fn(),
      on: vi.fn(),
    },
  }
})

const sampleManifest: PluginManifest = {
  id: 'test-plugin',
  name: 'Test Plugin',
  version: '1.0.0',
  description: 'A test plugin',
  author: 'Test Author',
  entryPoint: './index.js',
  permissions: ['terminal', 'notifications'],
  mcpConfig: {
    command: 'node',
    args: ['./plugin.js'],
    env: { DEBUG: '1' },
  },
}

describe('Plugin Manager Types', () => {
  describe('PluginManifest', () => {
    it('should have required fields', () => {
      expect(sampleManifest.id).toBe('test-plugin')
      expect(sampleManifest.name).toBe('Test Plugin')
      expect(sampleManifest.version).toBe('1.0.0')
      expect(sampleManifest.permissions).toContain('terminal')
    })

    it('should have optional mcpConfig', () => {
      expect(sampleManifest.mcpConfig).toBeDefined()
      expect(sampleManifest.mcpConfig!.command).toBe('node')
      expect(sampleManifest.mcpConfig!.args).toEqual(['./plugin.js'])
    })

    it('should allow manifest without mcpConfig', () => {
      const minimal: PluginManifest = {
        id: 'minimal',
        name: 'Minimal',
        version: '0.1.0',
        description: 'No MCP',
        author: 'Dev',
        entryPoint: './index.js',
        permissions: [],
      }
      expect(minimal.mcpConfig).toBeUndefined()
    })
  })

  describe('PluginPermission', () => {
    it('should accept all valid permissions', () => {
      const perms = ['terminal', 'filesystem', 'notifications', 'clipboard', 'network'] as const
      expect(perms).toHaveLength(5)
    })
  })

  describe('PluginStatus', () => {
    it('should accept all valid statuses', () => {
      const statuses: PluginStatus[] = ['installed', 'enabled', 'disabled', 'error']
      expect(statuses).toHaveLength(4)
    })
  })

  describe('PluginEntry', () => {
    it('should construct a valid entry', () => {
      const entry: PluginEntry = {
        manifest: sampleManifest,
        status: 'installed',
        installedAt: Date.now(),
        config: {},
      }
      expect(entry.status).toBe('installed')
      expect(entry.error).toBeUndefined()
      expect(entry.lastEnabledAt).toBeUndefined()
    })

    it('should support error state', () => {
      const entry: PluginEntry = {
        manifest: sampleManifest,
        status: 'error',
        installedAt: Date.now(),
        config: {},
        error: 'Failed to start',
      }
      expect(entry.error).toBe('Failed to start')
    })
  })
})

describe('Plugin Manager Module', () => {
  beforeEach(() => {
    mockStore._reset()
    vi.clearAllMocks()
  })

  it('should export initPluginManager function', async () => {
    const mod = await import('../plugin-manager')
    expect(typeof mod.initPluginManager).toBe('function')
  })

  it('should export helper functions', async () => {
    const mod = await import('../plugin-manager')
    expect(typeof mod.getPluginRegistry).toBe('function')
    expect(typeof mod.getEnabledPlugins).toBe('function')
    expect(typeof mod.getPluginById).toBe('function')
    expect(typeof mod.setPluginStatus).toBe('function')
  })

  it('initPluginManager should register IPC handlers', async () => {
    const { ipcMain } = await import('electron')
    const { initPluginManager } = await import('../plugin-manager')
    const { BrowserWindow } = await import('electron')
    const mockWindow = new (BrowserWindow as any)()

    await initPluginManager(mockWindow as any)

    const handleCalls = (ipcMain.handle as ReturnType<typeof vi.fn>).mock.calls
    const handlerChannels = handleCalls.map((c: any[]) => c[0])

    const expectedChannels = [
      'plugin:list',
      'plugin:get',
      'plugin:register',
      'plugin:unregister',
      'plugin:enable',
      'plugin:disable',
      'plugin:getConfig',
      'plugin:setConfig',
      'plugin:setError',
    ]

    for (const ch of expectedChannels) {
      expect(handlerChannels).toContain(ch)
    }
  })

  it('getPluginRegistry should return an object', async () => {
    const { getPluginRegistry } = await import('../plugin-manager')
    const registry = getPluginRegistry()
    expect(typeof registry).toBe('object')
  })

  it('getEnabledPlugins should return an array', async () => {
    const { getEnabledPlugins } = await import('../plugin-manager')
    const enabled = getEnabledPlugins()
    expect(Array.isArray(enabled)).toBe(true)
  })

  it('getPluginById should return undefined for unknown plugin', async () => {
    const { getPluginById } = await import('../plugin-manager')
    const result = getPluginById('nonexistent')
    expect(result).toBeUndefined()
  })
})
