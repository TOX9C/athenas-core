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

// Per-pane terminal geometry probe: canvas backing-store vs CSS size ratio
// must equal devicePixelRatio. A blow-up/degrade shows as !=1 scale.
// Note: this runs via browser.execute — it must be self-contained (no closure).
function probePanes() {
  const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
  const grid = grids[grids.length - 1]
  const out = []
  if (!grid) return out
  const dpr = window.devicePixelRatio || 1
  for (const pane of grid.querySelectorAll('.pane-wrap')) {
    const mount = pane.querySelector('.xterm-mount')
    const canvas = pane.querySelector('.xterm-mount canvas')
    const screen = pane.querySelector('.xterm-screen')
    if (!canvas) {
      out.push({ paneId: pane.getAttribute('data-pane-id'), canvas: false })
      continue
    }
    const rect = canvas.getBoundingClientRect()
    const paneId = pane.getAttribute('data-pane-id')
    // Internal cell geometry via the e2e-exposed Terminal instance. The
    // canvas-invariant alone passes even when glyph quads render oversized
    // (the reported bug), so assert the renderer's css cell dims too.
    const term = (window.__athenaTermMap || {})[paneId]
    let cell = null
    try {
      const dims = term?._core?._renderService?.dimensions
      if (dims) cell = { w: dims.css.cell.width, h: dims.css.cell.height }
    } catch {}
    out.push({
      paneId,
      canvas: true,
      cell,
      attrW: canvas.width,
      attrH: canvas.height,
      rectW: rect.width,
      rectH: rect.height,
      dpr,
      scaleW: rect.width > 0 ? canvas.width / rect.width : null,
      scaleH: rect.height > 0 ? canvas.height / rect.height : null,
      screenRect: screen ? screen.getBoundingClientRect().toJSON() : null,
      hasCover: !!pane.querySelector('.xterm-remount-cover'),
    })
  }
  return out
}

const GRID_JS = "const grids = Array.from(document.querySelectorAll('.workspace-grid-root')); const grid = grids[grids.length - 1];"

async function makeSpaceWithShells(count) {
  await browser.execute(() => {
    window.__athenaE2E = true
    // Best-effort, as in all other specs: /tmp roots are rejected by the
    // trust gate, but __athenaE2E bypasses it for the New Space flow.
    window.__TAURI__.core.invoke('workspace_add_trusted_root', { dir: '/tmp' }).catch(() => {})
  })

  expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
  await browser.waitUntil(
    () => browser.execute(() =>
      Array.from(document.querySelectorAll('button')).some((btn) =>
        (btn.textContent || '').includes('Terminal Workspace'),
      ),
    ),
    { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
  )
  expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
  expect((await clickButtonByText('Next >')).ok).toBe(true)
  await browser.waitUntil(
    () => browser.execute(() => !!document.getElementById('add-shell')),
    { timeout: 10000, interval: 250, timeoutMsg: 'Terminal configuration step did not appear' },
  )
  const readAgentCount = () =>
    browser.execute(() => {
      const label = Array.from(document.querySelectorAll('label, div, span'))
        .map((el) => (el.textContent || '').trim())
        .find((text) => /^Agents \(\d+\/16\)$/.test(text))
      const m = label && label.match(/Agents \((\d+)\/16\)/)
      return m ? Number(m[1]) : null
    })
  await browser.waitUntil(async () => (await readAgentCount()) !== null, {
    timeout: 10000, interval: 250, timeoutMsg: 'Agents count label did not appear',
  })
  const initial = await readAgentCount()
  for (let n = 1; n <= count - initial; n++) {
    await browser.execute(() => document.getElementById('add-shell')?.click())
    const expected = initial + n
    await browser.waitUntil(async () => (await readAgentCount()) >= expected, {
      timeout: 5000, interval: 250, timeoutMsg: `Agents count did not reach ${expected}`,
    })
  }
  expect((await clickButtonByText('Launch Space')).ok).toBe(true)
  await browser.waitUntil(
    () => browser.execute((n) => {
      const mounts = Array.from(document.querySelectorAll('.xterm-mount'))
      const ready = mounts.filter((m) => !!m.querySelector('.xterm'))
      return mounts.length === n && ready.length === n
    }, count),
    { timeout: 30000, interval: 500, timeoutMsg: `${count} terminal panes did not mount` },
  )
  // Wait until each pane actually has a renderer canvas (may lag .xterm).
  await browser.waitUntil(
    () => browser.execute((n) =>
      document.querySelectorAll('.xterm-mount canvas').length >= n, count),
    { timeout: 20000, interval: 500, timeoutMsg: 'terminal canvases did not appear' },
  )
  // Let initial fits, the RO debounce, the font-settle remeasure and any
  // late webfont metric adjustment finish BEFORE baselines are captured.
  await browser.pause(3000)
}

// Close the pane at `index` among the current pane-wraps, return survivor probe.
const PANE_WRAP_JS = `Array.from((() => { const g = Array.from(document.querySelectorAll('.workspace-grid-root')).pop(); return g ? g.querySelectorAll('.pane-wrap') : [] })())`
async function closePaneByIndex(index) {
  const paneIds = await browser.execute(`return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`)
  const targetId = paneIds[index]
  // Retain survivor mount element refs to detect remounts.
  await browser.execute(`window.__survivorMounts = (${PANE_WRAP_JS}).filter((p) => p.getAttribute('data-pane-id') !== ${JSON.stringify('__T__')}).map((p) => p.querySelector('.xterm-mount'))`.replace(JSON.stringify('__T__'), JSON.stringify(targetId)))
  await browser.execute(`const pane = (${PANE_WRAP_JS}).find((p) => p.getAttribute('data-pane-id') === "${targetId}"); pane.querySelector('button[title="Close pane"]').click()`)
  const remaining = paneIds.length - 1
  await browser.waitUntil(
    () => browser.execute(`return (${PANE_WRAP_JS}).length === ${remaining}`),
    { timeout: 10000, interval: 250, timeoutMsg: `Pane count did not drop to ${remaining}` },
  )
  return { survivorIds: paneIds.filter((id) => id !== targetId) }
}

async function settlesClean(survivorCount, label) {
  // Sample through and past the RO debounce (50ms), rAF pair, and the 1200ms
  // remount-cover hard timeout: geometry must be sane at every sample and no
  // cover may remain in the end.
  const sampleTimes = [100, 300, 800, 1600, 2500]
  let probes = []
  for (const t of sampleTimes) {
    await browser.pause(t - (sampleTimes[sampleTimes.indexOf(t) - 1] || 0))
    probes = await browser.execute(probePanes)
    expect(probes.length).toBe(survivorCount)
  }
  const bad = probes.filter((p) =>
    !p.canvas || p.hasCover ||
    Math.abs(p.scaleW - p.dpr) > 0.02 * p.dpr ||
    Math.abs(p.scaleH - p.dpr) > 0.02 * p.dpr,
  )
  if (bad.length > 0) {
    await browser.saveScreenshot(join(screenshotDir, `font-scale-bad-${label}.png`))
  }
  expect(bad).toEqual([])
  // Also ensure everything settled to the same shape across the last two samples.
  const finalRectEqual = probes.every(
    (p, i) => i === 0 || (Math.abs(p.rectW - probes[0].rectW) < 1 && Math.abs(p.rectH - probes[0].rectH) < 1),
  )
  return { probes, finalRectEqual }
}

describe('Font blow-up on pane close regression', function () {
  // Baseline cell dims keyed by pane id → surviving panes must keep them
  // exactly after any pane close (the blow-up shows as inflated cell dims).
  function assertCellsUnchanged(before, afterProbes, label) {
    const base = new Map(before.filter((p) => p.cell).map((p) => [p.paneId, p.cell]))
    const drift = afterProbes.filter((p) => {
      const b = base.get(p.paneId)
      if (!b || !p.cell) return true // missing probe counts as failure
      return Math.abs(p.cell.w - b.w) > 0.11 || Math.abs(p.cell.h - b.h) > 0.11
    })
    if (drift.length > 0) {
      console.error(`cell drift [${label}]`, JSON.stringify(drift))
    }
    expect(drift).toEqual([])
  }

  it('keeps canvas backing-store == CSS size * dpr when a top-row pane closes (cross-row move)', async function () {
    this.timeout(120000)
    await makeSpaceWithShells(3)

    const before = await browser.execute(probePanes)
    const beforeBad = before.filter((p) => Math.abs(p.scaleW - p.dpr) > 0.02 * p.dpr)
    expect(beforeBad).toEqual([])

    // Close pane index 1 (top-right in the default 2-over-1 layout).
    await closePaneByIndex(1)
    const { probes } = await settlesClean(2, 'top-right-close')
    assertCellsUnchanged(before, probes, 'top-right-close')
    for (const p of probes) console.log('post-close probe', JSON.stringify(p))
  })

  it('keeps geometry consistent when the bottom pane closes (pure flex reflow, no remount)', async function () {
    this.timeout(120000)
    // Reuse the running app if a grid exists, else create one.
    const hasGrid = await browser.execute(`return ${PANE_WRAP_JS}.length > 0`)
    if (!hasGrid) {
      await makeSpaceWithShells(3)
    } else {
      // Recreate a 3-pane space in the same session.
      await makeSpaceWithShells(3)
    }

    const before = await browser.execute(probePanes)

    // Close the LAST pane (bottom, full width) — survivors never remount.
    const { survivorIds } = await closePaneByIndex(2)
    const { probes } = await settlesClean(2, 'bottom-close')
    assertCellsUnchanged(before, probes, 'bottom-close')

    // Survivor .xterm-mount elements must be the SAME nodes (no remount).
    const remounted = await browser.execute(`return (() => {
      const ids = ${JSON.stringify(survivorIds)};
      const wraps = ${PANE_WRAP_JS};
      const mounts = ids.map((id) =>
        wraps.find((p) => p.getAttribute('data-pane-id') === id)?.querySelector('.xterm-mount'),
      )
      return (window.__survivorMounts || []).some((el, i) => el !== mounts[i])
    })()`)
    expect(remounted).toBe(false)
    for (const p of probes) console.log('post-close probe', JSON.stringify(p))
  })
})
