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

  // Regression for the shared glyph atlas (xterm.js #6014): all same-config
  // panes share ONE CharAtlasCache atlas, and clearTextureAtlas() only
  // rebuilds the CALLING terminal's render model. Sibling panes keep per-cell
  // UVs into the wiped atlas and paint mis-sliced glyph fragments until their
  // rows are rewritten — the "randomly garbled terminal" symptom. The fix
  // makes reset_glyph_atlas force a full repaint on EVERY live terminal.
  //
  // Drive the REAL reset path via window.__athenaResetAtlas (exposed by
  // XtermMount in e2e mode); a bare term.clearTextureAtlas() from JS would
  // bypass the sibling repair and prove nothing.
  it('repaints sibling panes when one pane resets the shared atlas', async function () {
    this.timeout(180000)

    await ensureSpaceWithShell()
    // Need TWO panes: an initiator and an idle sibling left untouched.
    let paneIds = await browser.execute(
      `return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`,
    )
    if (paneIds.length < 2) {
      const before = paneIds.length
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
      paneIds = await browser.execute(
        `return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`,
      )
    }
    const [initiator, sibling] = paneIds
    console.log('[shared] initiator:', initiator, 'sibling:', sibling)

    // Fill the sibling with numbered lines so any glyph corruption is visible
    // as wrong/mis-sliced text, then let it go idle.
    await browser.execute(
      (id) => window.__TAURI__.core.invoke('pty_write', { id, data: 'seq 1 40 | nl -ba\n' }),
      sibling,
    )
    await browser.pause(2500)

    // Snapshot the sibling's render state (cell text + which rows repaint).
    const snapshot = `((id) => {
      const term = (window.__athenaTermMap || {})[id]
      if (!term) return { ok: false, reason: 'no term' }
      const buf = term.buffer.active
      const lines = []
      for (let y = 0; y < Math.min(buf.length, 8); y++) {
        lines.push(buf.getLine(y)?.translateToString(true) ?? null)
      }
      // Instrument clearTextureAtlas on THIS instance to observe the sibling
      // repair (the fix calls sibling.clearTextureAtlas(), which clears the
      // sibling renderer's own render model — a bare refresh diffs rows and
      // would keep stale UVs, so we assert on the model-clearing call).
      if (!term.__athenaClearLog) {
        term.__athenaClearLog = []
        const origClear = term.clearTextureAtlas.bind(term)
        term.clearTextureAtlas = () => {
          term.__athenaClearLog.push({ t: performance.now() })
          return origClear()
        }
      }
      term.__athenaClearLog.length = 0
      return { ok: true, lines }
    })`
    const beforeSnap = await browser.execute(`return (${snapshot})(${JSON.stringify(sibling)})`)
    console.log('[shared] sibling before:', JSON.stringify(beforeSnap))
    expect(beforeSnap.ok).toBe(true)

    // Trigger the REAL reset path on the INITIATOR only.
    const hookType = await browser.execute('return typeof window.__athenaResetAtlas')
    expect(hookType).toBe('function')
    await browser.execute(`window.__athenaResetAtlas(${JSON.stringify(initiator)})`)
    await browser.pause(1500)

    // The sibling MUST have received its own clearTextureAtlas (its render
    // model rebuilds against the cleared atlas instead of keeping stale UVs).
    // Pre-fix nobody calls the sibling's clearTextureAtlas from a reset
    // triggered on another pane, so this assertion fails before the fix.
    const repair = await browser.execute(
      `return ((id) => {
        const term = (window.__athenaTermMap || {})[id]
        if (!term || !term.__athenaClearLog) return { ok: false }
        return { ok: true, clears: term.__athenaClearLog.length }
      })(${JSON.stringify(sibling)})`,
    )
    console.log('[shared] sibling repair:', JSON.stringify(repair))
    expect(repair.ok).toBe(true)
    expect(repair.clears).toBeGreaterThan(0)

    // And the sibling's top rows must still hold the original text.
    const afterSnap = await browser.execute(`return (${snapshot})(${JSON.stringify(sibling)})`)
    console.log('[shared] sibling after:', JSON.stringify(afterSnap.lines))
    expect(afterSnap.ok).toBe(true)
    expect(afterSnap.lines).toEqual(beforeSnap.lines)
  })

  // Regression for the kitty-graphics placeholder leak+desync: when OMP draws
  // an inline image it sends an APC G payload followed by a rectangle of
  // U+10EEEE + combining-diacritic placeholder cells, advancing its cursor by
  // those cells. The addon must (a) strip placeholder runes while splicing
  // exactly one invisible NBSP sentinel per cell — preserving the sender's
  // cursor advance and keeping a findable anchor in the buffer — and
  // (b) paint the decoded image into the overlay layer over that block.
  // Pre-fix, stripped cells collapsed so trailing text landed on the image's
  // rows, and missing filters left tofu/missing-glyph grids behind.
  it('strips kitty placeholders into sentinel cells and paints the image overlay', async function () {
    this.timeout(120000)

    await ensureSpaceWithShell()
    const paneIds = await browser.execute(
      `return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`,
    )
    expect(paneIds.length).toBeGreaterThan(0)
    const paneId = paneIds[0]

    // Wait for this pane's kitty feed filter (attached on mount; retries
    // until the defer-loaded addon script evaluates).
    await browser.waitUntil(
      (id) =>
        browser.execute(
          `return typeof ((window.__athenaTermMap || {})[${JSON.stringify(id)}] || {}).__athenaKittyFeed === 'function'`,
        ),
      { timeout: 30000, interval: 250, timeoutMsg: 'kitty feed filter never attached' },
    )

    // 1x1 PNG; q=2 keeps the addon from answering into the PTY input queue,
    // C=1 suppresses the auto cursor-drop so the only cursor advance comes
    // from the 4 placeholder cells that follow.
    const result = await browser.execute(
      (id) => {
        const term = (window.__athenaTermMap || {})[id]
        if (!term) return { ok: false, reason: 'no term' }
        const PNG_B64 =
          'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
        const ESC = String.fromCharCode(27)
        const apc = `${ESC}_Ga=T,f=100,q=2,C=1,c=4,r=2;${PNG_B64}${ESC}\\`
        const enc = new TextEncoder()
        const cell = enc.encode('\u{10EEEE}\u0302') // one placeholder cell
        const payload = []
        payload.push(...enc.encode(apc), ...enc.encode('\r\n'))
        for (let n = 0; n < 4; n++) payload.push(...cell)
        payload.push(...enc.encode('MARK\r\n'))
        const filtered = term.__athenaKittyFeed(new Uint8Array(payload))
        if (filtered) term.write(filtered)
        return { ok: true, filtered: !!filtered }
      },
      paneId,
    )
    expect(result.ok).toBe(true)
    await browser.pause(800)

    const inspect = await browser.execute(
      (id) => {
        const term = (window.__athenaTermMap || {})[id]
        if (!term) return { ok: false, reason: 'no term' }
        const buf = term.buffer.active
        const PLACEHOLDER_RE = /\u{10EEEE}/u
        let placeholderLines = 0
        let markLine = null
        for (let y = 0; y < buf.length; y++) {
          const text = buf.getLine(y)?.translateToString(true) || ''
          if (PLACEHOLDER_RE.test(text)) placeholderLines++
          if (text.includes('MARK')) markLine = text
        }
        const layer = term.element && term.element.querySelector('.athena-kitty-layer')
        return {
          ok: true,
          placeholderLines,
          markLine,
          overlayChildren: layer ? layer.children.length : -1,
        }
      },
      paneId,
    )
    console.log('[kitty] inspect:', JSON.stringify(inspect))
    expect(inspect.ok).toBe(true)
    // (a) no placeholder runes in the grid, and the 4 stripped cells became
    // 4 NBSP sentinel cells  (that's what the refresh scan looks for) —
    // 'MARK' starts at column 4 of its line.
    expect(inspect.placeholderLines).toBe(0)
    expect(inspect.markLine).toBe('\u00a0\u00a0\u00a0\u00a0MARK')
    // (b) the image painted into the overlay layer.
    expect(inspect.overlayChildren).toBe(1)
  })
})
