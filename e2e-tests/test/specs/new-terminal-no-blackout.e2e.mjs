// Regression guard for the "clicking Add Shell blacks out every pane for
// ~2 seconds" symptom (plan 2026-10-11-new-terminal-blackout):
//
//  1. Rolling out: the grid's pane-count rewrite should not reflow every
//     existing pane (T1 — capacity guard).
//  2. The reflow itself turned every pane black at once via freeze_screen
//     (opacity 0) — the reveal then waited on the whole-grid resize convoy.
//     T3 skips the shared-atlas wipe on routine mounts; T4 prevents resize
//     from queueing behind a slow fork/exec (SIGWINCH never convoys).
//
// Observable liveness = every pane's renderer canvas remains present and
// sized during the layout frame. Sampled every 100ms for 1.5s; no pane may
// stay canvas-less beyond ~400ms after the click.
//
// Run:  tauri-wd &  &&  npx wdio run wdio.conf.mjs --spec test/specs/new-terminal-no-blackout.e2e.mjs

const PANE_WRAP_JS = `Array.from((() => { const g = Array.from(document.querySelectorAll('.workspace-grid-root')).pop(); return g ? g.querySelectorAll('.pane-wrap') : [] })())`

const APP_STATE_JS = `(() => {
  const btns = Array.from(document.querySelectorAll('button'))
  return {
    hasNewWorkspace: btns.some((b) => (b.textContent || '').includes('New Workspace')),
    hasAddShell: btns.some((b) => (b.getAttribute('title') || '').startsWith('Add Shell')),
    paneCount: (() => {
      const g = Array.from(document.querySelectorAll('.workspace-grid-root')).pop()
      return g ? g.querySelectorAll('.pane-wrap').length : 0
    })(),
  }
})()`

/// Per-pane canvas liveness: presence + non-zero size = renderer is alive.
async function canvasLiveness() {
  return browser.execute(() => {
    const grid = Array.from(document.querySelectorAll('.workspace-grid-root')).pop()
    if (!grid) return { ok: false, panes: [] }
    const panes = Array.from(grid.querySelectorAll('.pane-wrap[data-pane-id]'))
    return {
      ok: true,
      panes: panes.map((p) => {
        const canvases = Array.from(p.querySelectorAll('.xterm-mount canvas'))
        return {
          id: p.getAttribute('data-pane-id'),
          canCount: canvases.length,
          live: canvases.some((c) => c.width > 0 && c.height > 0),
          widths: canvases.map((c) => c.width).join('x'),
        }
      }),
    }
  })
}

/// Serially sample liveness every 100ms for `ms`. A pane is "blank" when no
/// canvas has a non-zero size. Returns the samples; callers inspect them.
async function sampleLivenessFor(ms, intervalMs = 100) {
  const samples = []
  const t0 = Date.now()
  while (Date.now() - t0 < ms) {
    samples.push(await canvasLiveness())
    await browser.pause(intervalMs)
  }
  return samples
}

function blanksAfter(samples, graceMs) {
  // first sample at t≈0, interval 100ms → sample i is at t≈i*100ms.
  return samples.filter((s, i) => i * 100 > graceMs && s.ok && s.panes.some((p) => !p.live))
}

async function ensureWorkspaceWithPanes(paneCount) {
  await browser.execute(() => {
    window.__athenaE2E = true
    window.__TAURI__.core.invoke('workspace_add_trusted_root', { dir: '/tmp' }).catch(() => {})
  })
  await browser.pause(1500)
  const state0 = await browser.execute(`return ${APP_STATE_JS}`)
  console.log('[blackout] initial:', JSON.stringify(state0))

  if (state0.hasAddShell && state0.paneCount >= paneCount) {
    await browser.waitUntil(
      () => browser.execute(() => document.querySelectorAll('.xterm-mount canvas').length > 0),
      { timeout: 15000, interval: 500, timeoutMsg: 'existing panes did not paint' },
    )
    return
  }

  expect(state0.hasNewWorkspace).toBe(true)
  await browser.execute(() =>
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').includes('New Workspace'))
      ?.click(),
  )
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((b) =>
          (b.textContent || '').includes('Terminal Workspace'),
        ),
      ),
    { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
  )
  await browser.execute(() =>
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').includes('Terminal Workspace'))
      ?.click(),
  )
  await browser.waitUntil(() => browser.execute(() => !!document.getElementById('add-shell')), {
    timeout: 10000,
    interval: 250,
    timeoutMsg: 'Terminal configuration step did not appear',
  })

  // Modal default has one shell pane; each #add-shell click adds another.
  // Clicking N-1 times produces `paneCount` panes on Launch.
  for (let i = 1; i < paneCount; i++) {
    const r = await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      if (!btn) return { ok: false }
      btn.click()
      return { ok: true }
    })
    expect(r.ok).toBe(true)
    await browser.pause(200)
  }

  await browser.execute(() =>
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').trim() === 'Launch Space')
      ?.click(),
  )
  await browser.waitUntil(
    () =>
      browser.execute((n) => {
        const grid = Array.from(document.querySelectorAll('.workspace-grid-root')).pop()
        return {
          paneCount: grid ? grid.querySelectorAll('.pane-wrap').length : 0,
          canvasCount: document.querySelectorAll('.xterm-mount canvas').length,
          target: n,
        }
      }, paneCount).then((s) => s.paneCount === s.target && s.canvasCount >= s.target),
    { timeout: 30000, interval: 500, timeoutMsg: `Workspace did not reach ${paneCount} panes` },
  )
  // Settle: mount handshakes, initial fits, first paints.
  await browser.pause(3000)
}

describe('Add Shell blackout regression', function () {
  it('adding a pane never leaves an existing pane canvas black >400ms', async function () {
    this.timeout(120000)
    await ensureWorkspaceWithPanes(4)

    const before = await canvasLiveness()
    expect(before.panes.length).toBe(4)
    for (const p of before.panes) expect(p.live).toBe(true)

    // BUILD evidence: this workspace just mounted 4 panes (or reused the
    // 4 panes from the fresh boot). First pane had no same-config sibling,
    // so the settle task DID reset the shared glyph atlas — the counter
    // proves the codepath fired at least once for the true first mount.
    const atlasBoot = await browser.execute(() => window.__athenaAtlasResets || 0)
    console.log('[blackout] atlas resets at space-ready:', atlasBoot)
    expect(atlasBoot).toBeGreaterThanOrEqual(1)

    // Record the counter across the Add Shell click: the new pane shares its
    // font config with the 4 live siblings, so T3 must NOT reset again.
    const atlasBefore = atlasBoot

    await browser.execute(() => {
      Array.from(document.querySelectorAll('button'))
        .find((b) => (b.getAttribute('title') || '').startsWith('Add Shell'))
        ?.click()
    })

    // Allow the layout frame one grace window, then assert.
    await browser.pause(400)
    const liveness = await sampleLivenessFor(1500, 100)

    const bad = blanksAfter(liveness, 400)
    if (bad.length) {
      console.log('[blackout] first failure sample:', JSON.stringify(bad[0]))
    }
    expect(bad.length).toBe(0)

    const atlasAfter = await browser.execute(() => window.__athenaAtlasResets || 0)
    const delta = atlasAfter - atlasBefore
    console.log('[blackout] atlas resets on Add Shell:', atlasBefore, '->', atlasAfter)
    expect(delta).toBe(0)

    const final = await browser.execute(() => {
      const grid = Array.from(document.querySelectorAll('.workspace-grid-root')).pop()
      return grid ? grid.querySelectorAll('.pane-wrap').length : 0
    })
    expect(final).toBe(5)
  })

  it('right-sidebar toggle does not stall the terminal grid', async function () {
    this.timeout(120000)
    await ensureWorkspaceWithPanes(4)

    // Open the Athena right sidebar: lib.rs flips main_flex_basis 100% -> 60%,
    // which reflows every pane (each >=25% width change fires freeze_screen).
    await browser.execute(() =>
      Array.from(document.querySelectorAll('button'))
        .find((b) => (b.getAttribute('title') || '').startsWith('Athena'))
        ?.click(),
    )
    await browser.pause(400)

    const liveness = await sampleLivenessFor(1200, 100)
    const bad = blanksAfter(liveness, 400)
    if (bad.length) {
      console.log('[sidebar] first failure sample:', JSON.stringify(bad[0]))
    }
    expect(bad.length).toBe(0)
  })
})
