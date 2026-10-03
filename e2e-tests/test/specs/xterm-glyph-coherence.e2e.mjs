// Regression guard for the "garbled text on a new terminal" class:
//
//  1. A pane's glyph atlas must be settled (and reset) BEFORE the first PTY
//     byte is written into its terminal. A char-size change after first paint
//     installs a new atlas while the render model keeps the old atlas' glyph
//     coordinates, which paints the wrong characters.
//  2. The WebGL drawing buffer must NOT be preserved. With it preserved, the
//     renderer's only GL clear happens on atlas-page churn, so cells whose
//     content is erased (TUI redraw, EL/ED, clear) keep their old glyph pixels
//     on the canvas — the stale-glyph soup in the reported screenshot.
//
// Check 2 is asserted end-to-end: write a full screen of numbered lines, clear
// the screen and push the prompt down, then measure ink in the band that the
// buffer says is blank. A healthy pane shows nothing there.
//
// Run:  tauri-wd &  &&  npx wdio run wdio.conf.mjs --spec test/specs/xterm-glyph-coherence.e2e.mjs

import { writeFileSync } from 'node:fs'

const DUMP = '/tmp/xterm-glyph-coherence.json'

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

/// Instrument Terminal BEFORE the pane under test mounts: record every
/// non-empty write and every clearTextureAtlas, tagged by instance.
async function installTerminalProbe() {
  return browser.execute(() => {
    if (window.__athenaGlyphProbe) return true
    const Original = window.Terminal
    if (typeof Original !== 'function') return false
    window.__athenaGlyphProbe = { log: [], instances: [] }
    const probe = window.__athenaGlyphProbe

    const originalWrite = Original.prototype.write
    Original.prototype.write = function (data) {
      try {
        const len = typeof data === 'string' ? data.length : (data && data.length) || 0
        if (len > 0) {
          probe.log.push({ id: this.__athenaGlyphId, kind: 'write', len, t: performance.now() })
        }
      } catch {}
      return originalWrite.apply(this, arguments)
    }

    const originalClear = Original.prototype.clearTextureAtlas
    if (typeof originalClear === 'function') {
      Original.prototype.clearTextureAtlas = function () {
        try {
          probe.log.push({ id: this.__athenaGlyphId, kind: 'atlas-reset', t: performance.now() })
        } catch {}
        return originalClear.apply(this, arguments)
      }
    }

    let nextId = 0
    window.Terminal = new Proxy(Original, {
      construct(target, args, newTarget) {
        const instance = Reflect.construct(target, args, newTarget)
        try {
          instance.__athenaGlyphId = ++nextId
          probe.instances.push(instance)
        } catch {}
        return instance
      },
    })
    return true
  })
}

/// WebGL context attributes actually in use for a pane's renderer canvas.
async function readGLAttributes(paneId) {
  return browser.execute((paneId) => {
    const pane = document.querySelector(`.pane-wrap[data-pane-id="${paneId}"]`)
    const canvases = Array.from(pane?.querySelectorAll('.xterm-mount canvas') || [])
    const isGL = (c) => {
      try {
        return c.getContext('2d') === null
      } catch {
        return true
      }
    }
    const canvas = canvases.filter(isGL).sort((a, b) => b.width * b.height - a.width * a.height)[0]
    if (!canvas) return { paneId, ok: false, reason: 'no GL canvas' }
    let attrs = null
    try {
      attrs = canvas.getContext('webgl2')?.getContextAttributes() || null
    } catch {}
    return {
      paneId,
      ok: true,
      canvasCount: canvases.length,
      preserveDrawingBuffer: attrs ? attrs.preserveDrawingBuffer : null,
      canvasW: canvas.width,
      canvasH: canvas.height,
    }
  }, paneId)
}

/// Ink pixels inside a vertical band of a pane's renderer canvas, plus the
/// whole-canvas total so a blind probe (nothing readable at all) is visible.
async function measureInk(paneId, topFrac, bottomFrac) {
  return browser.execute(
    (paneId, topFrac, bottomFrac) => {
      const pane = document.querySelector(`.pane-wrap[data-pane-id="${paneId}"]`)
      const canvases = Array.from(pane?.querySelectorAll('.xterm-mount canvas') || [])
      const isGL = (c) => {
        try {
          return c.getContext('2d') === null
        } catch {
          return true
        }
      }
      const src =
        canvases.filter(isGL).sort((a, b) => b.width * b.height - a.width * a.height)[0] || null
      if (!src || !src.width || !src.height) return { paneId, ok: false }
      const work = document.createElement('canvas')
      work.width = src.width
      work.height = src.height
      const ctx = work.getContext('2d', { willReadFrequently: true })
      if (!ctx) return { paneId, ok: false }
      ctx.drawImage(src, 0, 0)
      let data
      try {
        data = ctx.getImageData(0, 0, work.width, work.height).data
      } catch (e) {
        return { paneId, ok: false, err: String(e) }
      }
      const W = work.width
      const H = work.height
      let bg = [0, 0, 0]
      {
        let n = 0
        for (let y = 0; y < Math.min(8, H); y++) {
          for (let x = 0; x < Math.min(8, W); x++) {
            const i = (y * W + x) * 4
            bg[0] += data[i]
            bg[1] += data[i + 1]
            bg[2] += data[i + 2]
            n++
          }
        }
        bg = bg.map((v) => v / n)
      }
      const inkOf = (fromY, toY) => {
        let ink = 0
        for (let y = Math.max(0, fromY); y < Math.min(H, toY); y++) {
          for (let x = 0; x < W; x += 2) {
            const i = (y * W + x) * 4
            const d =
              Math.abs(data[i] - bg[0]) +
              Math.abs(data[i + 1] - bg[1]) +
              Math.abs(data[i + 2] - bg[2])
            if (d > 30) ink++
          }
        }
        return ink
      }
      return {
        paneId,
        ok: true,
        w: W,
        h: H,
        bg: bg.map((v) => Math.round(v)),
        bandInk: inkOf(Math.round(topFrac * H), Math.round(bottomFrac * H)),
        totalInk: inkOf(0, H),
      }
    },
    paneId,
    topFrac,
    bottomFrac,
  )
}

async function ensureSpaceWithShell() {
  await browser.execute(() => {
    window.__athenaE2E = true
    window.__TAURI__.core.invoke('workspace_add_trusted_root', { dir: '/tmp' }).catch(() => {})
  })
  await browser.pause(1500)
  const state = await browser.execute(`return ${APP_STATE_JS}`)
  console.log('[glyph] app state:', JSON.stringify(state))

  if (state.hasAddShell && state.paneCount > 0) {
    await browser.waitUntil(
      () => browser.execute((n) => document.querySelectorAll('.xterm-mount canvas').length >= n, state.paneCount),
      { timeout: 20000, interval: 500, timeoutMsg: 'existing terminal canvases did not appear' },
    )
    return
  }

  expect(state.hasNewWorkspace).toBe(true)
  await browser.execute(() => {
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').includes('New Workspace'))
      ?.click()
  })
  await browser.waitUntil(
    () =>
      browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((b) =>
          (b.textContent || '').includes('Terminal Workspace'),
        ),
      ),
    { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
  )
  await browser.execute(() => {
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').includes('Terminal Workspace'))
      ?.click()
  })
  await browser.waitUntil(() => browser.execute(() => !!document.getElementById('add-shell')), {
    timeout: 10000,
    interval: 250,
    timeoutMsg: 'Terminal configuration step did not appear',
  })
  await browser.execute(() => {
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').trim() === 'Launch Space')
      ?.click()
  })
  await browser.waitUntil(
    () => browser.execute(() => document.querySelectorAll('.xterm-mount').length > 0),
    { timeout: 30000, interval: 500, timeoutMsg: 'terminal panes did not mount' },
  )
  await browser.pause(3000)
}

describe('xterm glyph coherence', function () {
  it('resets the atlas before the first byte and clears erased glyphs', async function () {
    this.timeout(180000)
    const dump = { startedAt: new Date().toISOString() }

    await ensureSpaceWithShell()

    // Instrument BEFORE the new pane's Terminal is constructed.
    const probeInstalled = await installTerminalProbe()
    expect(probeInstalled).toBe(true)

    const before = await browser.execute(`return (${PANE_WRAP_JS}).length`)
    await browser.execute(() => {
      Array.from(document.querySelectorAll('button'))
        .find((b) => (b.getAttribute('title') || '').startsWith('Add Shell'))
        ?.click()
    })
    await browser.waitUntil(
      () => browser.execute(`return (${PANE_WRAP_JS}).length === ${before + 1}`),
      { timeout: 20000, interval: 250, timeoutMsg: 'Add Shell did not add a pane' },
    )
    // Give the mount handshake (layout commit -> font settle -> atlas reset ->
    // fit -> PTY resize -> release) room to complete on a cold pane.
    await browser.pause(4000)

    const paneIds = await browser.execute(
      `return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`,
    )
    const paneId = paneIds[paneIds.length - 1]
    dump.paneId = paneId
    dump.paneCount = paneIds.length

    // ── 1. Ordering invariant: atlas reset precedes the first content write ──
    const ordering = await browser.execute((paneId) => {
      const probe = window.__athenaGlyphProbe
      if (!probe) return { ok: false, reason: 'probe missing' }
      const term = (window.__athenaTermMap || {})[paneId]
      const id = term?.__athenaGlyphId
      const events = probe.log.filter((e) => e.id === id)
      const firstWrite = events.find((e) => e.kind === 'write' && e.len > 0) || null
      const firstReset = events.find((e) => e.kind === 'atlas-reset') || null
      return {
        ok: true,
        instanceId: id ?? null,
        firstWrite: firstWrite && firstWrite.t,
        firstReset: firstReset && firstReset.t,
        resets: events.filter((e) => e.kind === 'atlas-reset').length,
        writes: events.filter((e) => e.kind === 'write').length,
        resetBeforeFirstWrite:
          !!firstReset && (!firstWrite || firstReset.t <= firstWrite.t),
      }
    }, paneId)
    dump.ordering = ordering
    console.log('[glyph] ordering:', JSON.stringify(ordering))
    expect(ordering.ok).toBe(true)
    expect(ordering.firstWrite).not.toBe(null)
    expect(ordering.resetBeforeFirstWrite).toBe(true)

    // ── 2. The drawing buffer must not be preserved ─────────────────────────
    const gl = await readGLAttributes(paneId)
    dump.gl = gl
    console.log('[glyph] gl:', JSON.stringify(gl))
    expect(gl.ok).toBe(true)
    expect(gl.preserveDrawingBuffer).toBe(false)

    // ── 3. Erased content must not leave glyph pixels behind ────────────────
    // Fill the screen, then clear it and push the prompt several rows down so
    // the top band is blank in the buffer. Any ink there is a stale ghost.
    await browser.execute(
      (id) => window.__TAURI__.core.invoke('pty_write', { id, data: 'seq 1 80 | nl -ba\n' }),
      paneId,
    )
    await browser.pause(2500)
    const filled = await measureInk(paneId, 0.05, 0.35)
    dump.filled = filled
    console.log('[glyph] filled:', JSON.stringify(filled))
    expect(filled.ok).toBe(true)
    // Sanity: the probe must be able to see text at all.
    expect(filled.bandInk).toBeGreaterThan(50)

    await browser.execute(
      (id) =>
        window.__TAURI__.core.invoke('pty_write', {
          id,
          data: "clear; printf '\\n\\n\\n\\n\\n\\n'\n",
        }),
      paneId,
    )
    await browser.pause(2500)
    const cleared = await measureInk(paneId, 0.02, 0.15)
    dump.cleared = cleared
    console.log('[glyph] cleared:', JSON.stringify(cleared))
    expect(cleared.ok).toBe(true)
    // Prompt is still on screen (probe alive) but the cleared band is empty.
    expect(cleared.totalInk).toBeGreaterThan(50)
    expect(cleared.bandInk).toBeLessThan(40)

    writeFileSync(DUMP, JSON.stringify(dump, null, 2))
    console.log(`[glyph] dump written to ${DUMP}`)
  })

  // Regression for the display:none occlusion bug: WKWebView reclaims the GL
  // context of hidden panes; before the fix the pane fell back to the DOM
  // renderer permanently and painted scattered glyphs until a remount/resize.
  // Force a REAL context loss via WEBGL_lose_context, wait out the renderer's
  // grace period so our onContextLoss handler disposes the addon, then restore
  // the context and assert WebGL is re-attached with a fresh glyph atlas.
  it('re-attaches WebGL and rebuilds the atlas after a forced context loss', async function () {
    this.timeout(180000)

    await ensureSpaceWithShell()
    const probeInstalled = await installTerminalProbe()
    expect(probeInstalled).toBe(true)

    const before = await browser.execute(`return (${PANE_WRAP_JS}).length`)
    await browser.execute(() => {
      Array.from(document.querySelectorAll('button'))
        .find((b) => (b.getAttribute('title') || '').startsWith('Add Shell'))
        ?.click()
    })
    await browser.waitUntil(
      () => browser.execute(`return (${PANE_WRAP_JS}).length === ${before + 1}`),
      { timeout: 20000, interval: 250, timeoutMsg: 'Add Shell did not add a pane' },
    )
    await browser.pause(4000)

    const paneIds = await browser.execute(
      `return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`,
    )
    const paneId = paneIds[paneIds.length - 1]

    const glBefore = await readGLAttributes(paneId)
    console.log('[ctxloss] gl before:', JSON.stringify(glBefore))
    expect(glBefore.ok).toBe(true)

    // Baseline: how many atlas resets the probe has seen so far.
    const resetsBefore = await browser.execute(
      () => window.__athenaGlyphProbe.log.filter((e) => e.kind === 'atlas-reset').length,
    )

    // Force lose. The extension object must come from the SAME canvas the
    // renderer owns; it stays callable on the lost context.
    const lost = await browser.execute((paneId) => {
      const pane = document.querySelector(`.pane-wrap[data-pane-id="${paneId}"]`)
      const canvases = Array.from(pane?.querySelectorAll('.xterm-mount canvas') || [])
      const canvas = canvases
        .filter((c) => {
          try {
            return c.getContext('2d') === null
          } catch {
            return true
          }
        })
        .sort((a, b) => b.width * b.height - a.width * a.height)[0]
      if (!canvas) return { ok: false, reason: 'no GL canvas' }
      const ctx = canvas.getContext('webgl2') || canvas.getContext('webgl')
      const ext = ctx && ctx.getExtension('WEBGL_lose_context')
      if (!ext) return { ok: false, reason: 'no WEBGL_lose_context' }
      window.__athenaLoseExt = ext
      ext.loseContext()
      return { ok: true }
    }, paneId)
    console.log('[ctxloss] lose:', JSON.stringify(lost))
    expect(lost.ok).toBe(true)

    // Wait past the renderer's internal ~3s grace for our onContextLoss
    // handler (dispose → DOM fallback) to actually run.
    await browser.pause(5000)

    // Restore the context; our webglcontextrestored listener must re-attach
    // a fresh WebglAddon and reset the glyph atlas.
    await browser.execute(() => window.__athenaLoseExt?.restoreContext())
    await browser.pause(3000)

    const glAfter = await readGLAttributes(paneId)
    const resetsAfter = await browser.execute(
      () => window.__athenaGlyphProbe.log.filter((e) => e.kind === 'atlas-reset').length,
    )
    console.log(
      '[ctxloss] gl after:',
      JSON.stringify(glAfter),
      'resets:',
      resetsBefore,
      '→',
      resetsAfter,
    )
    // WebGL re-attached: the largest canvas is a GL canvas again.
    expect(glAfter.ok).toBe(true)
    expect(glAfter.preserveDrawingBuffer).toBe(false)
    // The restore path rebuilt the atlas.
    expect(resetsAfter).toBeGreaterThan(resetsBefore)
  })
})
