import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

vi.mock('electron', () => ({
  BrowserWindow: vi.fn(function (this: any) {
    this.webContents = {
      send: vi.fn(),
    }
    this.isDestroyed = vi.fn(() => false)
  }),
  ipcMain: {
    handle: vi.fn(),
    on: vi.fn(),
    once: vi.fn(),
    removeHandler: vi.fn(),
  },
  app: { on: vi.fn() },
}))

import {
  appendOutput,
  registerPane,
  unregisterPane,
  getOutput,
  getOutputSince,
  getAgentList,
  subscribeToPane,
  getPaneBufferInfo,
  clearPaneBuffer,
  shutdownOutputBufferService,
} from '../../electron/services/output-buffer-service'

describe('output-buffer-service', () => {
  beforeEach(() => {
    shutdownOutputBufferService()
  })

  afterEach(() => {
    shutdownOutputBufferService()
  })

  describe('registerPane + getAgentList', () => {
    it('creates a buffer on registerPane and lists it', () => {
      registerPane('pane-1', 'claude')
      const agents = getAgentList()
      expect(agents).toHaveLength(1)
      expect(agents[0].paneId).toBe('pane-1')
      expect(agents[0].agentType).toBe('claude')
    })

    it('returns empty list when no panes registered', () => {
      expect(getAgentList()).toEqual([])
    })

    it('defaults agentType to shell', () => {
      registerPane('pane-2')
      const agents = getAgentList()
      expect(agents[0].agentType).toBe('shell')
    })

    it('idempotent — re-registering same pane is no-op', () => {
      registerPane('pane-1', 'opencode')
      registerPane('pane-1', 'opencode')
      expect(getAgentList()).toHaveLength(1)
    })
  })

  describe('appendOutput + getOutput', () => {
    it('appends output and retrieves it', () => {
      registerPane('p1')
      appendOutput('p1', 'line 1\nline 2\nline 3')
      const lines = getOutput('p1')
      expect(lines.length).toBeGreaterThanOrEqual(3)
    })

    it('auto-creates buffer on append if not registered', () => {
      appendOutput('auto-pane', 'hello')
      const lines = getOutput('auto-pane')
      expect(lines.length).toBeGreaterThanOrEqual(1)
      expect(lines[0].text).toBe('hello')
    })

    it('respects limit option', () => {
      registerPane('p1')
      for (let i = 0; i < 50; i++) appendOutput('p1', `line ${i}`)
      const lines = getOutput('p1', { limit: 5 })
      expect(lines).toHaveLength(5)
    })

    it('respects sinceLine option', () => {
      registerPane('p1')
      for (let i = 0; i < 10; i++) appendOutput('p1', `line ${i}`)
      const lines = getOutput('p1', { sinceLine: 8 })
      expect(lines.length).toBeGreaterThan(0)
      expect(lines.every((l) => l.lineNum > 8)).toBe(true)
    })

    it('respects sinceTime option', async () => {
      registerPane('p1')
      appendOutput('p1', 'old')
      await new Promise((r) => setTimeout(r, 5))
      const after = Date.now()
      await new Promise((r) => setTimeout(r, 5))
      appendOutput('p1', 'new')
      const lines = getOutput('p1', { sinceTime: after })
      expect(lines.length).toBeGreaterThanOrEqual(1)
    })

    it('returns empty for unknown pane', () => {
      expect(getOutput('no-such-pane')).toEqual([])
    })
  })

  describe('getOutputSince', () => {
    it('delegates to getOutput with sinceTime', async () => {
      registerPane('p1')
      appendOutput('p1', 'a')
      await new Promise((r) => setTimeout(r, 5))
      const ts = Date.now()
      await new Promise((r) => setTimeout(r, 5))
      appendOutput('p1', 'b')
      const lines = getOutputSince('p1', ts)
      expect(lines.length).toBeGreaterThanOrEqual(1)
    })
  })

  describe('stripAnsi', () => {
    it('strips ANSI escape codes from output', () => {
      registerPane('ansi')
      appendOutput('ansi', '\x1b[32mgreen text\x1b[0m')
      const lines = getOutput('ansi')
      expect(lines[0].text).not.toContain('\x1b')
      expect(lines[0].text).toContain('green text')
    })
  })

  describe('subscribeToPane', () => {
    it('receives lines via subscriber callback', () => {
      registerPane('p1')
      const received: any[] = []
      const unsub = subscribeToPane('p1', (line) => received.push(line))
      appendOutput('p1', 'event 1')
      appendOutput('p1', 'event 2')
      expect(received.length).toBeGreaterThanOrEqual(2)
      unsub()
    })

    it('stops receiving after unsubscribe', () => {
      registerPane('p1')
      const received: any[] = []
      const unsub = subscribeToPane('p1', (line) => received.push(line))
      appendOutput('p1', 'before')
      unsub()
      appendOutput('p1', 'after')
      expect(received.some((l) => l.text === 'before')).toBe(true)
      expect(received.some((l) => l.text === 'after')).toBe(false)
    })

    it('returns no-op for unknown pane', () => {
      const unsub = subscribeToPane('unknown', () => {})
      expect(typeof unsub).toBe('function')
      unsub()
    })
  })

  describe('getPaneBufferInfo', () => {
    it('returns buffer info for registered pane', () => {
      registerPane('p1', 'claude')
      appendOutput('p1', 'some data')
      const info = getPaneBufferInfo('p1')
      expect(info).not.toBeNull()
      expect(info!.paneId).toBe('p1')
      expect(info!.agentType).toBe('claude')
      expect(info!.lineCount).toBeGreaterThan(0)
      expect(info!.totalLines).toBeGreaterThan(0)
    })

    it('returns null for unknown pane', () => {
      expect(getPaneBufferInfo('unknown')).toBeNull()
    })
  })

  describe('clearPaneBuffer', () => {
    it('clears buffer lines but keeps pane registered', () => {
      registerPane('p1')
      appendOutput('p1', 'data')
      const cleared = clearPaneBuffer('p1')
      expect(cleared).toBe(true)
      expect(getOutput('p1')).toEqual([])
    })

    it('returns false for unknown pane', () => {
      expect(clearPaneBuffer('unknown')).toBe(false)
    })
  })

  describe('unregisterPane', () => {
    it('removes pane and clears subscribers', () => {
      registerPane('p1')
      appendOutput('p1', 'x')
      unregisterPane('p1')
      expect(getOutput('p1')).toEqual([])
      expect(getAgentList()).toHaveLength(0)
    })
  })

  describe('buffer limits', () => {
    it('trims at 5000 lines', () => {
      registerPane('big')
      for (let i = 0; i < 5500; i++) {
        appendOutput('big', `line ${i}`)
      }
      const lines = getOutput('big')
      expect(lines.length).toBeLessThanOrEqual(5000)
    })
  })
})
