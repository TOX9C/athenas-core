// Regression spec: the first Browse in a process must not freeze the app.
//
// Root cause this pins (.plans/2026-10-22-browse-dialog-main-thread-freeze.md):
// the first NSOpenPanel init in a process blocks the main thread ~4.6 s on
// macOS 27 while launchd spawns the openAndSavePanelService XPC host —
// inside `-[NSSavePanel _initBridgeAndStuff]` under `HIRunLoopSemaphore`.
// With the `fs_warm_open_panel` fix, the New Space modal pays that cost at
// modal-open; the Browse click finds the service warm.
//
// Method: open the New Space modal (triggers the warm-up), wait for the
// handshake window to pass (6 s > measured cold 4.6 s), click Browse, then
// ping `window_platform` every 150 ms and fail on any RTT > 500 ms.
// Without the fix this measures a 3.3–5.6 s stall (diagnosed live, twice).
//
// The native panel cannot be dismissed by WebDriver; the spec measures only
// the stall and lets suite teardown kill the app. browser.execute does NOT
// await returned Promises (trust.mjs:29), so the async measurement parks on
// window and is polled — same bridging as the trust helper.

import assert from 'node:assert'
const KEY = '__browseWarmupMeasure'

describe('browse dialog warm-up', () => {
  it('first Browse after modal-open does not stall the main thread', async () => {
    // Open the New Space modal — this is the warm-up trigger under test.
    const clicked = await browser.execute(() => {
      const cand = [...document.querySelectorAll('button')]
        .find((b) => (b.textContent || '').includes('New Workspace'))
      if (cand) { cand.click(); return true }
      const tabs = document.querySelector('button[title="New Workspace (Cmd+T)"]')
      if (tabs) { tabs.click(); return true }
      return false
    })
    if (!clicked) throw new Error('New Workspace button not found')

    // Step 0: pick the Terminal Workspace card (lands on the Browse step).
    await browser.pause(400)
    await browser.execute(() => {
      const cards = [...document.querySelectorAll('button')]
        .filter((b) => (b.textContent || '').includes('Terminal Workspace'))
      if (cards.length) cards[0].click()
    })

    // Give the warm-up the full cold-handshake window. Modal open -> 6 s
    // covers the measured 4.6 s worst case with margin; too short and the
    // warm-up is still in flight when Browse lands (under the fix the
    // re-warm is ~0.15 s, so this wait is not masking a partial warm).
    await browser.pause(6000)

    // Arm: click Browse, then ping until a >500 ms RTT appears or 8 quiet
    // seconds pass (panel is open and main thread healthy -> pass).
    await browser.execute((k) => {
      window[k] = { status: 'armed' }
      const invoke = window.__TAURI__.core.invoke

      ;(async () => {
        const btn = document.querySelector('[data-athena-browse="directory"]')
        if (!btn) { window[k] = { status: 'err', error: 'browse button not found' }; return }
        btn.click()
        window[k].status = 'clicked'
        const start = performance.now()
        let worst = 0
        let stalled = false
        while (performance.now() - start < 8000) {
          const t = performance.now()
          // eslint-disable-next-line no-await-in-loop
          await invoke('window_platform')
          const rtt = performance.now() - t
          if (rtt > worst) worst = rtt
          if (rtt > 500) { stalled = true; break }
          // eslint-disable-next-line no-await-in-loop
          await new Promise((r) => setTimeout(r, 150))
        }
        window[k] = {
          status: stalled ? 'stalled' : 'healthy',
          worstRtt: Math.round(worst),
          measuredMs: Math.round(performance.now() - start),
        }
      })().catch((e) => { window[k] = { status: 'err', error: String(e) } })
    }, KEY)

    await browser.waitUntil(
      async () => {
        const s = await browser.execute((k) => window[k] && window[k].status !== 'armed' && window[k].status !== 'clicked', KEY)
        return s === true
      },
      { timeout: 15000, interval: 250, timeoutMsg: 'Browse stall measurement never finished' },
    )
    const result = await browser.execute((k) => window[k], KEY)

    console.log('=== BROWSE WARM-UP MEASUREMENT ===', JSON.stringify(result))
    assert.ok(
      result.status === 'healthy',
      `main thread stalled after Browse (worst RTT ${result.worstRtt} ms — panel-service warm-up not applied?)`,
    )
    assert.ok(result.worstRtt < 500, `worst RTT ${result.worstRtt} ms exceeds 500 ms`)
  })
})
