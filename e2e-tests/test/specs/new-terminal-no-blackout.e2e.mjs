// Regression guard for the "new-terminal / sidebar-open blackout" class
// (plan doc .plans/2026-10-11-new-terminal-blackout.md, Task 5):
//
//  1. Scenario A — Add Shell on an existing multi-pane space must not black
//     out any pane for longer than a single layout settle. Every pane's
//     renderer canvas must stay present and sized past 400 ms after the
//     click; the window is sampled at 100 ms granularity for 3 s. A pane
//     whose canvas is missing or zero-sized is "blank" — freeze_screen
//     blacks the pane via opacity but keeps the canvas node; a starved
//     mount never gets a sized canvas at all.
//  2. A routine Add Shell must NOT reset the shared glyph atlas
//     (window.__athenaAtlasResets delta 0 across the click — Task 3's
//     reset_glyph_atlas_for_mount policy). The counter itself is proven
//     alive via the real reset path (window.__athenaResetAtlas), which must
//     bump it — guarding against the spec passing because the probe is dead.
//  3. Scenario B — sidebar toggle on the same space must not black out any
//     pane either, and must not fire atlas resets (no visibility/font/DPR
//     change should reset the atlas).
//
// Sampling uses the suite's proven start/stop window-parking pattern
// (layout-scroll-regression.e2e.mjs): browser.execute does NOT await a
// Promise returned by the page function, so the loop accumulates samples on
// window and a second execute call stops it and returns the vector.
//
// Run:  tauri-wd &  &&  npx wdio run wdio.conf.mjs --spec test/specs/new-terminal-no-blackout.e2e.mjs

import { e2eTrustRoot, seedTrustedRoots, setE2E, fillSpaceDir, trustViaProjectRoot } from '../helpers/trust.mjs'

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

/// Start a 100 ms liveness sampler under window.__athenaBlackoutSamples[label].
/// Each sample: { t (ms since start), paneCount, blank } where blank counts
/// panes whose renderer canvas is missing or zero-sized.
async function startLivenessSampler(label) {
  return browser.execute((label) => {
    const state = { t0: performance.now(), samples: [], running: true }
    window.__athenaBlackoutSamples = window.__athenaBlackoutSamples || {}
    window.__athenaBlackoutSamples[label] = state
    const tick = () => {
      if (!state.running) return
      const panes = [...document.querySelectorAll('.xterm-mount[data-pane-id]')]
      const blank = panes.filter((p) => {
        const c = p.querySelector('.xterm-screen canvas')
        return !c || c.width === 0
      }).length
      state.samples.push({ t: performance.now() - state.t0, paneCount: panes.length, blank })
      setTimeout(tick, 100)
    }
    tick()
    return true
  }, label)
}

/// Stop the sampler and return its sample vector.
async function stopLivenessSampler(label) {
  const out = await browser.execute((label) => {
    const state = window.__athenaBlackoutSamples?.[label]
    if (state) state.running = false
    return state ? state.samples : null
  }, label)
  if (!out) throw new Error(`liveness sampler "${label}" was never started`)
  return out
}

async function paneIds() {
  return browser.execute(`return (${PANE_WRAP_JS}).map((p) => p.getAttribute('data-pane-id'))`)
}

async function atlasResets() {
  return browser.execute(() => window.__athenaAtlasResets ?? 0)
}

async function clickAddShell() {
  await browser.execute(() => {
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.getAttribute('title') || '').startsWith('Add Shell'))
      ?.click()
  })
}

async function ensureSpaceWithShell() {
  await setE2E()
  // A permitted dir (the backend rejects /tmp); seeded so the add is a no-op
  // that skips the blocking confirmation dialog.
  const trustDir = e2eTrustRoot('new-terminal-no-blackout')
  await seedTrustedRoots(browser, [trustDir])
  await trustViaProjectRoot(browser)
  await browser.pause(1500)
  const state = await browser.execute(`return ${APP_STATE_JS}`)
  console.log('[blackout] app state:', JSON.stringify(state))

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
  await fillSpaceDir(browser, trustDir)
  // Advance Step 1 -> Step 2: filling the field does NOT auto-advance, and
  // the agent rows (incl. `add-shell`) only render on step 2.
  await browser.execute(() => {
    Array.from(document.querySelectorAll('button'))
      .find((b) => (b.textContent || '').trim() === 'Next >')
      ?.click()
  })
  await browser.waitUntil(() => browser.execute(() => !!document.getElementById('add-shell')), {
    timeout: 10000,
    interval: 250,
    timeoutMsg: 'Terminal configuration step did not appear',
  })
  // One pane minimum: the modal refuses Launch while total panes is 0.
  await browser.execute(() => {
    document.getElementById('add-shell')?.click()
  })
  await browser.pause(400)
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

/// Bring the active space to exactly `n` live panes via Add Shell clicks,
/// waiting for each pane's renderer canvas between clicks.
async function growToPanes(n) {
  let current = await browser.execute(`return (${PANE_WRAP_JS}).length`)
  while (current < n) {
    await clickAddShell()
    await browser.waitUntil(
      () => browser.execute(`return (${PANE_WRAP_JS}).length >= ${current + 1}`),
      { timeout: 20000, interval: 250, timeoutMsg: 'Add Shell did not add a pane' },
    )
    // Cold pane: give the mount handshake (layout commit -> font settle ->
    // atlas join -> fit -> PTY resize -> release) room to complete.
    await browser.pause(4000)
    current = await browser.execute(`return (${PANE_WRAP_JS}).length`)
  }
}

/// Sample the 3 s window around a trigger: start the sampler, run the
/// `trigger` thunk (the click), hold for `ms`, stop, return the samples.
async function sampleAround(label, ms, trigger) {
  await startLivenessSampler(label)
  await trigger()
  await browser.pause(ms)
  return stopLivenessSampler(label)
}

describe('new terminal / sidebar blackout', function () {
  it('keeps every pane canvas alive through an Add Shell (no blackout, no atlas reset)', async function () {
    this.timeout(240000)

    await ensureSpaceWithShell()

    // Grow to a 4-pane space one click at a time so every pane is settled
    // before the trigger under test.
    await growToPanes(4)
    const ids = await paneIds()
    expect(ids.length).toBe(4)

    // Echo content into each pane so the blackout is observable as content
    // loss, not just geometry.
    for (const id of ids) {
      await browser.execute(
        (pid) => window.__TAURI__.core.invoke('pty_write', { id: pid, data: 'seq 1 12 | nl -ba\n' }),
        id,
      )
    }
    await browser.pause(2000)

    // Baseline atlas counter immediately before the trigger.
    const resetsBefore = await atlasResets()

    // Trigger: Add Shell, sampled for 3 s from the same tick.
    const samples = await sampleAround('add-shell', 3000, clickAddShell)
    const resetsAfter = await atlasResets()
    const blanks = samples.filter((s) => s.t > 400 && s.blank > 0)
    console.log(
      '[blackout] add-shell: resets',
      resetsBefore,
      '→',
      resetsAfter,
      'blank samples past 400 ms:',
      JSON.stringify(blanks),
    )

    // A fifth pane appeared.
    expect(samples[samples.length - 1].paneCount).toBe(5)
    // After 400 ms of layout settle, no pane may be canvas-less/zero-sized.
    expect(blanks).toHaveLength(0)
    // Routine Add Shell must not reset the shared glyph atlas (Task 3).
    expect(resetsAfter - resetsBefore).toBe(0)
  })

  it('bumps the atlas counter via the real reset path (probe sanity)', async function () {
    this.timeout(120000)

    await ensureSpaceWithShell()
    const ids = await paneIds()
    expect(ids.length).toBeGreaterThan(0)

    // The counter must actually be wired: driving the REAL reset path
    // (window.__athenaResetAtlas → reset_glyph_atlas) must bump it. Without
    // this check, a dead probe would make the delta-0 assertions above
    // vacuously true.
    const resetsBefore = await atlasResets()
    await browser.execute(`window.__athenaResetAtlas(${JSON.stringify(ids[0])})`)
    await browser.pause(1000)
    const resetsAfter = await atlasResets()
    console.log('[blackout] probe sanity: resets', resetsBefore, '→', resetsAfter)
    expect(resetsAfter - resetsBefore).toBeGreaterThanOrEqual(1)
  })

  it('keeps every pane canvas alive through a sidebar toggle (no blackout, no atlas reset)', async function () {
    this.timeout(240000)

    await ensureSpaceWithShell()
    await growToPanes(4)

    const resetsBefore = await atlasResets()
    const samples = await sampleAround('sidebar', 3000, () =>
      browser.execute(() => {
        document.querySelector('[data-athena-toggle]')?.click()
      }),
    )
    const resetsAfter = await atlasResets()
    const blanks = samples.filter((s) => s.t > 400 && s.blank > 0)
    console.log(
      '[blackout] sidebar: resets',
      resetsBefore,
      '→',
      resetsAfter,
      'blank samples past 400 ms:',
      JSON.stringify(blanks),
    )

    expect(blanks).toHaveLength(0)
    expect(resetsAfter - resetsBefore).toBe(0)
  })
})
