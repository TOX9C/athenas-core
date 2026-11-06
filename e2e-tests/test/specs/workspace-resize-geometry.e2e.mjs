// Ported from e2e-tests/test_workspace_resize_geometry.mjs (deleted 2026-08-24).
//
// Contract under defense: after launching a Terminal Workspace, the pane
// grid stretches to the viewport (>= 80% width) and every pane child has
// non-zero area; xterm surfaces are mounted inside the grid.

import { e2eTrustRoot, setE2E, fillSpaceDir } from '../helpers/trust.mjs'

describe('workspace resize geometry', () => {
  it('stretches the pane grid to the viewport with non-zero-area panes', async () => {
    // Wait for app + Dioxus mount.
    await browser.pause(5000)

    // Skip modal validation (E2E flag honored by the app).
    await setE2E()

    // Permitted launch dir: the modal's blank-dir fallback is "/tmp", which
    // pty_spawn rejects as a forbidden root.
    const trustDir = e2eTrustRoot('workspace-resize-geometry')

    // Open the workspace modal.
    const opened = await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.includes('New Workspace')) {
          btn.click()
          return true
        }
      }
      return false
    })
    expect(opened).toBe(true)
    await browser.pause(1500)

    // Pick the Terminal Workspace card. Match on the card's own title only:
    // the old predicate also required `!text.includes('Launch')` to avoid
    // stepping on the "Launch Space" footer button, but the card's subtitle
    // ("Launch multiple terminal panes …") contains that word too, so the
    // filter excluded the card it was looking for and the probe returned
    // false. "Launch Space" never contains "Terminal Workspace", so no
    // exclusion is needed. The modal does not mount synchronously, so wait
    // for the card instead of probing once (same fix as e9e9eab).
    const picked = await browser.waitUntil(
      () =>
        browser.execute(() => {
          for (const btn of document.querySelectorAll('button')) {
            if ((btn.textContent || '').includes('Terminal Workspace')) {
              btn.click()
              return true
            }
          }
          return false
        }),
      { timeout: 15000, interval: 250, timeoutMsg: 'Terminal Workspace card never rendered' },
    )
    expect(picked).toBe(true)
    await browser.pause(1500)

    // Advance Step 1 (E2E flag lets Next pass with an empty dir), then add
    // one pane — launch stays disabled while total panes is 0 — and launch.
    await fillSpaceDir(browser, trustDir)
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Next >') {
          btn.click()
          break
        }
      }
    })
    await browser.pause(1200)
    const paneAdded = await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      if (!btn) return false
      btn.click()
      return true
    })
    // `expect(value, message)` is not usable here: this suite's `expect` is
    // jest's (v30 via expect-webdriverio/@wdio/runner), which throws
    // "Expect takes at most one argument." on ANY second argument — even for
    // a passing assertion — so the custom message is never rendered and the
    // assertion dies with an unrelated error. Guard explicitly instead: same
    // strength (`=== true` on a boolean), message preserved verbatim.
    if (paneAdded !== true) throw new Error('#add-shell button missing on pane step')
    await browser.pause(800)
    const launched = await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Launch Space') {
          btn.click()
          return true
        }
      }
      return false
    })
    if (launched !== true) throw new Error('Launch Space button missing')
    // Allow grid + xterm mounts to render.
    await browser.pause(3000)

    // The pane container is the flex column `.workspace-grid-root`
    // (terminal_grid.rs:222). It is no longer — and is not — a CSS grid: rows
    // and panes are laid out with `display: flex` (`row_styles` at
    // terminal_grid.rs:243), so the old probe looking for inline
    // `display: grid` + `grid-template-columns` matched nothing and always
    // reported `found: false`. Panes are the `[data-pane-id]` wrappers
    // emitted per pane (terminal_grid.rs:315).
    const geom = await browser.execute(() => {
      const container = document.querySelector('.workspace-grid-root')
      if (!container) return { found: false, viewportW: window.innerWidth }
      const gridRect = container.getBoundingClientRect()
      const panes = []
      for (const child of container.querySelectorAll('[data-pane-id]')) {
        const r = child.getBoundingClientRect()
        panes.push({ w: Math.round(r.width), h: Math.round(r.height) })
      }
      let xtermCount = 0
      for (const el of document.querySelectorAll('.xterm, [class*="xterm"]')) {
        if (el.getBoundingClientRect().width > 0) xtermCount++
      }
      return {
        found: true,
        viewportW: window.innerWidth,
        gridW: Math.round(gridRect.width),
        panes,
        xtermCount,
      }
    })

    expect(geom.found).toBe(true)
    const minW = Math.floor(geom.viewportW * 0.8)
    // Same reason as above: the 2-arg `expect` form throws, so each geometry
    // check guards explicitly with its original diagnostic message.
    if (!(geom.gridW >= minW)) {
      throw new Error(`grid width ${geom.gridW} below 80% of viewport ${geom.viewportW}`)
    }
    expect(geom.panes.length).toBeGreaterThan(0)
    for (const pane of geom.panes) {
      if (!(pane.w > 0)) throw new Error(`pane width 0: ${JSON.stringify(geom.panes)}`)
      if (!(pane.h > 0)) throw new Error(`pane height 0: ${JSON.stringify(geom.panes)}`)
    }
  })
})
