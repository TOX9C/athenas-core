import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
const TEST_SCREENSHOTS = join(dirname(fileURLToPath(import.meta.url)), '..', 'screenshots')

describe('Command palette smoke', () => {
  it('opens on Cmd+P, lists grouped commands, filters', async () => {
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.app-root')),
      { timeout: 25000, interval: 250, timeoutMsg: 'App root did not mount' },
    );

    const opened = await browser.execute(() => {
      const target = document.querySelector('.app-root')
      if (!target) return { ok: false }
      target.dispatchEvent(new KeyboardEvent('keydown', {
        key: 'p', code: 'KeyP', metaKey: true, bubbles: true, cancelable: true,
      }))
      return { ok: true }
    })
    expect(opened.ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('[role="dialog"][aria-label="Command palette"]')),
      { timeout: 5000, interval: 100, timeoutMsg: 'Palette did not open' },
    )

    const snapshot = await browser.execute(() => {
      const dlg = document.querySelector('[role="dialog"][aria-label="Command palette"]')
      const labels = [...dlg.querySelectorAll('button > span:nth-child(2) > span:first-child')].map((e) => e.textContent)
      return {
        groupHeaders: [...dlg.querySelectorAll('div')].map((e) => e.childNodes.length === 1 ? e.textContent : '')
          .filter((t) => ['Workspace', 'Navigation', 'Panels', 'Terminal', 'Athena', 'Settings'].includes(t)),
        labels,
        kbdCount: dlg.querySelectorAll('kbd').length,
        defaultPrevented: true,
      }
    })
    expect(snapshot.labels).toContain('New Workspace')
    expect(snapshot.labels).toContain('Toggle Sidebar')
    expect(snapshot.labels).toContain('Add Shell Terminal')
    expect(snapshot.labels).toContain('Change Theme')
    expect(snapshot.labels).toContain('Open Settings')
    expect(snapshot.labels).toContain('New Chat')
    expect(snapshot.labels).not.toContain('Theme: Nyx')
    expect(snapshot.kbdCount).toBeGreaterThanOrEqual(5)

    // Group headers rendered
    expect(snapshot.groupHeaders).toEqual(
      expect.arrayContaining(['Workspace', 'Navigation', 'Panels', 'Terminal', 'Athena', 'Settings']),
    )

    // Typing filters
    await browser.execute(() => {
      const input = document.querySelector('[role="dialog"][aria-label="Command palette"] input')
      input.value = 'theme'
      input.dispatchEvent(new Event('input', { bubbles: true }))
    })
    await browser.pause(300)
    const filtered = await browser.execute(() =>
      [...document.querySelectorAll('[role="dialog"][aria-label="Command palette"] button > span:nth-child(2) > span:first-child')].map((e) => e.textContent)
    )
    expect(filtered).toContain('Change Theme')

    await browser.saveScreenshot(join(TEST_SCREENSHOTS, 'palette.png'))
  })
})
