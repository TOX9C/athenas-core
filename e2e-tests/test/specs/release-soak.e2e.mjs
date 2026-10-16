import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')
const iterations = Number.parseInt(process.env.SOAK_ITERATIONS || '10', 10)

async function clickText(text, { partial = false } = {}) {
  return browser.execute(({ text, partial }) => {
    const button = Array.from(document.querySelectorAll('button')).find((candidate) => {
      const content = (candidate.textContent || '').trim()
      return partial ? content.includes(text) : content === text
    })
    if (!button) return false
    button.click()
    return true
  }, { text, partial })
}

// Workspaces persist in the store across app launches, so restored workspaces
// make `active_space` Some and the empty-state button never renders; the tab
// plus button is also gated on a non-empty list. The sidebar's bottom
// "New Workspace" button renders either way, but the sidebar starts collapsed
// and UI state is not persisted — expand it once so each cycle has a trigger.
async function ensureSidebarOpen() {
  return browser.execute(() => {
    if (document.querySelector('.sidebar')) return 'already-open'
    const expand = document.querySelector('button[title="Expand sidebar"]')
    if (!expand) return 'no-expand-button'
    expand.click()
    return 'expanded'
  })
}

// Open the workspace modal from the trigger that is present regardless of how
// many workspaces the store restored, keeping 10 open→close cycles real.
async function openNewWorkspaceModal(label) {
  await ensureSidebarOpen()
  const opened = await browser.execute(() => {
    const sidebar = document.querySelector('.sidebar')
    const button = Array.from(sidebar?.querySelectorAll('button') || []).find(
      (candidate) => (candidate.textContent || '').trim() === 'New Workspace',
    )
    if (!button) return false
    button.click()
    return true
  })
  if (!opened) throw new Error(`New Workspace missing at ${label}`)
}

async function closeTransientUi() {
  return browser.execute(() => {
    // Step 0 of the New Workspace modal closes via an icon-only button
    // (aria-label="Close dialog"), so also match the label attribute.
    const button = Array.from(document.querySelectorAll('button')).find((candidate) => {
      const text = (candidate.textContent || '').trim().toLowerCase()
      const label = (candidate.getAttribute('aria-label') || '').toLowerCase()
      return (
        text === 'cancel' ||
        text === 'close' ||
        text === 'back' ||
        label === 'close dialog'
      )
    })
    if (!button) return false
    button.click()
    return true
  })
}

describe('Release candidate renderer soak', () => {
  it(`survives ${iterations} repeated UI lifecycle cycles`, async function () {
    this.timeout(Math.max(180000, iterations * 20000))

    await browser.waitUntil(
      async () => browser.execute(() => {
        const loader = document.getElementById('wasm-loading')
        const root = document.getElementById('main')
        const mountedControl = Array.from(document.querySelectorAll('button')).some((button) =>
          (button.textContent || '').includes('New Workspace'),
        )
        const dioxusMounted = !!root.querySelector('[data-dioxus-id]')
        if (!root || !loader || (!mountedControl && !dioxusMounted)) return false
        const style = window.getComputedStyle(loader)
        const rect = loader.getBoundingClientRect()
        return style.display === 'none'
          || style.visibility === 'hidden'
          || Number.parseFloat(style.opacity || '1') === 0
          || rect.width === 0
          || rect.height === 0
      }),
      { timeout: 30000, interval: 500, timeoutMsg: 'Dioxus UI did not mount or WASM loader remained visible' },
    )

    await browser.execute(() => {
      window.__athenaE2E = true
      window.__releaseSoak = { errors: [], initialPanes: 0 }
      window.addEventListener('error', (event) => {
        window.__releaseSoak.errors.push(String(event.message || 'window error'))
      })
      window.addEventListener('unhandledrejection', (event) => {
        window.__releaseSoak.errors.push(String(event.reason || 'unhandled rejection'))
      })
    })

    for (let i = 0; i < iterations; i += 1) {
      await openNewWorkspaceModal(`iteration ${i + 1}`)
      await browser.pause(150)

      // Exercise the modal path without launching a destructive workspace on
      // every cycle. This catches remount/drop failures while keeping the
      // test independent from persistent user data.
      await closeTransientUi()
      await browser.pause(150)

      const state = await browser.execute(() => ({
        rootPresent: !!document.getElementById('main'),
        loaderVisible: (() => {
          const loader = document.getElementById('wasm-loading')
          if (!loader) return false
          const style = window.getComputedStyle(loader)
          const rect = loader.getBoundingClientRect()
          return style.display !== 'none'
            && style.visibility !== 'hidden'
            && Number.parseFloat(style.opacity || '1') > 0
            && rect.width > 0
            && rect.height > 0
        })(),
        panels: document.querySelectorAll('[data-panel], .panel, .workspace-grid-root').length,
        errors: window.__releaseSoak?.errors || [],
      }))
      if (!state.rootPresent || state.loaderVisible) {
        throw new Error(`renderer root/loader failure at iteration ${i + 1}: ${JSON.stringify(state)}`)
      }
      if (state.errors.length > 0) {
        throw new Error(`renderer error at iteration ${i + 1}: ${state.errors.join('; ')}`)
      }

      // Exercise an available panel switch if the current app exposes the
      // navigation labels; absence is recorded rather than treated as a
      // false stability failure for alternate empty-state layouts.
      await clickText('Settings', { partial: true })
      await browser.pause(75)
      await closeTransientUi()
    }

    const finalState = await browser.execute(() => ({
      mountedControl: Array.from(document.querySelectorAll('button')).some((button) =>
        (button.textContent || '').includes('New Workspace'),
      ),
      dioxusMounted: !!document.querySelector('#main [data-dioxus-id]'),
      rootPresent: !!document.getElementById('main'),
      loaderVisible: (() => {
        const loader = document.getElementById('wasm-loading')
        if (!loader) return false
        const style = window.getComputedStyle(loader)
        const rect = loader.getBoundingClientRect()
        return style.display !== 'none'
          && style.visibility !== 'hidden'
          && Number.parseFloat(style.opacity || '1') > 0
          && rect.width > 0
          && rect.height > 0
      })(),
      errors: window.__releaseSoak?.errors || [],
      ptyCount: document.querySelectorAll('.terminal-pane, .xterm-mount').length,
    }))
    await browser.saveScreenshot(join(screenshotDir, 'release-soak-final.png'))
    expect(finalState.rootPresent).toBe(true)
    expect(finalState.mountedControl || finalState.dioxusMounted).toBe(true)
    expect(finalState.loaderVisible).toBe(false)
    expect(finalState.errors).toEqual([])
    console.log('[release-soak] final state:', JSON.stringify(finalState))
  })
})
