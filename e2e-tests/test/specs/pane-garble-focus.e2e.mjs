import { writeFileSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

import { e2eTrustRoot, setE2E, fillSpaceDir, trustViaProjectRoot } from '../helpers/trust.mjs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

const PROBE_DUMP = '/tmp/garble-probes.json'

// A permitted dir for every launch in this spec: the backend rejects /tmp
// (workspace.rs:31) and the modal's blank-dir fallback is /tmp. Module scope
// so both the trust helper and the modal flow can reach it. It needs no
// seeding: a repo-root sandbox dir is authorized by the app's implicit project
// root.
const trustDir = e2eTrustRoot('pane-garble-focus')

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

// DOM + buffer probe per pane (self-contained for browser.execute).
function probePanes() {
  const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
  const grid = grids[grids.length - 1]
  const out = []
  if (!grid) return out
  const dpr = window.devicePixelRatio || 1
  for (const pane of grid.querySelectorAll('.pane-wrap')) {
    const mount = pane.querySelector('.xterm-mount')
    const paneId = pane.getAttribute('data-pane-id')
    const canvases = Array.from(pane.querySelectorAll('.xterm-mount canvas'))
    const screen = pane.querySelector('.xterm-screen')
    const wrapRect = pane.getBoundingClientRect()
    if (!canvases.length) {
      out.push({ paneId, canvas: false, wrapTop: wrapRect.top, wrapLeft: wrapRect.left })
      continue
    }
    const canvas = canvases.reduce((a, b) => (b.width * b.height > a.width * a.height ? b : a))
    const rect = canvas.getBoundingClientRect()
    const term = (window.__athenaTermMap || {})[paneId]
    let cell = null
    let buf = null
    try {
      const dims = term?._core?._renderService?.dimensions
      if (dims) cell = { w: dims.css.cell.width, h: dims.css.cell.height }
    } catch {}
    try {
      const b = term?.buffer?.active
      if (b) buf = { viewportY: b.viewportY, baseY: b.baseY, length: b.length, cursorY: b.cursorY, rows: term.rows, cols: term.cols }
    } catch {}
    out.push({
      paneId,
      canvas: true,
      canvasCount: canvases.length,
      canvasClass: canvas.className || null,
      sameMountNode: window.__mountRefs ? window.__mountRefs[paneId] === mount : null,
      cell,
      buf,
      attrW: canvas.width,
      attrH: canvas.height,
      rectW: rect.width,
      rectH: rect.height,
      dpr,
      scaleW: rect.width > 0 ? canvas.width / rect.width : null,
      scaleH: rect.height > 0 ? canvas.height / rect.height : null,
      wrapTop: wrapRect.top,
      wrapLeft: wrapRect.left,
      hasCover: !!pane.querySelector('.xterm-remount-cover'),
      screenRect: screen ? screen.getBoundingClientRect().toJSON() : null,
    })
  }
  return out
}

// Pixel probe: blit the largest canvas of each pane into a 2D canvas and
// measure the vertical distribution of ink. A healthy pane shows one
// contiguous run of content rows ending at the prompt; a garbled pane shows
// ink fragments scattered vertically (many short runs with gaps). Works only
// if the WebGL context keeps its drawing buffer (preserveDrawingBuffer).
function pixelProbe() {
  const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
  const grid = grids[grids.length - 1]
  if (!grid) return []
  const work = document.createElement('canvas')
  const out = []
  for (const pane of grid.querySelectorAll('.pane-wrap')) {
    const paneId = pane.getAttribute('data-pane-id')
    const canvases = Array.from(pane.querySelectorAll('.xterm-mount canvas'))
    if (!canvases.length) {
      out.push({ paneId, ok: false, err: 'no-canvas' })
      continue
    }
    // Pick the WebGL canvas: a canvas that already has a GL context returns
    // null for getContext('2d'). The link/selection 2D layers are transparent
    // and would read as falsely blank.
    const glSignal = (c) => {
      try {
        return c.getContext('2d') === null
      } catch {
        return true
      }
    }
    const src =
      canvases.filter(glSignal).sort((a, b) => b.width * b.height - a.width * a.height)[0] ||
      canvases.reduce((a, b) => (b.width * b.height > a.width * a.height ? b : a))
    // GL canvases with preserveDrawingBuffer:false read back BLACK unless the
    // frame being read is still current: force a synchronous repaint in THIS
    // task before drawImage (same contract as xterm-glyph-coherence's
    // measureInk). Cross-task readback measured 0 ink while text was on screen.
    const term = (window.__athenaTermMap || {})[paneId]
    try {
      const svc = term && term._core && term._core._renderService
      if (svc && svc._renderRows) svc._renderRows(0, term.rows - 1)
      else if (svc && svc.renderRows) svc.renderRows(0, term.rows - 1)
    } catch {}
    // Model-side ink: buffer lines holding text. This is what the canvas must
    // be showing; an empty buffer legitimately renders blank (a pane the
    // trigger just added is still spawning its shell), so the phase assertion
    // keys on this rather than on raw pixels alone.
    let modelInkRows = null
    try {
      const b = term?.buffer?.active
      if (b) {
        modelInkRows = 0
        for (let i = 0; i < b.length; i++) {
          if ((b.getLine(i)?.translateToString() || '').trim()) modelInkRows++
        }
      }
    } catch {}
    work.width = src.width
    work.height = src.height
    const ctx = work.getContext('2d', { willReadFrequently: true })
    if (!ctx) {
      out.push({ paneId, ok: false, err: 'no-2d-context' })
      continue
    }
    ctx.drawImage(src, 0, 0)
    let data
    try {
      data = ctx.getImageData(0, 0, work.width, work.height).data
    } catch (e) {
      out.push({ paneId, ok: false, err: String(e) })
      continue
    }
    const W = work.width
    const H = work.height
    // Background reference: the most frequent color on a coarse grid. The
    // terminal background dominates the canvas. A corner sample is NOT safe:
    // content starts at row 0, so the top-left 8x8 read (84,84,84) against a
    // true background of (11,11,12) and every sampled row then read as ink
    // (measurement saturated at 3357/3360, identical for every pane).
    const hist = new Map()
    for (let y = 0; y < H; y += 3) {
      for (let x = 0; x < W; x += 3) {
        const i = (y * W + x) * 4
        const k = `${data[i] >> 3}_${data[i + 1] >> 3}_${data[i + 2] >> 3}`
        const e = hist.get(k) || [0, 0, 0, 0]
        e[0]++
        e[1] += data[i]
        e[2] += data[i + 1]
        e[3] += data[i + 2]
        hist.set(k, e)
      }
    }
    const bg = [0, 0, 0]
    let dominant = null
    for (const e of hist.values()) if (!dominant || e[0] > dominant[0]) dominant = e
    if (dominant) bg.splice(0, 3, dominant[1] / dominant[0], dominant[2] / dominant[0], dominant[3] / dominant[0])
    // Sample row luminance on a coarse grid (every 4th row, 32 x-samples).
    const rowInk = []
    let totalInkPx = 0
    for (let y = 0; y < H; y += 4) {
      let ink = 0
      for (let s = 0; s < 32; s++) {
        const x = Math.floor((s + 0.5) * (W / 32))
        const i = (y * W + x) * 4
        const d =
          Math.abs(data[i] - bg[0]) + Math.abs(data[i + 1] - bg[1]) + Math.abs(data[i + 2] - bg[2])
        if (d > 30) ink++
      }
      rowInk.push(ink)
      totalInkPx += ink
    }
    // Count contiguous ink runs (rows with any ink) and the largest gap.
    let runs = 0
    let inRun = false
    let lastInkRow = -1
    let maxGapRows = 0
    let gap = 0
    rowInk.forEach((ink, idx) => {
      if (ink > 0) {
        if (!inRun) runs++
        inRun = true
        lastInkRow = idx
        gap = 0
      } else if (inRun) {
        gap++
        if (gap > maxGapRows) maxGapRows = gap
      }
    })
    out.push({
      paneId,
      ok: true,
      picked: { idx: canvases.indexOf(src), total: canvases.length, gl: glSignal(src), cls: src.className || null },
      w: W,
      h: H,
      bg: bg.map((v) => Math.round(v)),
      modelInkRows,
      totalInkPx,
      inkRuns: runs,
      maxGapSamples: maxGapRows,
      lastInkRow,
      sampledRows: rowInk.length,
      blank: totalInkPx < 4,
    })
  }
  return out
}

// Every trigger phase must assert rendering, for every pane — not once,
// before the trigger (the defect this spec was named for: a regression that
// blanked all panes after the Add Shell reflow used to pass green).
//
// A pane is asserted when the probe got pixels (ok) AND the terminal model
// holds text (modelInkRows > 0). A blank canvas over an empty buffer is
// correct — the pane the trigger just added is still spawning its shell — so
// excluding it is precision, not relaxation. A blank canvas over a pane with
// content is exactly the garble this spec exists to catch.
function assertAllPanesRendered(label, pixel) {
  const measured = pixel.filter((p) => p.ok)
  const errs = pixel.filter((p) => !p.ok)
  const content = measured.filter((p) => p.modelInkRows > 0)
  const blank = content.filter((p) => p.blank)
  console.log(
    `[garble:${label}] ink ${content.length - blank.length}/${content.length} content panes non-blank; ` +
      `probe ok=${measured.length} err=${errs.length} ` +
      JSON.stringify(pixel.map((p) => [p.paneId, p.ok ? p.totalInkPx : `err:${p.err}`, p.ok ? p.modelInkRows : null])),
  )
  if (!content.length) {
    // Distinct from a blank pane: nothing was renderable at all. Report the
    // ok/err split so a DOM-renderer fallback (no canvas → getImageData never
    // runs) is not misreported as "every pane is blank".
    throw new Error(
      `[${label}] no pane with renderable content was measured — probe ok=${measured.length} err=${errs.length} ` +
        `errReasons=${JSON.stringify(errs.map((p) => `${p.paneId}:${p.err ?? 'unknown'}`))}`,
    )
  }
  expect(blank.map((p) => `${p.paneId}:${p.totalInkPx}px over ${p.modelInkRows} content rows`)).toEqual([])
}

// Snapshot per-pane mount nodes so remounts are detectable later.
async function snapshotMountRefs() {
  await browser.execute(() => {
    const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
    const grid = grids[grids.length - 1]
    window.__mountRefs = {}
    if (!grid) return
    for (const pane of grid.querySelectorAll('.pane-wrap')) {
      window.__mountRefs[pane.getAttribute('data-pane-id')] = pane.querySelector('.xterm-mount')
    }
  })
}

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
    modalOpen: !!document.querySelector('.modal-backdrop, [role="dialog"], .palette-overlay'),
  }
})()`

// Ensure an active workspace with at least `count` shell panes. The store is
// polluted between runs (app boots into a persisted space with no "New
// Workspace" button), so prefer reusing the active space and topping up via
// the toolbar "Add Shell" button; fall back to the modal flow when there is
// no active space at all.
async function ensureSpaceWithShells(count) {
  await setE2E()
  await trustViaProjectRoot(browser)
  await browser.pause(1500)
  let state = await browser.execute(`return ${APP_STATE_JS}`)
  console.log('[garble] initial app state:', JSON.stringify(state))

  if (!state.hasAddShell && state.paneCount === 0 && state.hasNewWorkspace) {
    await makeSpaceViaModal(count)
  } else if (state.hasAddShell) {
    // Reuse the persisted space; top up panes via the real Add Shell path.
    let panes = state.paneCount
    for (let i = 0; i < 10 && panes < count; i++) {
      await browser.execute(() => {
        Array.from(document.querySelectorAll('button'))
          .find((b) => (b.getAttribute('title') || '').startsWith('Add Shell'))
          ?.click()
      })
      const expected = panes + 1
      await browser.waitUntil(
        () => browser.execute(`return (${PANE_WRAP_JS}).length >= ${expected}`),
        { timeout: 15000, interval: 250, timeoutMsg: `Pane count did not reach ${expected}` },
      )
      panes = expected
      await browser.pause(1000)
    }
    if (panes < count) throw new Error(`could not reach ${count} panes (stuck at ${panes})`)
    await browser.waitUntil(
      () =>
        browser.execute(
          (n) => document.querySelectorAll('.xterm-mount canvas').length >= n,
          count,
        ),
      { timeout: 20000, interval: 500, timeoutMsg: 'terminal canvases did not appear' },
    )
    // Let fits, RO debounce, font-settle remeasure finish before baselines.
    await browser.pause(3000)
  } else {
    const btns = await browser.execute(() =>
      Array.from(document.querySelectorAll('button'))
        .map((b) => (b.textContent || '').trim())
        .filter(Boolean)
        .slice(0, 40),
    )
    throw new Error(
      `No path to a workspace: state=${JSON.stringify(state)} buttons=${JSON.stringify(btns)}`,
    )
  }
}

async function makeSpaceViaModal(count) {
  expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((btn) =>
          (btn.textContent || '').includes('Terminal Workspace'),
        ),
      ),
    { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
  )
  expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
  await fillSpaceDir(browser, trustDir)
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
    timeout: 10000,
    interval: 250,
    timeoutMsg: 'Agents count label did not appear',
  })
  const initial = await readAgentCount()
  for (let n = 1; n <= count - initial; n++) {
    await browser.execute(() => document.getElementById('add-shell')?.click())
    const expected = initial + n
    await browser.waitUntil(async () => (await readAgentCount()) >= expected, {
      timeout: 5000,
      interval: 250,
      timeoutMsg: `Agents count did not reach ${expected}`,
    })
  }
  expect((await clickButtonByText('Launch Space')).ok).toBe(true)
  await browser.waitUntil(
    () =>
      browser.execute((n) => {
        const mounts = Array.from(document.querySelectorAll('.xterm-mount'))
        const ready = mounts.filter((m) => !!m.querySelector('.xterm'))
        return mounts.length === n && ready.length === n
      }, count),
    { timeout: 30000, interval: 500, timeoutMsg: `${count} terminal panes did not mount` },
  )
  await browser.waitUntil(
    () =>
      browser.execute(
        (n) => document.querySelectorAll('.xterm-mount canvas').length >= n,
        count,
      ),
    { timeout: 20000, interval: 500, timeoutMsg: 'terminal canvases did not appear' },
  )
  await browser.pause(3000)
}

describe('Pane garble after add-shell reflow repro', function () {
  it('captures per-pane DOM/pixel evidence around the add-shell reflow and the scroll fix', async function () {
    this.timeout(180000)

    const dump = { meta: { startedAt: new Date().toISOString() }, samples: [] }
    const record = (label, dom, pixel, extra = {}) => {
      dump.samples.push({ label, at: Date.now(), dom, pixel, ...extra })
      console.log(`[garble:${label}] dom=`, JSON.stringify(dom))
      console.log(`[garble:${label}] pixel=`, JSON.stringify(pixel))
    }

    await ensureSpaceWithShells(4)
    await snapshotMountRefs()

    // Give every pane real content — a reflow of idle prompts proves nothing
    // about reflow garble. Route: window.__TAURI__.core.invoke('pty_write'),
    // the one proven to work for SHELL panes (xterm-glyph-coherence.e2e.mjs
    // :447-449, arrival asserted at :460). term.paste() is proven only for OMP
    // panes (omp-typing.e2e.mjs:187-198); used on shell panes here it wrote
    // nothing (5/5 runs: baseY 0, length = term.rows, no `seq` output).
    const paneIds = await browser.execute(
      `return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`,
    )
    for (const id of paneIds) {
      await browser.execute(
        (paneId) =>
          window.__TAURI__.core.invoke('pty_write', { id: paneId, data: 'seq 1 120 | nl -ba\n' }),
        id,
      )
      await browser.pause(400)
    }
    await browser.pause(2500)

    // The content must actually be in the panes before any garble phase runs.
    // `nl -ba` right-justifies the number and separates it with a TAB, which
    // xterm stores as spaces, so the tail line reads "   120        120".
    const contentArrival = await browser.execute(() =>
      Object.entries(window.__athenaTermMap || {}).map(([paneId, term]) => {
        const b = term.buffer.active
        const lines = []
        for (let i = 0; i < b.length; i++) lines.push(b.getLine(i)?.translateToString() || '')
        return {
          paneId,
          baseY: b.baseY,
          length: b.length,
          rows: term.rows,
          numbered: lines.filter((t) => /^\s*\d+\s+\d+\s*$/.test(t)).length,
          sawSeqTail: lines.some((t) => /^\s*120\s+120\s*$/.test(t)),
        }
      }),
    )
    dump.contentArrival = contentArrival
    console.log('[garble] content arrival:', JSON.stringify(contentArrival))
    const arrived = contentArrival.filter((p) => p.baseY > 0 && p.sawSeqTail)
    console.log(`[garble] content arrived: ${arrived.length}/${contentArrival.length} panes`)
    expect(arrived.length).toBe(contentArrival.length)

    const paneCountBefore = paneIds.length
    dump.meta.paneCountBefore = paneCountBefore

    // Record + assert one phase. Every phase the spec is named for goes
    // through this, so no post-trigger phase can go unasserted.
    const capturePhase = async (label, extra = {}) => {
      const dom = await browser.execute(probePanes)
      const pixel = await browser.execute(pixelProbe)
      record(label, dom, pixel, extra)
      assertAllPanesRendered(label, pixel)
      return pixel
    }

    await capturePhase(`baseline-${paneCountBefore}-panes`)

    // Trigger: the real in-app add-shell path (toolbar "Add Shell" button →
    // add_pane_to_space → grid reflow + remounts).
    const added = await browser.execute(() => {
      const btn = Array.from(document.querySelectorAll('button')).find((b) =>
        (b.getAttribute('title') || '').startsWith('Add Shell'),
      )
      if (!btn) return false
      btn.click()
      return true
    })
    expect(added).toBe(true)
    await browser.waitUntil(
      () => browser.execute(`return (${PANE_WRAP_JS}).length === ${paneCountBefore + 1}`),
      {
        timeout: 15000,
        interval: 250,
        timeoutMsg: `Pane count did not reach ${paneCountBefore + 1} after Add Shell`,
      },
    )

    for (const delay of [100, 400, 1000]) {
      await browser.pause(delay)
      const label = `post-addshell-t${[100, 500, 1500][[100, 400, 1000].indexOf(delay)]}`
      await capturePhase(label)
      await browser.saveScreenshot(join(screenshotDir, `pane-garble-${label}.png`))
    }

    // Focus trigger: user reported clicking/focusing a top pane worsens it.
    // Dispatch pointer+click on the first pane (top-left) exactly like a user.
    await browser.execute(() => {
      const grid = Array.from(document.querySelectorAll('.workspace-grid-root')).pop()
      const first = grid?.querySelector('.pane-wrap .xterm-mount')
      if (!first) return
      const r = first.getBoundingClientRect()
      const x = r.left + r.width / 2
      const y = r.top + r.height / 2
      for (const type of ['pointerdown', 'mousedown', 'pointerup', 'mouseup', 'click']) {
        first.dispatchEvent(
          new PointerEvent(type, { bubbles: true, clientX: x, clientY: y, button: 0 }),
        )
      }
      window.__athenaTermMap?.[
        first.closest('.pane-wrap')?.getAttribute('data-pane-id')
      ]?.focus()
    })
    for (const delay of [100, 400, 1000]) {
      await browser.pause(delay)
      const label = `post-focus-t${[100, 500, 1500][[100, 400, 1000].indexOf(delay)]}`
      await capturePhase(label)
      await browser.saveScreenshot(join(screenshotDir, `pane-garble-${label}.png`))
    }

    // The blank-state diagnostic that used to sit here (`term.refresh(0,
    // rows-1)` on a blank-reading pane, re-probe, and a
    // 'refresh-restored-ink (lost-dirty-range repaint)' verdict) is deleted:
    // pixelProbe already forces a repaint of the frame it reads, which is the
    // fix this file exists for — so the "lost a repaint" readback the
    // comparison was keyed on is not a valid signal at all, and its verdict
    // branches could never fire. A blank pane is now caught directly, per
    // phase, by assertAllPanesRendered above. The per-phase, per-pane ink the
    // dump carries is the diagnostic.

    // Scroll-fix probe: scroll every pane down 1 line and back up, re-probe.
    const scrollResult = await browser.execute(() =>
      Object.entries(window.__athenaTermMap || {}).map(([paneId, term]) => {
        const before = {
          viewportY: term.buffer.active.viewportY,
          baseY: term.buffer.active.baseY,
        }
        term.scrollLines(1)
        const afterDown = {
          viewportY: term.buffer.active.viewportY,
          baseY: term.buffer.active.baseY,
        }
        term.scrollLines(-1)
        return { paneId, before, afterDown }
      }),
    )
    dump.scrollLinesResult = scrollResult
    console.log('[garble:scroll-lines] ', JSON.stringify(scrollResult))

    // Wheel-scroll the way the user does (xterm listens to 'wheel' on the
    // mount), then re-probe to see if ink runs normalize.
    await browser.execute(() => {
      const grid = Array.from(document.querySelectorAll('.workspace-grid-root')).pop()
      for (const wrap of grid?.querySelectorAll('.pane-wrap') || []) {
        const mount = wrap.querySelector('.xterm-mount')
        const r = mount?.getBoundingClientRect()
        if (!mount) continue
        mount.dispatchEvent(
          new WheelEvent('wheel', {
            bubbles: true,
            cancelable: true,
            deltaY: 40,
            clientX: r.left + r.width / 2,
            clientY: r.top + r.height / 2,
          }),
        )
      }
    })
    await browser.pause(300)
    await capturePhase('post-wheelscroll')
    await browser.pause(500)
    await capturePhase('post-scroll', { scrollResult })
    await browser.saveScreenshot(join(screenshotDir, 'pane-garble-post-scroll.png'))

    // Buffer-dump evidence for each pane: where is ink in the model?
    const bufferDump = await browser.execute(() =>
      Object.entries(window.__athenaTermMap || {}).map(([paneId, term]) => {
        const b = term.buffer.active
        const rows = []
        for (let i = 0; i < b.length; i++) {
          const t = b.getLine(i)?.translateToString() || ''
          if (t.trim()) rows.push(i)
        }
        return {
          paneId,
          viewportY: b.viewportY,
          baseY: b.baseY,
          length: b.length,
          inkRowIndices: rows,
          cols: term.cols,
          rowsCount: term.rows,
        }
      }),
    )
    dump.bufferDump = bufferDump

    writeFileSync(PROBE_DUMP, JSON.stringify(dump, null, 2))
    console.log(`[garble] full probe dump written to ${PROBE_DUMP}`)
  })
})
