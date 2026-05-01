import { describe, it, expect, vi, beforeEach } from 'vitest'
import { readFileSync } from 'fs'
import { resolve } from 'path'

const projectRoot = resolve(__dirname, '..')
const pluginManagerPath = resolve(projectRoot, 'electron/services/plugin-manager.ts')
const preloadPath = resolve(projectRoot, 'electron/preload.ts')

const pluginManagerSource = readFileSync(pluginManagerPath, 'utf-8')
const preloadSource = readFileSync(preloadPath, 'utf-8')

function extractIpcHandles(source: string): string[] {
  const matches = [...source.matchAll(/ipcMain\.handle\(['"`]([^'"`]+)['"`]/g)]
  return matches.map((m) => m[1])
}

function extractIpcInvokes(source: string): string[] {
  const matches = [...source.matchAll(/ipcRenderer\.invoke\(['"`]([^'"`]+)['"`]/g)]
  return matches.map((m) => m[1])
}

function extractWebContentsSends(source: string): string[] {
  const matches = [...source.matchAll(/webContents\.send\(['"`]([^'"`]+)['"`]/g)]
  return matches.map((m) => m[1])
}

function extractIpcOnListeners(source: string): string[] {
  const matches = [...source.matchAll(/ipcRenderer\.on\(['"`]([^'"`]+)['"`]/g)]
  return matches.map((m) => m[1])
}

describe('IPC Channel Alignment', () => {
  it('preload invoke channels should match plugin-manager handle channels', () => {
    const handles = extractIpcHandles(pluginManagerSource)
    const invokes = extractIpcInvokes(preloadSource)

    const pluginInvokes = invokes.filter((ch) => ch.startsWith('plugin:'))
    const pluginHandles = handles.filter((ch) => ch.startsWith('plugin:'))

    for (const channel of pluginInvokes) {
      expect(pluginHandles).toContain(channel)
    }
  })

  it('preload on channels should match plugin-manager send channels', () => {
    const sends = extractWebContentsSends(pluginManagerSource)
    const listeners = extractIpcOnListeners(preloadSource)

    const pluginListeners = listeners.filter(
      (ch) => ch.startsWith('plugin:') && ch !== 'plugin:event',
    )
    const pluginSends = [...new Set(sends)]

    for (const channel of pluginListeners) {
      expect(pluginSends).toContain(channel)
    }
  })

  it('no preload channels should use plural plugins: prefix', () => {
    const invokes = extractIpcInvokes(preloadSource)
    const listeners = extractIpcOnListeners(preloadSource)

    const pluralInvokes = invokes.filter((ch) => ch.startsWith('plugins:'))
    const pluralListeners = listeners.filter((ch) => ch.startsWith('plugins:'))

    expect(pluralInvokes).toHaveLength(0)
    expect(pluralListeners).toHaveLength(0)
  })
})
