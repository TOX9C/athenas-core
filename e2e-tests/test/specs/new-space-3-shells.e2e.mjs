import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

async function clickButtonByText(text, { partial = false } = {}) {
  return browser.execute(
    ({ text, partial }) => {
      for (const btn of document.querySelectorAll('button')) {
        const content = (btn.textContent || '').trim()
        const matches = partial ? content.includes(text) : content === text
        if (!matches) continue

        btn.click()
        return { ok: true, content }
      }
      return { ok: false, text }
    },
    { text, partial },
  )
}

describe('New space terminal regression', () => {
  it('launches a new space with 3 shell panes and mounts xterm in each pane', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    const openResult = await clickButtonByText('New Workspace', { partial: true })
    expect(openResult.ok).toBe(true)

    await browser.waitUntil(
      async () => {
        const isOpen = await browser.execute(() => {
          return Array.from(document.querySelectorAll('button')).some((btn) =>
            (btn.textContent || '').includes('Terminal Workspace'),
          )
        })
        return isOpen
      },
      { timeout: 10000, interval: 250, timeoutMsg: 'New Space modal did not open' },
    )

    const terminalModeResult = await clickButtonByText('Terminal Workspace', { partial: true })
    expect(terminalModeResult.ok).toBe(true)

    const nextResult = await clickButtonByText('Next >')
    expect(nextResult.ok).toBe(true)

    await browser.waitUntil(
      async () => {
        return browser.execute(() => !!document.getElementById('add-shell'))
      },
      { timeout: 10000, interval: 250, timeoutMsg: 'Terminal configuration step did not appear' },
    )

    const addResult = await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      if (!btn) return { ok: false }
      for (let i = 0; i < 3; i += 1) {
        btn.click()
      }
      const summary = Array.from(document.querySelectorAll('label, div, span'))
        .map((el) => (el.textContent || '').trim())
        .find((text) => text.includes('Agents (3/16)'))
      return { ok: true, summary: summary || null }
    })
    expect(addResult.ok).toBe(true)
    expect(addResult.summary).toContain('Agents (3/16)')

    const launchResult = await clickButtonByText('Launch Space')
    expect(launchResult.ok).toBe(true)

    await browser.waitUntil(
      async () => {
        const counts = await browser.execute(() => {
          const mounts = Array.from(document.querySelectorAll('.xterm-mount'))
          const readyMounts = mounts.filter((mount) => !!mount.querySelector('.xterm'))
          return {
            mounts: mounts.length,
            readyMounts: readyMounts.length,
            hasStatusText: document.body.textContent.includes('3 panes'),
          }
        })
        return counts.mounts === 3 && counts.readyMounts === 3
      },
      {
        timeout: 20000,
        interval: 500,
        timeoutMsg: 'Expected 3 mounted xterm panes after launching the space',
      },
    )

    const terminalState = await browser.execute(() => {
      const mounts = Array.from(document.querySelectorAll('.xterm-mount'))
      const panes = mounts.map((mount, index) => {
        const rect = mount.getBoundingClientRect()
        return {
          index,
          hasXterm: !!mount.querySelector('.xterm'),
          width: Math.round(rect.width),
          height: Math.round(rect.height),
        }
      })
      return {
        mounts: mounts.length,
        panes,
        statusText: document.body.textContent,
      }
    })

    expect(terminalState.mounts).toBe(3)
    for (const pane of terminalState.panes) {
      expect(pane.hasXterm).toBe(true)
      expect(pane.width).toBeGreaterThan(50)
      expect(pane.height).toBeGreaterThan(50)
    }
    expect(terminalState.statusText).toContain('3 panes')

    await browser.saveScreenshot(join(screenshotDir, 'new-space-3-shells.png'))
  })
})
