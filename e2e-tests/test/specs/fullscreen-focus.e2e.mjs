import { mkdirSync } from 'node:fs'
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

// Focus + fullscreen-layout probe for a pane.
const focusProbe = (paneId) =>
  browser.execute((paneId) => {
    const active = document.activeElement
    const wrap = document.querySelector(`.pane-wrap[data-pane-id="${paneId}"]`)
    const style = wrap?.getAttribute('style') || ''
    return {
      helperFocused: !!active?.classList?.contains('xterm-helper-textarea'),
      focusedInside: !!wrap?.contains(active),
      isFullscreen: style.includes('position: absolute') && style.includes('z-index: 40'),
      isHidden: style.includes('display: none'),
    }
  }, paneId)

const paneIds = async () =>
  browser.execute(() => {
    const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
    const grid = grids[grids.length - 1]
    return Array.from(grid?.querySelectorAll('.pane-wrap') || [])
      .map((pane) => pane.getAttribute('data-pane-id'))
      .filter(Boolean)
  })

const clickFullscreenButton = (paneId, title) =>
  browser.execute((paneId, title) => {
    const btn = document.querySelector(
      `.pane-wrap[data-pane-id="${paneId}"] button[title="${title}"]`,
    )
    if (!btn) return { ok: false, title }
    btn.click()
    return { ok: true, title }
  }, paneId, title)

describe('Fullscreen button focuses the terminal', () => {
  it('entering and exiting fullscreen keeps keyboard focus on the pane', async function () {
    this.timeout(90000)

    // Trusted root: a dedicated /tmp subdir (not /tmp itself —
    // `is_forbidden_root` rejects /tmp and /private). mkdirSync because the
    // tauri API has no dir-creation command the webview can invoke.
    mkdirSync('/tmp/e2e-fs-focus', { recursive: true })
    await browser.execute(() => {
      window.__athenaE2E = true
      window.__fsTrustRoot = 'pending'
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: '/tmp/e2e-fs-focus' })
        .then(() => { window.__fsTrustRoot = 'ok' })
        .catch((error) => { window.__fsTrustRoot = 'error:' + String(error) })
    })
    // Open the sidebar if the app booted with it collapsed (default is
    // hidden; the rail's Expand button restores it).
    expect(await browser.execute(() => {
      const rail = document.querySelector('button[title="Expand sidebar"]')
      if (rail) rail.click()
      return true
    })).toBe(true)
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
    expect(await browser.execute(() => {
      const button = document.getElementById('add-shell')
      if (!button) return { ok: false }
      button.click()
      button.click()
      return { ok: true }
    })).toEqual({ ok: true })
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    await browser.waitUntil(
      async () => (await paneIds()).length === 2,
      { timeout: 25000, interval: 500, timeoutMsg: 'Two terminal panes did not mount' },
    )
    const [paneA, paneB] = await paneIds()

    // Make pane A the active, focused pane (the pane-body pointerdown path).
    await browser.execute((paneId) => {
      const mount = document.getElementById(paneId)
      mount?.dispatchEvent(new PointerEvent('pointerdown', {
        bubbles: true, cancelable: true, pointerId: 7, button: 0,
      }))
    }, paneA)
    await browser.waitUntil(async () => {
      const p = await focusProbe(paneA)
      return p.helperFocused && p.focusedInside
    }, { timeout: 10000, interval: 200, timeoutMsg: 'Pane A did not take focus after click' })

    // Enter fullscreen on pane B (not the active pane) — the reported repro.
    expect((await clickFullscreenButton(paneB, 'Fullscreen')).ok).toBe(true)

    // THE regression assertion: pane B's helper textarea owns focus, no
    // manual click on the terminal.
    await browser.waitUntil(async () => {
      const p = await focusProbe(paneB)
      return p.helperFocused && p.focusedInside
    }, { timeout: 10000, interval: 200, timeoutMsg: 'Fullscreen did not focus the terminal' })

    const layout = await focusProbe(paneB)
    expect(layout.isFullscreen).toBe(true)
    expect((await focusProbe(paneA)).isHidden).toBe(true)

    // Exit fullscreen — focus must survive the button click.
    expect((await clickFullscreenButton(paneB, 'Exit Fullscreen')).ok).toBe(true)
    await browser.waitUntil(async () => {
      const p = await focusProbe(paneB)
      return !p.isFullscreen && p.helperFocused && p.focusedInside
    }, { timeout: 10000, interval: 200, timeoutMsg: 'Exit fullscreen lost terminal focus' })

    // Case B: re-enter fullscreen on the ALREADY-active pane (set_active
    // no-ops; the direct focus call is the only thing covering this).
    expect((await clickFullscreenButton(paneB, 'Fullscreen')).ok).toBe(true)
    await browser.waitUntil(async () => {
      const p = await focusProbe(paneB)
      return p.isFullscreen && p.helperFocused && p.focusedInside
    }, { timeout: 10000, interval: 200, timeoutMsg: 'Re-enter fullscreen on active pane lost focus' })
    expect((await clickFullscreenButton(paneB, 'Exit Fullscreen')).ok).toBe(true)
  })

  afterEach(async function () {
    if (this.currentTest?.state === 'failed') {
      await browser.saveScreenshot(join(screenshotDir, 'fullscreen-focus-failure.png'))
    }
  })
})
