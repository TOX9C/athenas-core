import { describe, it, expect, beforeEach } from 'vitest'
import { OutputBufferManager } from '../src/output-buffer.js'
import type { OutputEntry } from '../src/types/index.js'

describe('OutputBufferManager', () => {
  let manager: OutputBufferManager

  beforeEach(() => {
    manager = new OutputBufferManager()
  })

  describe('append', () => {
    it('appends an entry and returns it with correct metadata', () => {
      const entry = manager.append('pane-1', 'hello world')
      expect(entry.content).toBe('hello world')
      expect(entry.lineNumber).toBe(1)
      expect(entry.isStderr).toBe(false)
      expect(entry.timestamp).toBeGreaterThan(0)
    })

    it('increments line numbers across multiple appends', () => {
      manager.append('pane-1', 'line 1')
      manager.append('pane-1', 'line 2')
      const entry3 = manager.append('pane-1', 'line 3')
      expect(entry3.lineNumber).toBe(3)
    })

    it('tracks stderr flag', () => {
      const entry = manager.append('pane-1', 'error msg', true)
      expect(entry.isStderr).toBe(true)
    })

    it('maintains separate line counters per pane', () => {
      manager.append('pane-1', 'a')
      manager.append('pane-1', 'b')
      const entry = manager.append('pane-2', 'x')
      expect(entry.lineNumber).toBe(1)
    })

    it('trims buffer when exceeding max lines', () => {
      const smallManager = new OutputBufferManager({ maxLinesPerPane: 5 })
      for (let i = 0; i < 10; i++) {
        smallManager.append('pane-1', `line ${i}`)
      }
      const entries = smallManager.read('pane-1')
      expect(entries).toHaveLength(5)
      expect(entries[0].lineNumber).toBe(6)
      expect(entries[4].lineNumber).toBe(10)
    })
  })

  describe('read', () => {
    it('returns empty array for unknown pane', () => {
      expect(manager.read('unknown')).toEqual([])
    })

    it('returns full buffer', () => {
      manager.append('pane-1', 'a')
      manager.append('pane-1', 'b')
      manager.append('pane-1', 'c')
      const entries = manager.read('pane-1')
      expect(entries).toHaveLength(3)
    })

    it('returns last N lines', () => {
      for (let i = 0; i < 10; i++) {
        manager.append('pane-1', `line ${i}`)
      }
      const entries = manager.read('pane-1', { lines: 3 })
      expect(entries).toHaveLength(3)
      expect(entries[0].content).toBe('line 7')
      expect(entries[2].content).toBe('line 9')
    })

    it('filters by sinceTimestamp', () => {
      manager.append('pane-1', 'old line')
      const afterFirst = Date.now()
      manager.append('pane-1', 'new line 1')
      manager.append('pane-1', 'new line 2')

      const entries = manager.read('pane-1', { sinceTimestamp: afterFirst })
      expect(entries.length).toBeGreaterThanOrEqual(2)
    })

    it('combines lines and sinceTimestamp', () => {
      for (let i = 0; i < 20; i++) {
        manager.append('pane-1', `line ${i}`)
      }
      const entries = manager.read('pane-1', { lines: 5, sinceTimestamp: 0 })
      expect(entries).toHaveLength(5)
    })
  })

  describe('readSince', () => {
    it('returns empty for unknown pane', () => {
      expect(manager.readSince('unknown')).toEqual([])
    })

    it('filters by sinceLine', () => {
      for (let i = 0; i < 10; i++) {
        manager.append('pane-1', `line ${i}`)
      }
      const entries = manager.readSince('pane-1', { sinceLine: 7 })
      expect(entries).toHaveLength(3)
      expect(entries[0].lineNumber).toBe(8)
      expect(entries[2].lineNumber).toBe(10)
    })

    it('filters by sinceTimestamp (exclusive)', () => {
      const entry1 = manager.append('pane-1', 'first')
      const ts = entry1.timestamp - 1
      manager.append('pane-1', 'second')
      manager.append('pane-1', 'third')

      const entries = manager.readSince('pane-1', { sinceTimestamp: ts })
      expect(entries.length).toBeGreaterThanOrEqual(3)
    })

    it('excludes entries at or before sinceTimestamp', () => {
      const entry1 = manager.append('pane-1', 'first')
      manager.append('pane-1', 'second')

      const entries = manager.readSince('pane-1', { sinceTimestamp: entry1.timestamp })
      expect(entries.every((e) => e.lineNumber > entry1.lineNumber)).toBe(true)
    })

    it('combines sinceLine and sinceTimestamp', () => {
      for (let i = 0; i < 10; i++) {
        manager.append('pane-1', `line ${i}`)
      }
      const entries = manager.readSince('pane-1', { sinceLine: 3, sinceTimestamp: 0 })
      expect(entries.length).toBe(7)
    })
  })

  describe('clear', () => {
    it('removes all entries for a pane', () => {
      manager.append('pane-1', 'a')
      manager.append('pane-1', 'b')
      manager.clear('pane-1')
      expect(manager.read('pane-1')).toEqual([])
    })

    it('resets line counter', () => {
      manager.append('pane-1', 'a')
      manager.append('pane-1', 'b')
      manager.clear('pane-1')
      const entry = manager.append('pane-1', 'c')
      expect(entry.lineNumber).toBe(1)
    })

    it('does not affect other panes', () => {
      manager.append('pane-1', 'a')
      manager.append('pane-2', 'x')
      manager.clear('pane-1')
      expect(manager.read('pane-2')).toHaveLength(1)
    })
  })

  describe('utility methods', () => {
    it('getPaneIds returns all known pane IDs', () => {
      manager.append('a', 'x')
      manager.append('b', 'y')
      manager.append('c', 'z')
      expect(manager.getPaneIds()).toEqual(['a', 'b', 'c'])
    })

    it('getLineCount returns total lines written', () => {
      manager.append('pane-1', 'a')
      manager.append('pane-1', 'b')
      expect(manager.getLineCount('pane-1')).toBe(2)
    })

    it('getBufferLength returns current buffer size', () => {
      const smallManager = new OutputBufferManager({ maxLinesPerPane: 3 })
      for (let i = 0; i < 5; i++) {
        smallManager.append('pane-1', `line ${i}`)
      }
      expect(smallManager.getBufferLength('pane-1')).toBe(3)
    })
  })

  describe('subscribe / unsubscribe', () => {
    it('receives entries via subscription', () => {
      const received: OutputEntry[] = []
      const sub = manager.subscribe('pane-1', (entry) => received.push(entry))

      manager.append('pane-1', 'hello')
      manager.append('pane-1', 'world')

      expect(received).toHaveLength(2)
      expect(received[0].content).toBe('hello')

      manager.unsubscribe(sub)
    })

    it('does not receive entries after unsubscribe', () => {
      const received: OutputEntry[] = []
      const sub = manager.subscribe('pane-1', (entry) => received.push(entry))

      manager.append('pane-1', 'before')
      manager.unsubscribe(sub)
      manager.append('pane-1', 'after')

      expect(received).toHaveLength(1)
    })

    it('does not receive entries for different pane', () => {
      const received: OutputEntry[] = []
      manager.subscribe('pane-1', (entry) => received.push(entry))

      manager.append('pane-2', 'other pane')

      expect(received).toHaveLength(0)
    })

    it('getActiveSubscriptions returns correct count', () => {
      expect(manager.getActiveSubscriptions('pane-1')).toBe(0)
      const sub1 = manager.subscribe('pane-1', () => {})
      expect(manager.getActiveSubscriptions('pane-1')).toBe(1)
      const sub2 = manager.subscribe('pane-1', () => {})
      expect(manager.getActiveSubscriptions('pane-1')).toBe(2)
      manager.unsubscribe(sub1)
      expect(manager.getActiveSubscriptions('pane-1')).toBe(1)
      manager.unsubscribe(sub2)
      expect(manager.getActiveSubscriptions('pane-1')).toBe(0)
    })

    it('subscription has correct metadata', () => {
      const sub = manager.subscribe('pane-1', () => {})
      expect(sub.paneId).toBe('pane-1')
      expect(sub.active).toBe(true)
      expect(sub.id).toMatch(/^sub-/)
      manager.unsubscribe(sub)
      expect(sub.active).toBe(false)
    })

    it('handles subscriber errors gracefully', () => {
      const goodReceived: OutputEntry[] = []
      const badSub = manager.subscribe('pane-1', () => {
        throw new Error('boom')
      })
      const goodSub = manager.subscribe('pane-1', (entry) => goodReceived.push(entry))

      manager.append('pane-1', 'test')

      expect(goodReceived).toHaveLength(1)
      manager.unsubscribe(badSub)
      manager.unsubscribe(goodSub)
    })
  })
})
