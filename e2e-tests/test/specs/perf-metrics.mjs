/**
 * Performance-metrics verification spec.
 *
 * Verifies the frontend render/IPC instrumentation exposed via
 * `window.__athenaMetrics.snapshot()`:
 *
 *   1. The snapshot is installed and returns renders/ipc/events/eventBytes.
 *   2. With a multi-pane terminal workspace streaming output, per-pane
 *      renders (`PaneItem`) clearly outnumber root-shell renders (`App`).
 *   3. IPC traffic and push-event bytes are observed while streaming.
 *
 * The last point is the regression gate for the reactive-isolation work:
 * hot per-pane state lives in `TerminalRegistry` signals, so a root-shell
 * component must NOT re-render per terminal-data event. `App` renders only
 * when its own low-frequency state (workspace membership, panels, modals)
 * changes, while each `PaneItem` re-renders on its own pane's signal.
 *
 * CSP note: the app forbids `unsafe-eval`, so this spec never uses
 * `new Function(...)` inside `browser.execute` — input values are set via the
 * native prototype setter inline, and pane output is streamed by writing a
 * shell loop to the pane PTY with `window.__TAURI__.core.invoke('pty_write')`.
 *
 * Run with: cd e2e-tests && npm run test:metrics
 * Requires a debug binary built with the metrics instrumentation.
 */
import { expect } from '@wdio/globals'

async function metricsSnapshot() {
  const raw = await browser.execute(() => {
    const metrics = window.__athenaMetrics
    return metrics && typeof metrics.snapshot === 'string' ? metrics.snapshot : null
  })
  return raw ? JSON.parse(raw) : null
}

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

/**
 * Set an input's value via the native prototype setter (CSP-safe: no eval)
 * and dispatch an `input` event so the Dioxus/WASM handler observes it.
 */
async function setInputValue(selector, value) {
  return browser.execute(
    ({ selector, value }) => {
      const el = document.querySelector(selector)
      if (!el) return false
      const proto = Object.getPrototypeOf(el)
      const setter = Object.getOwnPropertyDescriptor(proto, 'value').set
      setter.call(el, value)
      el.dispatchEvent(new Event('input', { bubbles: true }))
      return true
    },
    { selector, value },
  )
}

/**
 * Stream `paneId` output by writing a shell loop through the PTY. Retries
 * until the backend accepts the write (the PTY session is spawned
 * asynchronously after mount), mirroring the agent-badges spec.
 */
async function streamPaneOutput(paneId) {
  await browser.waitUntil(
    async () => {
      await browser.execute(
        ({ paneId }) => {
          window.__athenaWrite = 'pending'
          window.__TAURI__.core
            .invoke('pty_write', {
              id: paneId,
              data: "bash -c \"for i in $(seq 1 200); do echo metrics-line-$i; sleep 0.05; done\"\n",
            })
            .then(() => {
              window.__athenaWrite = 'ok'
            })
            .catch((e) => {
              window.__athenaWrite = 'ERR:' + String(e)
            })
        },
        { paneId },
      )
      await browser.pause(600)
      const state = await browser.execute(() => ({ write: window.__athenaWrite }))
      return state.write === 'ok'
    },
    {
      timeout: 20_000,
      interval: 700,
      timeoutMsg: `pty_write never accepted for ${paneId} (session never ready)`,
    },
  )
}

async function mountedPaneIds() {
  return browser.execute(() =>
    Array.from(document.querySelectorAll('.xterm-mount')).map((mount) => mount.id),
  )
}

describe('perf metrics instrumentation', function () {
  this.timeout(180_000)

  it('exposes a snapshot with counters', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })
    await browser.pause(2_500)

    const snap = await metricsSnapshot()
    expect(snap).not.toBeNull()
    expect(snap).toHaveProperty('renders')
    expect(snap).toHaveProperty('renderDurations')
    expect(snap).toHaveProperty('ipc')
    expect(snap).toHaveProperty('events')
    expect(snap).toHaveProperty('eventBytes')
  })

  it('keeps root-shell renders below per-pane renders under streaming load', async () => {
    // Open a new terminal workspace with several panes.
    await clickButtonByText('New Workspace', { partial: true })
    await browser.pause(800)
    await clickButtonByText('Terminal Workspace')
    await browser.pause(800)
    await setInputValue('input[placeholder="/path/to/project"]', '/tmp')
    await setInputValue('input[placeholder="my-project"]', 'metrics-space')
    await clickButtonByText('Next >')
    await browser.pause(800)

    // Add 3 shell panes and launch.
    for (let i = 0; i < 3; i += 1) {
      await browser.execute(() => {
        const btn = document.querySelector('#add-shell') || document.querySelector('button[title^="Add Shell"]')
        if (btn) btn.click()
      })
      await browser.pause(400)
    }
    await clickButtonByText('Launch Space', { partial: true })

    // Wait for 3 xterm panes to mount, then read the baseline snapshot.
    await browser.waitUntil(
      async () => (await mountedPaneIds()).length >= 3,
      { timeout: 20_000, interval: 500, timeoutMsg: 'Expected 3 mounted xterm panes' },
    )
    const before = await metricsSnapshot()
    const appBefore = before.renders.App || 0
    const paneBefore = before.renders.PaneItem || 0
    const eventBytesBefore = before.eventBytes || 0

    // Stream output into every pane — this is the hot path: each chunk drives
    // a pty:raw event and a per-pane re-render, and (crucially) must NOT
    // re-render the root App shell.
    const paneIds = await mountedPaneIds()
    for (const paneId of paneIds) {
      await streamPaneOutput(paneId)
    }
    // Let the streams run and the render counters accumulate.
    await browser.pause(6_000)

    const after = await metricsSnapshot()
    const appRenders = after.renders.App || 0
    const paneRenders = after.renders.PaneItem || 0
    const controllerRenders = after.renders.TerminalController || 0
    const appDelta = appRenders - appBefore
    const paneDelta = paneRenders - paneBefore

    // IPC traffic must have occurred while spawning and streaming.
    const ipcTotal = Object.values(after.ipc).reduce((a, b) => a + b, 0)
    expect(ipcTotal).toBeGreaterThan(0)
    // Push-event bytes must have grown from the streamed PTY output.
    expect(after.eventBytes).toBeGreaterThan(eventBytesBefore)

    // The regression gate: per-pane renders must clearly outnumber root-shell
    // renders under streaming load. With per-pane signal isolation, PaneItem
    // re-renders per terminal-data event while App only re-renders on
    // low-frequency state changes.
    //
    // Threshold tuned to a measured run: App +1 vs PaneItem +5 renders while
    // ~3.8 MB of PTY output streamed across 3 panes (≈5:1 ratio). The gate
    // requires at least a 2:1 ratio so a subtle root-shell subscription leak
    // (which would re-render App per event) fails loudly.
    expect(appDelta).toBeGreaterThanOrEqual(0)
    expect(paneDelta).toBeGreaterThan(appDelta * 2)

    // The controller (which owns terminal-store reads) must be mounted.
    expect(controllerRenders).toBeGreaterThan(0)

    // Print the evidence for the report / screenshot review.
    const summary = `metrics: App=${appRenders} (+${appDelta}) PaneItem=${paneRenders} (+${paneDelta}) TerminalController=${controllerRenders} ipc=${ipcTotal} eventBytes=${after.eventBytes}`
    // eslint-disable-next-line no-console
    console.log(`[perf-metrics] ${summary}`)
    await browser.execute((s) => {
      window.__lastPerfMetrics = s
    }, summary)
  })
})
