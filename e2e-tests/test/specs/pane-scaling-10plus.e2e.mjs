import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

async function clickButtonByText(text, { partial = false } = {}) {
  return browser.execute(
    ({ text, partial }) => {
      for (const button of document.querySelectorAll('button')) {
        const content = (button.textContent || '').trim()
        if ((partial && content.includes(text)) || (!partial && content === text)) {
          button.click()
          return true
        }
      }
      return false
    },
    { text, partial },
  )
}

describe('Large workspace geometry regression', () => {
  it('keeps twelve panes visible, non-overlapping, and responsive', async function () {
    this.timeout(150000)

    await browser.execute(() => {
      window.__athenaE2E = true
      window.__paneScalingTrustRoot = 'pending'
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: '/tmp' })
        .then(() => {
          window.__paneScalingTrustRoot = 'ok'
        })
        .catch((error) => {
          window.__paneScalingTrustRoot = 'error:' + String(error)
        })
    })
    await browser.waitUntil(
      async () => browser.execute(() => window.__paneScalingTrustRoot === 'ok'),
      { timeout: 10000, interval: 100, timeoutMsg: 'Could not register /tmp as a trusted root' },
    )

    expect(await clickButtonByText('New Workspace', { partial: true })).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((button) =>
          (button.textContent || '').includes('Terminal Workspace'),
        ),
      ),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
    )
    expect(await clickButtonByText('Terminal Workspace', { partial: true })).toBe(true)
    expect(await clickButtonByText('Next >')).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-shell')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Terminal configuration step did not appear' },
    )
    const added = await browser.execute(() => {
      const button = document.getElementById('add-shell')
      if (!button) return false
      for (let index = 0; index < 12; index += 1) button.click()
      return true
    })
    expect(added).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => {
        const heading = Array.from(document.querySelectorAll('body *'))
          .find((element) => /^Agents \(\d+\/16\)$/.test((element.textContent || '').trim()))
        return !!heading && /^Agents \(12\/16\)$/.test((heading.textContent || '').trim())
      }),
      { timeout: 5000, interval: 100, timeoutMsg: 'Twelve pane configuration was not reflected before launch' },
    )
    expect(await clickButtonByText('Launch Space')).toBe(true)

    const start = Date.now()
    await browser.waitUntil(
      async () => browser.execute(() => {
        const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
        const grid = grids[grids.length - 1]
        if (!grid) return false
        const panes = Array.from(grid.querySelectorAll('.pane-wrap'))
        return panes.length === 12 && panes.every((pane) => {
          const rect = pane.getBoundingClientRect()
          return rect.width > 0 && rect.height > 0
        })
      }),
      { timeout: 60000, interval: 500, timeoutMsg: 'Twelve panes did not reach non-zero geometry' },
    )
    const mountMs = Date.now() - start

    const geometry = await browser.execute(() => {
      const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
      const grid = grids[grids.length - 1]
      const panes = Array.from(grid.querySelectorAll('.pane-wrap')).map((pane) => {
        const rect = pane.getBoundingClientRect()
        return { left: rect.left, top: rect.top, right: rect.right, bottom: rect.bottom, width: rect.width, height: rect.height }
      })
      const overlaps = []
      for (let left = 0; left < panes.length; left += 1) {
        for (let right = left + 1; right < panes.length; right += 1) {
          const a = panes[left]
          const b = panes[right]
          const overlap = a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top
          // Adjacent panes may share a border; positive-area overlap is invalid.
          if (overlap && Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 &&
              Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1) {
            overlaps.push([left, right])
          }
        }
      }
      return { panes, overlaps }
    })

    expect(geometry.panes.length).toBe(12)
    expect(geometry.overlaps).toEqual([])
    expect(geometry.panes.every((pane) => pane.width > 20 && pane.height > 20)).toBe(true)

    // A second layout pass should settle quickly and preserve usable geometry.
    const relayoutStart = Date.now()
    await browser.execute(() => window.dispatchEvent(new Event('resize')))
    await browser.waitUntil(
      async () => browser.execute(() => {
        const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
        const grid = grids[grids.length - 1]
        return !!grid && Array.from(grid.querySelectorAll('.pane-wrap')).every((pane) => {
          const rect = pane.getBoundingClientRect()
          return rect.width > 20 && rect.height > 20
        })
      }),
      { timeout: 10000, interval: 100, timeoutMsg: 'Twelve-pane geometry degraded after resize' },
    )
    const relayoutMs = Date.now() - relayoutStart

    console.log(`[pane-scaling] mountMs=${mountMs} relayoutMs=${relayoutMs} panes=${geometry.panes.length}`)
    await browser.saveScreenshot(join(screenshotDir, 'pane-scaling-12.png'))
  })
})
