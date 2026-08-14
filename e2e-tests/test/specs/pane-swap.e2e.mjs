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

async function lastGridState() {
  return browser.execute(() => {
    const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
    const grid = grids[grids.length - 1]
    if (!grid) return { paneIds: [], pillIds: [] }
    return {
      paneIds: Array.from(grid.querySelectorAll('.pane-wrap'))
        .map((pane) => pane.getAttribute('data-pane-id'))
        .filter(Boolean),
      pillIds: Array.from(grid.querySelectorAll('[data-agent-pill="true"]'))
        .map((pill) => pill.getAttribute('data-agent-pane-id'))
        .filter(Boolean),
    }
  })
}

describe('Terminal pane drag/swap regression', () => {
  it('swaps two panes through the fullscreen pill drag overlay', async function () {
    this.timeout(90000)

    await browser.execute(() => {
      window.__athenaE2E = true
      window.__paneSwapTrustRoot = 'pending'
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: '/tmp' })
        .then(() => {
          window.__paneSwapTrustRoot = 'ok'
        })
        .catch((error) => {
          window.__paneSwapTrustRoot = 'error:' + String(error)
        })
    })
    await browser.waitUntil(
      async () => browser.execute(() => window.__paneSwapTrustRoot === 'ok'),
      { timeout: 10000, interval: 100, timeoutMsg: 'Could not register /tmp as a trusted root' },
    )

    expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((btn) =>
          (btn.textContent || '').includes('Terminal Workspace'),
        ),
      ),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
    )

    expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
    expect((await clickButtonByText('Next >')).ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-shell')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Terminal configuration step did not appear' },
    )

    const addResult = await browser.execute(() => {
      const button = document.getElementById('add-shell')
      if (!button) return { ok: false }
      button.click()
      button.click()
      return { ok: true }
    })
    expect(addResult.ok).toBe(true)
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => {
        const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
        const grid = grids[grids.length - 1]
        return !!grid && grid.querySelectorAll('.pane-wrap').length === 2 &&
          grid.querySelectorAll('[data-agent-pill="true"]').length === 2 &&
          grid.querySelectorAll('.xterm-mount').length === 2
      }),
      { timeout: 25000, interval: 500, timeoutMsg: 'Two terminal panes did not mount' },
    )

    const before = await lastGridState()
    expect(before.paneIds.length).toBe(2)
    expect(before.pillIds.length).toBe(2)
    expect(before.paneIds[0]).not.toBe(before.paneIds[1])
    expect(before.pillIds).toEqual(before.paneIds)

    const dragPoints = await browser.execute(() => {
      const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
      const grid = grids[grids.length - 1]
      if (!grid) return { ok: false }
      const pills = Array.from(grid.querySelectorAll('[data-agent-pill="true"]'))
      const targetPaneId = pills[1]?.getAttribute('data-agent-pane-id')
      const target = Array.from(grid.querySelectorAll('.pane-wrap'))
        .find((pane) => pane.getAttribute('data-pane-id') === targetPaneId)
      if (!pills[0] || !target) return { ok: false }
      const sourceRect = pills[0].getBoundingClientRect()
      const targetRect = target.getBoundingClientRect()
      const pointerId = 73
      pills[0].dispatchEvent(new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        pointerId,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: sourceRect.left + sourceRect.width / 2,
        clientY: sourceRect.top + sourceRect.height / 2,
      }))
      return {
        ok: true,
        pointerId,
        x: targetRect.left + targetRect.width / 2,
        y: targetRect.top + targetRect.height / 2,
      }
    })
    expect(dragPoints.ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.dnd-overlay')),
      { timeout: 5000, interval: 100, timeoutMsg: 'Pane drag overlay did not mount' },
    )

    const moveResult = await browser.execute(({ pointerId, x, y }) => {
      const overlay = document.querySelector('.dnd-overlay')
      if (!overlay) return { ok: false }
      overlay.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: x,
        clientY: y,
      }))
      return { ok: true }
    }, dragPoints)
    expect(moveResult.ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.pane-wrap.is-dnd-target')),
      { timeout: 5000, interval: 100, timeoutMsg: 'Pane drop target was not highlighted' },
    )

    const upResult = await browser.execute(({ pointerId, x, y }) => {
      const overlay = document.querySelector('.dnd-overlay')
      if (!overlay) return { ok: false }
      overlay.dispatchEvent(new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: x,
        clientY: y,
      }))
      return { ok: true }
    }, dragPoints)
    expect(upResult.ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !document.querySelector('.dnd-overlay')),
      { timeout: 5000, interval: 100, timeoutMsg: 'Pane drag overlay did not unmount after release' },
    )

    await browser.waitUntil(
      async () => {
        const after = await lastGridState()
        return after.paneIds.length === 2 &&
          after.paneIds[0] === before.paneIds[1] &&
          after.paneIds[1] === before.paneIds[0] &&
          after.pillIds[0] === before.pillIds[1] &&
          after.pillIds[1] === before.pillIds[0]
      },
      { timeout: 10000, interval: 250, timeoutMsg: 'Pane content/order did not swap after drag release' },
    )

    await browser.saveScreenshot(join(screenshotDir, 'pane-swap.png'))
  })
})
