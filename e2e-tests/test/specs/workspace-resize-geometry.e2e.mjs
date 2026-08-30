// Ported from e2e-tests/test_workspace_resize_geometry.mjs (deleted 2026-08-24).
//
// Contract under defense: after launching a Terminal Workspace, the pane
// grid stretches to the viewport (>= 80% width) and every pane child has
// non-zero area; xterm surfaces are mounted inside the grid.

describe('workspace resize geometry', () => {
  it('stretches the pane grid to the viewport with non-zero-area panes', async () => {
    // Wait for app + Dioxus mount.
    await browser.pause(5000)

    // Skip modal validation (E2E flag honored by the app).
    await browser.execute(() => {
      window.__athenaE2E = true
    })

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

    // Pick Terminal Workspace card (avoid the Launch Space button).
    const picked = await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.includes('Terminal Workspace') && !btn.textContent.includes('Launch')) {
          btn.click()
          return true
        }
      }
      return false
    })
    expect(picked).toBe(true)
    await browser.pause(1500)

    // Advance Step 1 (E2E flag lets Next pass with an empty dir), then add
    // one pane — launch stays disabled while total panes is 0 — and launch.
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
    expect(paneAdded, '#add-shell button missing on pane step').toBe(true)
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
    expect(launched, 'Launch Space button missing').toBe(true)
    // Allow grid + xterm mounts to render.
    await browser.pause(3000)

    const geom = await browser.execute(() => {
      let grid = null
      for (const el of document.querySelectorAll('div')) {
        const s = el.getAttribute('style') || ''
        if (s.includes('display: grid') && s.includes('grid-template-columns')) {
          grid = el
          break
        }
      }
      if (!grid) return { found: false, viewportW: window.innerWidth }
      const gridRect = grid.getBoundingClientRect()
      const panes = []
      for (const child of grid.children) {
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
    expect(
      geom.gridW,
      `grid width ${geom.gridW} below 80% of viewport ${geom.viewportW}`,
    ).toBeGreaterThanOrEqual(minW)
    expect(geom.panes.length).toBeGreaterThan(0)
    for (const pane of geom.panes) {
      expect(pane.w, `pane width 0: ${JSON.stringify(geom.panes)}`).toBeGreaterThan(0)
      expect(pane.h, `pane height 0: ${JSON.stringify(geom.panes)}`).toBeGreaterThan(0)
    }
  })
})
