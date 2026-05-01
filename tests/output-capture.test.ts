import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

vi.mock('electron', () => ({
  BrowserWindow: vi.fn(function (this: any) {
    this.webContents = { send: vi.fn() }
    this.isDestroyed = vi.fn(() => false)
  }),
  ipcMain: {
    handle: vi.fn(),
    on: vi.fn(),
    once: vi.fn(),
  },
  app: { on: vi.fn() },
}))

import {
  onPtySpawn,
  onPtyData,
  onPtyExit,
  captureStderr,
  shutdownOutputCapture,
} from '../../electron/services/output-capture'
import { getOutput, getAgentList } from '../../electron/services/output-buffer-service'

describe('output-capture', () => {
  beforeEach(() => {
    shutdownOutputCapture()
  })

  afterEach(() => {
    shutdownOutputCapture()
  })

  describe('onPtySpawn', () => {
    it('registers a pane with agent type', () => {
      onPtySpawn('pty-1', 'opencode')
      const agents = getAgentList()
      expect(agents).toHaveLength(1)
      expect(agents[0].paneId).toBe('pty-1')
      expect(agents[0].agentType).toBe('opencode')
    })

    it('defaults to shell agent type', () => {
      onPtySpawn('pty-2')
      const agents = getAgentList()
      expect(agents[0].agentType).toBe('shell')
    })
  })

  describe('onPtyData', () => {
    it('appends data to pane buffer', () => {
      onPtySpawn('pty-1')
      onPtyData('pty-1', 'hello from terminal')
      const lines = getOutput('pty-1')
      expect(lines.length).toBeGreaterThanOrEqual(1)
      expect(lines.some((l) => l.text.includes('hello from terminal'))).toBe(true)
    })

    it('handles multiline data', () => {
      onPtySpawn('pty-1')
      onPtyData('pty-1', 'line 1\nline 2\nline 3')
      const lines = getOutput('pty-1')
      expect(lines.length).toBeGreaterThanOrEqual(3)
    })
  })

  describe('onPtyExit', () => {
    it('unregisters pane on exit', () => {
      onPtySpawn('pty-1')
      onPtyData('pty-1', 'some data')
      onPtyExit('pty-1')
      expect(getOutput('pty-1')).toEqual([])
      expect(getAgentList()).toHaveLength(0)
    })
  })

  describe('captureStderr', () => {
    it('appends stderr data to buffer', () => {
      onPtySpawn('pty-1')
      captureStderr('pty-1', 'error output')
      const lines = getOutput('pty-1')
      expect(lines.some((l) => l.text.includes('error output'))).toBe(true)
    })
  })

  describe('pty lifecycle integration', () => {
    it('spawn → data → exit cycle works correctly', () => {
      onPtySpawn('pty-cycle', 'claude-code')
      onPtyData('pty-cycle', 'working...')
      onPtyData('pty-cycle', 'done!')
      const lines = getOutput('pty-cycle')
      expect(lines.length).toBeGreaterThanOrEqual(2)

      onPtyExit('pty-cycle')
      expect(getAgentList()).toHaveLength(0)
    })
  })
})
