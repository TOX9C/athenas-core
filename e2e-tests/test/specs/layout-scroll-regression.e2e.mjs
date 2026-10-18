// Regression: adding a shell pane used to leave an existing OMP pane stuck
// scrolled up (one pane stuck mid-buffer, the other pinned — racy). Root
// cause: the fit path fired xterm onResize → pty_resize → SIGWINCH, and the
// TUI's deferred reprint (normal↔alt buffer toggle, history reprint) landed
// AFTER the one-shot viewport restore, with nothing to re-pin the pane.
// Fix: xterm_mount/xterm_helpers keep a follow-bottom intent window across
// the redraw settle and re-pin at the write path plus on delayed timers.
// This spec drives the real app end-to-end: two OMP panes with resumed
// sessions (deep scrollback), then two more panes added via the toolbar,
// asserting (a) bottom-following panes finish pinned, (b) no pane remounts,
// (c) a user-scrolled pane keeps its distance from the bottom.

import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

import { e2eTrustRoot, seedTrustedRoots, setE2E, fillSpaceDir, trustViaProjectRoot } from '../helpers/trust.mjs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')
// Trusted root this spec uses. A dir under the repo root is permitted: it is
// not a forbidden prefix (`is_forbidden_root`, workspace.rs:31) and the app's
// own project root is implicitly trusted, so descendants pass the sandbox
// check. /tmp is rejected by that gate, and `__athenaE2E` is a frontend-only
// bypass, so a /tmp root can never authorize a PTY spawn.
const repoRoot = e2eTrustRoot('layout-scroll-regression')

// The app may restore a previous session's workspace (specs share the
// persisted store), so terminal panes can already be mounted before a test's
// own New Workspace flow. Guards below only drive the modal when nothing is
// mounted yet — same convention as font-size-shortcuts.e2e.mjs. Deliberately
// loose: a mount restored at startup may not yet carry the renderer
// attribute or the `.xterm` child when this runs, so matching those would
// race and re-open the modal on top of existing panes.
const hasTerminal = () =>
  browser.execute(() => !!document.querySelector('.xterm-mount'))

async function clickButtonByText(text, { partial = false } = {}) {
  return browser.execute(
    ({ text, partial }) => {
      for (const button of document.querySelectorAll('button')) {
        const content = (button.textContent || '').trim()
        if ((partial ? content.includes(text) : content === text)) {
          button.click()
          return { ok: true, content }
        }
      }
      return { ok: false, text }
    },
    { text, partial },
  )
}

// Set by the add-shell test's counter-case: the pane a user scrolled up. It
// must stay where the user left it, so the resize test does not require it to
// be bottom-pinned.
let userScrolledPaneId = null

// Panes the assertions should require to be bottom-pinned. The pane the
// add-shell test scrolled up by hand is correct to stay where the user left
// it, so it is excluded everywhere a gate demands bottom-pinning.
// Panes this test did not create are excluded too: a workspace restored
// mid-run (observed 2026-10-16: the app restored two `shell-*` panes while
// the resize case was settling its own OMP panes) mounts extra xterms whose
// PTYs were never driven here, so they can never satisfy a bottom-pinned
// gate. Always keep the full set — `paneIds` is set per test after its panes
// mount, so it is null until then and every pane is in scope before it.
let ownedPaneIds = null
const pinnable = (state) =>
  Object.entries(state).filter(
    ([id]) =>
      id !== userScrolledPaneId && (!ownedPaneIds || ownedPaneIds.includes(id)),
  )

// Every mounted xterm pane's scroll position, keyed by data-pane-id.
async function paneScrollState() {
  return browser.execute(() => {
    const out = {}
    for (const mount of document.querySelectorAll('.xterm-mount')) {
      const paneId = mount.getAttribute('data-pane-id') || mount.id
      const viewport = mount.querySelector('.xterm-viewport')
      if (!viewport) continue
      out[paneId] = {
        scrollTop: viewport.scrollTop,
        max: viewport.scrollHeight - viewport.clientHeight,
        atBottom:
          viewport.scrollTop >= viewport.scrollHeight - viewport.clientHeight - 1,
      }
    }
    return out
  })
}

async function paneCount() {
  return browser.execute(
    () => document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]').length,
  )
}

// Sample every pane's DOM scrollTop once per animation frame, in the page.
// The regression the spec guards against is not the FINAL position (already
// asserted) but the transition: a pane that jumps to the top of scrollback and
// then visibly scrolls all the way back down. That shows up as a sample near
// the TOP of the buffer, so we record the worst (smallest relative) position
// per pane during the window and let the caller assert no such frame existed.
//
// Relative position (scrollTop / max) is the metric, not an absolute distance:
// PTY output appended mid-window grows `max`, and a bottom-pinned pane follows
// along, so an absolute distance-from-bottom spikes for a perfectly healthy
// pane whenever new rows arrive. A genuine jump-to-top reports relative ~0 at
// any buffer depth.
//
// DOM scrollTop is the right signal even under the WebGL renderer: xterm keeps
// .xterm-viewport.scrollHeight in sync with the buffer, so a pane parked at the
// top of a deep buffer reports scrollTop ~0 while `scrollHeight - clientHeight`
// stays large.
async function startScrollSampler(label) {
  // Mirror the test's pane scope into the page: a workspace restored mid-run
  // mounts xterms that were never driven here, and sampling them would let an
  // undriven pane's position fail the transition assertion.
  await browser.execute((ids) => {
    window.__athenaSamplerPaneIds = ids
  }, ownedPaneIds)
  return browser.execute((label) => {
    window.__scrollSamples = window.__scrollSamples || {}
    const state = (window.__scrollSamples[label] = { samples: [], worst: {}, running: true })
    const owned = window.__athenaSamplerPaneIds || null
    const tick = () => {
      if (!state.running) return
      for (const mount of document.querySelectorAll('.xterm-mount')) {
        const paneId = mount.getAttribute('data-pane-id') || mount.id
        if (owned && !owned.includes(paneId)) continue
        const viewport = mount.querySelector('.xterm-viewport')
        if (!viewport) continue
        const max = viewport.scrollHeight - viewport.clientHeight
        const scrollTop = viewport.scrollTop
        const distance = max - scrollTop
        // Relative position, growth-independent: a pane pinned at the bottom
        // of a growing buffer follows along and stays near 1.0 no matter how
        // many rows the PTY appends mid-window, whereas a jump-to-top reads
        // ~0.0. `max` alone is a trap — new output inflates it, so an absolute
        // distance spikes on any genuinely bottom-pinned pane.
        const relative = max > 0 ? scrollTop / max : 1
        if (!state.worst[paneId] || relative < state.worst[paneId].relative) {
          state.worst[paneId] = { distance, max, scrollTop, relative }
        }
      }
      requestAnimationFrame(tick)
    }
    requestAnimationFrame(tick)
    return true
  }, label)
}

async function stopScrollSampler(label) {
  const result = await browser.execute((label) => {
    const state = window.__scrollSamples?.[label]
    if (state) state.running = false
    return state ? { worst: state.worst } : null
  }, label)
  if (!result) throw new Error(`scroll sampler "${label}" was never started`)
  return result.worst
}

// How close to the TOP of its own scrollback a pane may legitimately get while
// a resize reprint is in flight. Relative, so a shallow pane and a deep one are
// judged on the same yardstick, and independent of how many rows the PTY
// appended mid-window. A remount-replay bug parks the pane at the very top
// (relative ~0.0), so anything above ~0.05 is tolerance for burst jitter and
// line-level follow lag.
const SCROLL_TOP_RATIO = 0.05

describe('Layout change viewport regression', () => {
  it('keeps existing panes pinned to bottom when a third shell is added', async function () {
    this.timeout(180000)

    // Authorize PTY spawn against the repo-root sandbox dir. Seeding the
    // persisted trusted-root list first makes the add below a no-op that
    // skips the backend's native confirmation dialog — a root that is not
    // already persisted hits `blocking_show()` with no window to click, and
    // the invoke never returns (workspace.rs:79-100).
    await seedTrustedRoots(browser, [repoRoot])
    await setE2E()
    await trustViaProjectRoot(browser)
    const trusted = await browser
      .waitUntil(
        async () => browser.execute(() => window.__athenaTrust === 'ok'),
        { timeout: 10000, interval: 250 },
      )
      .then(() => true)
      .catch(() => false)
    if (!trusted) {
      const flag = await browser.execute(() => String(window.__athenaTrust))
      throw new Error(`trusted root not authorized (flag=${flag}, dir=${repoRoot})`)
    }

    // A previous session's workspace restores with panes whose PTYs are dead
    // (verified: writing to a restored pane id changes neither
    // .xterm-viewport scrollHeight nor any other DOM signal), so the
    // assertions below — which need live OMP panes with deep scrollback —
    // cannot be met by reusing them. Close the restored workspace first so
    // the flow below creates its own live panes.
    if (await hasTerminal()) {
      const hasWorkspaceTab = await browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((b) =>
          (b.getAttribute('title') || '').includes('Close workspace'),
        ),
      )
      if (hasWorkspaceTab) {
        await browser.execute(() => {
          for (const button of document.querySelectorAll('button')) {
            if ((button.getAttribute('title') || '').includes('Close workspace')) {
              button.click()
              break
            }
          }
        })
        await browser.waitUntil(async () => !(await hasTerminal()), {
          timeout: 15000,
          interval: 250,
          timeoutMsg: 'Restored workspace did not close',
        })
      }
    }

    if (!(await hasTerminal())) {
      expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
      await browser.waitUntil(
        async () =>
          browser.execute(() =>
            Array.from(document.querySelectorAll('button')).some((button) =>
              (button.textContent || '').includes('Terminal Workspace'),
            ),
          ),
        { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
      )
      expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
      await browser.pause(250)
      await fillSpaceDir(browser, repoRoot)
      expect((await clickButtonByText('Next >')).ok).toBe(true)
      await browser.waitUntil(
        async () => browser.execute(() => !!document.getElementById('add-shell')),
        { timeout: 10000, interval: 250, timeoutMsg: 'Shell row did not appear' },
      )
      // Two shells.
      await browser.execute(() => {
        const btn = document.getElementById('add-shell')
        btn.click()
        btn.click()
      })
      expect((await clickButtonByText('Launch Space')).ok).toBe(true)
    } else {
      // Reaching here means a workspace survived the close above. Its panes
      // are the dead restored kind, which cannot satisfy this spec, so fail
      // loudly rather than asserting against a non-responding buffer.
      throw new Error('a workspace was still mounted before the run created its own')
    }

    await browser.waitUntil(
      async () => {
        const ready = await browser.execute(
          () =>
            Array.from(
              document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]'),
            ).filter((m) => !!m.querySelector('.xterm')).length,
        )
        return ready === 2
      },
      { timeout: 30000, interval: 500, timeoutMsg: 'Two shell panes did not mount xterm' },
    )
    await browser.pause(2000)

    // First: fill with plain scrollback to prove the baseline stays pinned.
    // Then start the real OMP TUI (the reported scenario) via the pty_write
    // IPC — the same path xterm onData uses — since synthetic WebDriver key
    // events are unreliable in WKWebView.
    const paneIds = await browser.execute(() =>
      Array.from(document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]')).map(
        (m) => m.getAttribute('data-pane-id') || m.id,
      ),
    )
    console.log('[layout-scroll] pane ids:', JSON.stringify(paneIds))
    // Scope gates below to the panes this test created (see `ownedPaneIds`).
    ownedPaneIds = paneIds
    // Baseline per pane. A successful `omp` launch lands in OMP's alt screen,
    // which discards scrollback, so the viewport can legitimately end up at
    // (or near) the visible screen height. Readiness therefore accepts either
    // "scrollback grew" or "alt screen" instead of demanding scrollback.
    const readPaneBoxes = () =>
      browser.execute(() =>
        Array.from(
          document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]'),
        )
          .filter((m) => m.querySelector('.xterm'))
          .map((m) => {
            const vp = m.querySelector('.xterm-viewport')
            return {
              id: m.getAttribute('data-pane-id') || m.id,
              scrollHeight: vp ? vp.scrollHeight : 0,
              clientHeight: vp ? vp.clientHeight : 0,
            }
          }),
      )
    const baseline = await readPaneBoxes()

    for (const [idx, id] of paneIds.entries()) {
      await browser.execute(
        ({ id, idx }) =>
          window.__TAURI__.core.invoke('pty_write', {
            id,
            data: `seq 1 300; echo FILL_DONE_${idx}; omp\r`,
          }),
        { id, idx },
      )
    }
    // Wait until both panes have painted PTY output. DOM-based on purpose:
    // panes restored from a previous session mount before this spec sets
    // `__athenaE2E`, so xterm_mount never registers them in
    // `__athenaTermMap` (that registration is gated on the flag,
    // xterm_mount.rs:1381-1388) and a map-based probe returns null. WebGL
    // keeps `.xterm-rows` empty too, so the only reliable signal is the
    // viewport: PTY output that painted rows makes it scrollable.
    const ompUp = await browser
      .waitUntil(
        async () => {
          const boxes = await readPaneBoxes()
          if (boxes.length < 2) return false
          return boxes.every((box) => {
            const start = baseline.find((s) => s.id === box.id)
            if (!start) return false
            // scrollback grew (shell output painted before the TUI took over),
            // or the viewport collapsed to the visible screen height, which is
            // OMP's alt screen: no scrollback exists there, so demanding
            // scrollHeight > clientHeight never becomes true.
            const grew = box.scrollHeight > start.scrollHeight
            const altScreen = box.scrollHeight > 0 && box.scrollHeight === box.clientHeight
            return grew || altScreen
          })
        },
        { timeout: 30000, interval: 500 },
      )
      .catch(() => false)
    if (!ompUp) {
      const diag = await browser.execute(() => {
        const details = Array.from(document.querySelectorAll('.xterm-mount')).map((m) => {
          const vp = m.querySelector('.xterm-viewport')
          return {
            id: m.getAttribute('data-pane-id') || m.id,
            domId: m.id,
            scrollHeight: vp ? vp.scrollHeight : null,
            clientHeight: vp ? vp.clientHeight : null,
            hasXterm: !!m.querySelector('.xterm'),
          }
        })
        return details
      })
      console.log('[layout-scroll] omp diagnostics:', JSON.stringify(diag, null, 2))
      throw new Error(
        `OMP TUI did not come up in both panes (baseline=${JSON.stringify(baseline)})`,
      )
    }
    // Let OMP settle (initial render flurry done) before measuring.
    await browser.pause(3000)

    // The user's repro uses panes with long conversations (via /resume).
    // Resume a session in each pane so the buffers have deep scrollback.
    // OMP's /resume picker is a heavy interactive redraw; firing it while the
    // TUI is still settling can wedge the webview (the plugin request fails
    // with a connection error), so settle fully first.
    await browser.pause(5000)
    for (const id of paneIds) {
      await browser.execute(
        (id) =>
          window.__TAURI__.core.invoke('pty_write', { id, data: '/resume\r' }),
        id,
      )
    }
    await browser.pause(1500)
    for (const id of paneIds) {
      await browser.execute(
        (id) => window.__TAURI__.core.invoke('pty_write', { id, data: '\r' }),
        id,
      )
    }
    // Wait for the resumed sessions to land back in the normal buffer: deep
    // (> 500 rows) AND pinned to the bottom. The alt-screen phase (picker
    // open, restored session loading) reports max === 0, so a depth probe run
    // during it sees nothing; measured transition for this repo was ~12 s from
    // `\r` to deep-and-pinned.
    const deep = await browser
      .waitUntil(
        async () => {
          const state = await paneScrollState()
          const vals = pinnable(state).map(([, s]) => s)
          if (vals.length < 2) return false
          if (vals.some((s) => s.max === 0)) return false
          return vals.every((s) => s.atBottom && s.max > 500)
        },
        { timeout: 40000, interval: 1000 },
      )
      .then(() => true)
      .catch(() => false)
    const afterResume = await paneScrollState()
    console.log('[layout-scroll] after resume:', JSON.stringify(afterResume))
    if (!deep) {
      throw new Error(
        `Resumed sessions never landed deep and pinned: ${JSON.stringify(afterResume)}`,
      )
    }
    // `/resume` restores whichever session the picker offers first, which is
    // the most recent one and is often short (measured: 289-323 rows). The
    // regression needs a deep buffer so a jump-to-top is visible at all, so
    // leave the TUI and re-establish depth with bounded output. This does not
    // weaken the spec: the assertions are about scrollback position, never
    // about OMP being in the foreground.
    // Build the SIGINT byte inside the page: an escape in the spec source has
    // to survive WDIO's JSON serialisation of the function argument, and
    // `\u0003` has been observed not to.
    for (const id of paneIds) {
      await browser.execute(
        (id) =>
          window.__TAURI__.core.invoke('pty_write', {
            id,
            data: String.fromCharCode(3),
          }),
        id,
      )
    }
    await browser.pause(1500)
    for (const [idx, id] of paneIds.entries()) {
      await browser.execute(
        ({ id, idx }) =>
          window.__TAURI__.core.invoke('pty_write', {
            id,
            data: `seq 1 1200; echo DEEP_${idx}\r`,
          }),
        { id, idx },
      )
    }
    const refilled = await browser
      .waitUntil(
        async () => {
          const state = await paneScrollState()
          const vals = pinnable(state).map(([, s]) => s)
          if (vals.length < 2) return false
          if (vals.some((s) => s.max === 0)) return false
          return vals.every((s) => s.atBottom && s.max > 500)
        },
        { timeout: 40000, interval: 1000 },
      )
      .then(() => true)
      .catch(() => false)
    const afterRefill = await paneScrollState()
    console.log('[layout-scroll] after refill:', JSON.stringify(afterRefill))
    if (!refilled) {
      throw new Error(
        `Refill did not restore deep bottom-pinned buffers: ${JSON.stringify(afterRefill)}`,
      )
    }

    // Remount probes: tag each pane's container + xterm DOM node; if a pane
    // is remounted by the layout change, a fresh node loses the tag.
    await browser.execute(() => {
      for (const mount of document.querySelectorAll('.xterm-mount')) {
        mount.setAttribute('data-probe-m', '1')
        mount.querySelector('.xterm')?.setAttribute('data-probe-x', '1')
        mount.querySelector('.xterm-helper-textarea')?.setAttribute('data-probe-t', '1')
      }
    })

    const before = await paneScrollState()
    console.log('[layout-scroll] before add:', JSON.stringify(before))
    for (const [id, s] of Object.entries(before)) {
      if (ownedPaneIds && !ownedPaneIds.includes(id)) continue
      if (!s.atBottom) throw new Error(`pane ${id} was not at bottom before layout change`)
    }

    // Trigger the layout change: add a third shell via the toolbar button.
    const added = await browser.execute(() => {
      for (const button of document.querySelectorAll('button')) {
        if ((button.getAttribute('title') || '').includes('Add Shell')) {
          button.click()
          return true
        }
      }
      return false
    })
    expect(added).toBe(true)

    // Watch the transition, not just the endpoint: the bug this guards is a
    // pane that lands at the TOP of scrollback and then visibly scrolls all
    // the way back down (the reported "goes back to the top and keeps
    // scrolling" animation). Sample every frame of the add.
    await startScrollSampler('add-third-shell')

    await browser.waitUntil(async () => (await paneCount()) >= 3, {
      timeout: 15000,
      interval: 250,
      timeoutMsg: 'Third pane did not mount',
    })
    // Wait out the full settle: resize-fit → PTY resize → SIGWINCH → TUI
    // redraw burst (OMP reprints its conversation; buffers briefly toggle
    // normal↔alternate). The buffered DOM scroll position oscillates during
    // the burst; the contract is the FINAL state.
    await browser.pause(3600)

    // Did any pre-existing pane lose its tagged DOM (i.e. get remounted)?
    const probe = await browser.execute(
      (beforeIds) => {
        const report = {}
        for (const id of beforeIds) {
          const mount = document.querySelector(`.xterm-mount[data-pane-id="${id}"]`)
          report[id] = {
            exists: !!mount,
            mountTagged: !!mount?.getAttribute('data-probe-m'),
            xtermTagged: !!mount?.querySelector('.xterm[data-probe-x]'),
            textareaTagged: !!mount?.querySelector('.xterm-helper-textarea[data-probe-t]'),
          }
        }
        return report
      },
      Object.keys(before),
    )
    for (const [id, p] of Object.entries(probe)) {
      // The layout change must reuse each pane's xterm in place — a remount
      // loses per-mount state and forces a full buffer replay.
      if (!p.exists || !p.mountTagged || !p.xtermTagged || !p.textareaTagged) {
        throw new Error(`pane ${id} remounted on layout change: ${JSON.stringify(p)}`)
      }
    }

    const after = await paneScrollState()
    console.log('[layout-scroll] after add:', JSON.stringify(after))
    await browser.saveScreenshot(join(screenshotDir, 'layout-scroll-regression.png'))

    // No pre-existing pane may ever show a jump-to-top frame during the add.
    // Tolerance is a few rows of reprint jitter; the remount-replay bug parks a
    // deep pane at the very top (distance ≈ full scrollback depth).
    const beforeIds = Object.keys(before)
    const addWorst = await stopScrollSampler('add-third-shell')
    console.log('[layout-scroll] add transition worst:', JSON.stringify(addWorst))
    const jumpers = beforeIds
      .filter((id) => addWorst[id])
      .filter((id) => addWorst[id].relative < SCROLL_TOP_RATIO)
      .map((id) => `${id} got ${addWorst[id].distance} rows from bottom (of ${addWorst[id].max})`)
    expect(jumpers).toEqual([])

    const offenders = after && Object.entries(after)
      .filter(([id]) => beforeIds.includes(id))
      .filter(([, s]) => !s.atBottom)
      .map(([id, s]) => `${id} scrollTop=${s.scrollTop}/${s.max}`)

    expect(offenders).toEqual([])

    // Counter-case: a pane the USER scrolled up (reading scrollback) must NOT
    // be forcibly re-pinned by the next layout change; its distance from the
    // bottom should be roughly preserved. A pane still at bottom must stay
    // pinned.
    const [p0] = Object.keys(before)
    // Test 2 must not demand bottom-pinning for this pane: it is the
    // user-scrolled counter-case and is correct to stay where the user left it.
    userScrolledPaneId = p0
    // Scroll the pane up by its viewport via the DOM, not __athenaTermMap:
    // panes restored from a previous session mount before the e2e flag is set,
    // so the map is null and a scrollToLine() call on it is a silent no-op that
    // leaves the pane at the bottom (xterm_mount.rs:1381-1388 gates the map).
    await browser.execute((id) => {
      for (const mount of document.querySelectorAll('.xterm-mount')) {
        if ((mount.getAttribute('data-pane-id') || mount.id) !== id) continue
        const vp = mount.querySelector('.xterm-viewport')
        if (vp) vp.scrollTop = vp.scrollTop - vp.clientHeight * 5
      }
    }, p0)
    await browser.pause(3400) // let the prior add's follow window (3.2s) expire
    const distanceBefore = await browser.execute((id) => {
      for (const mount of document.querySelectorAll('.xterm-mount')) {
        if ((mount.getAttribute('data-pane-id') || mount.id) !== id) continue
        const vp = mount.querySelector('.xterm-viewport')
        if (!vp) return null
        return vp.scrollHeight - vp.clientHeight - vp.scrollTop
      }
      return null
    }, p0)

    const added4 = await browser.execute(() => {
      for (const button of document.querySelectorAll('button')) {
        if ((button.getAttribute('title') || '').includes('Add Shell')) {
          button.click()
          return true
        }
      }
      return false
    })
    expect(added4).toBe(true)
    await browser.waitUntil(async () => (await paneCount()) >= 4, {
      timeout: 15000,
      interval: 250,
      timeoutMsg: 'Fourth pane did not mount',
    })
    await browser.pause(3500)

    const distanceAfter = await browser.execute((id) => {
      for (const mount of document.querySelectorAll('.xterm-mount')) {
        if ((mount.getAttribute('data-pane-id') || mount.id) !== id) continue
        const vp = mount.querySelector('.xterm-viewport')
        if (!vp) return null
        return vp.scrollHeight - vp.clientHeight - vp.scrollTop
      }
      return null
    }, p0)
    const finalState = await paneScrollState()
    console.log('[layout-scroll] user-scrolled pane distance:', {
      before: distanceBefore,
      after: distanceAfter,
      final: JSON.stringify(finalState),
    })
    expect(distanceAfter).not.toBe(null)
    // Scrolled-up pane must not be re-pinned to the bottom.
    if (distanceBefore && distanceBefore > 20) {
      expect(distanceAfter).toBeGreaterThan(20)
    }
  })

  // Second reported repro: no pane is added, the APP WINDOW is resized. The
  // layout reflow changes every pane's row count, so each one runs the same
  // fit → PTY resize → SIGWINCH sequence. Before the fix, panes replaced their
  // scrollback from the top and visibly scrolled down; the same frame-sampler
  // must now see no jump-to-top frame.
  it('keeps panes pinned to bottom across an app-window resize', async function () {
    this.timeout(240000)

    await browser.execute(() => {
      window.__athenaTrust = 'ok'
    })

    // Two OMP panes with deep scrollback, via the same launch path as the
    // case above (the assertion is about the resize transition, so this setup
    // is deliberately reused rather than re-invented). If the previous test
    // already left two panes mounted, reuse them.
    if (!(await hasTerminal())) {
      expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
      await browser.waitUntil(
        async () =>
          browser.execute(() =>
            Array.from(document.querySelectorAll('button')).some((button) =>
              (button.textContent || '').includes('Terminal Workspace'),
            ),
          ),
        { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
      )
      expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
      await browser.pause(250)
      await fillSpaceDir(browser, repoRoot)
      expect((await clickButtonByText('Next >')).ok).toBe(true)
      await browser.waitUntil(
        async () => browser.execute(() => !!document.getElementById('add-shell')),
        { timeout: 10000, interval: 250, timeoutMsg: 'Shell row did not appear' },
      )
      await browser.execute(() => {
        const btn = document.getElementById('add-shell')
        btn.click()
        btn.click()
      })
      expect((await clickButtonByText('Launch Space')).ok).toBe(true)
    }

    // No blanket re-pin here: the previous test's counter-case deliberately
    // left one pane scrolled up, and re-pinning it would assert the very bug
    // this spec exists to prevent. That pane is excluded from the gates below.

    await browser.waitUntil(
      async () => {
        const ready = await browser.execute(
          () =>
            Array.from(
              document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]'),
            ).filter((m) => !!m.querySelector('.xterm')).length,
        )
        return ready >= 2
      },
      { timeout: 30000, interval: 500, timeoutMsg: 'Two shell panes did not mount xterm' },
    )
    await browser.pause(2000)

    const paneIds = await browser.execute(() =>
      Array.from(document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]')).map(
        (m) => m.getAttribute('data-pane-id') || m.id,
      ),
    )
    // Scope every gate below to the panes this test just created. A workspace
    // the app restores mid-run mounts extra xterms that were never driven
    // here, and they can never satisfy a "pinned to bottom" assertion.
    ownedPaneIds = paneIds
    for (const [idx, id] of paneIds.entries()) {
      await browser.execute(
        ({ id, idx }) =>
          window.__TAURI__.core.invoke('pty_write', {
            id,
            data: `seq 1 300; echo FILL_DONE_${idx}; omp\r`,
          }),
        { id, idx },
      )
    }
    await browser.pause(4000)
    for (const id of paneIds) {
      await browser.execute(
        (id) => window.__TAURI__.core.invoke('pty_write', { id, data: '/resume\r' }),
        id,
      )
    }
    await browser.pause(1500)
    for (const id of paneIds) {
      await browser.execute((id) => {
        window.__TAURI__.core.invoke('pty_write', { id, data: '\r' })
      }, id)
    }
    // Panes must be pinned and deep before the resize is measured.
    //
    // This cannot be demanded at the instant `\r` lands: OMP's `/resume`
    // picker runs in the alt screen, where the viewport has no scrollback
    // (`max === 0`) for several seconds while the session is being restored.
    // Measured behaviour across the transition (this repo, 2026-10-16):
    //
    //   before resume      max=12767  scrollTop=12767  (pinned, deep)
    //   omp launched       max=0      scrollTop=0      (alt screen)
    //   /resume entered    max=0      scrollTop=0      (alt screen, picker)
    //   session restored   max=15011  scrollTop=15011  (pinned, deep, growing)
    //
    // So accept the alt-screen phase as "in flight" and require the landing
    // state: back in the normal buffer, pinned to the bottom, deep.
    const landedDeep = await browser
      .waitUntil(
        async () => {
          const state = await paneScrollState()
          const vals = pinnable(state).map(([, s]) => s)
          if (vals.length < 2) return false
          // Alt screen (or still loading): in flight, keep waiting.
          if (vals.some((s) => s.max === 0)) return false
          return vals.every((s) => s.atBottom && s.max > 500)
        },
        { timeout: 40000, interval: 1000, timeoutMsg: 'Panes never pinned with deep scrollback' },
      )
      .then(() => true)
      .catch(() => false)
    if (!landedDeep) {
      const state = await paneScrollState()
      throw new Error(`Panes never pinned with deep scrollback: ${JSON.stringify(state)}`)
    }

    const beforeResize = await paneScrollState()
    console.log('[layout-scroll-resize] before resize:', JSON.stringify(beforeResize))

    // Sample every frame of the resize.
    await startScrollSampler('window-resize')

    // Make the window narrower and shorter; WKWebView fires ResizeObserver for
    // every pane, which is the exact reflow the bug rides on.
    const size = await browser.getWindowSize()
    const restoreSize = async () => {
      await browser.setWindowSize(size.width, size.height)
      await browser.pause(500)
    }
    try {
      await browser.setWindowSize(
        Math.max(320, size.width - 320),
        Math.max(360, size.height - 200),
      )
      await browser.pause(4000)
      const worst = await stopScrollSampler('window-resize')

      const afterResize = await paneScrollState()
      console.log('[layout-scroll-resize] after resize:', JSON.stringify(afterResize))
      console.log('[layout-scroll-resize] transition worst:', JSON.stringify(worst))
      await browser.saveScreenshot(join(screenshotDir, 'layout-scroll-resize-regression.png'))

      const jumpers = Object.keys(beforeResize)
        .filter((id) => !ownedPaneIds || ownedPaneIds.includes(id))
        .filter((id) => worst[id])
        .filter((id) => worst[id].relative < SCROLL_TOP_RATIO)
        .map((id) => `${id} reached relative ${worst[id].relative.toFixed(3)} (of ${worst[id].max})`)
      expect(jumpers).toEqual([])

      // Every pane this case drove may be required to be bottom-pinned here:
      // it drives /resume + a deep refill into all of them, which legitimately
      // lands every buffer at the bottom — including the pane the previous
      // test scrolled up by hand (its user-scroll preservation is asserted by
      // that test, where no new content is written into it). Panes this test
      // never drove (a workspace restored mid-run) can never pin, so they are
      // out of scope: `ownedPaneIds`.
      for (const [id, s] of Object.entries(afterResize)) {
        if (ownedPaneIds && !ownedPaneIds.includes(id)) continue
        expect(s.atBottom).toBe(true)
      }
    } finally {
      // Restore the size even when an assertion above fails: the runner is
      // serial and specs glob alphabetically, so a leaked narrow window would
      // silently cripple every spec after this one.
      await restoreSize()
    }
  })
})
