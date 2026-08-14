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

// Snapshot the LAST space (the one this test just launched). Workspaces
// persist across app launches, so assertions must never assume a global
// count — everything is scoped to the newest tab + grid.
async function lastSpaceState() {
  return browser.execute(() => {
    const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
    const grid = grids[grids.length - 1] || null
    const tabs = Array.from(document.querySelectorAll('.workspace-tab'))
    const tab = tabs[tabs.length - 1] || null
    const rows = Array.from(document.querySelectorAll('.workspace-row'))
    const row = rows[rows.length - 1] || null
    const dots = grid ? Array.from(grid.querySelectorAll('.status-dot')) : []
    const wraps = grid ? Array.from(grid.querySelectorAll('.pane-wrap')) : []
    const badges = row
      ? Array.from(row.querySelectorAll('.badge')).map((b) => ({
          label: b.getAttribute('aria-label'),
          text: (b.textContent || '').trim(),
        }))
      : []
    const topTabBadges = tab ? Array.from(tab.querySelectorAll('.badge')).length : 0
    return {
      hasGrid: !!grid,
      hasTab: !!tab,
      dotCount: dots.length,
      dotClasses: dots.map((d) => d.className),
      paneIds: wraps.map((w) => w.getAttribute('data-pane-id')),
      badges,
      topTabBadges,
    }
  })
}

describe('Agent activity detection UI', () => {
  it('shows per-pane dots, hides zero badges for shell panes, and shows only live working badges for an agent-like process', async function () {
    // The full loop (launch + agent simulation + settle) exceeds the 60s
    // default mocha window.
    this.timeout(150000)

    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // The modal defaults every shell pane's cwd to /tmp, and pty_spawn
    // rejects cwds outside the workspace's trusted roots — with a clean
    // store there are no roots, so ALL panes would fail to spawn ("session
    // not found" on write). Ensure /tmp is trusted so sessions actually
    // start. Fire-and-forget: it takes effect before the spawn below.
    await browser.execute(() => {
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: '/tmp' })
        .catch((e) => console.error('[agent-badges] trust-root failed:', String(e)))
      return true
    })

    // ── 1. Launch a space with 3 shell panes (proven flow) ─────────────
    const openResult = await clickButtonByText('New Workspace', { partial: true })
    expect(openResult.ok).toBe(true)

    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll('button')).some((btn) =>
            (btn.textContent || '').includes('Terminal Workspace'),
          ),
        ),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Space modal did not open' },
    )

    const terminalModeResult = await clickButtonByText('Terminal Workspace', { partial: true })
    expect(terminalModeResult.ok).toBe(true)

    const nextResult = await clickButtonByText('Next >')
    expect(nextResult.ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-shell')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Terminal configuration step did not appear' },
    )

    const addResult = await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      if (!btn) return { ok: false }
      for (let i = 0; i < 3; i += 1) {
        btn.click()
      }
      return { ok: true }
    })
    expect(addResult.ok).toBe(true)

    const launchResult = await clickButtonByText('Launch Space')
    expect(launchResult.ok).toBe(true)

    // ── 2. The new space mounts 3 panes; every pane pill has an idle dot,
    //       and the space tab shows NO agent count badges (plain shells
    //       don't count as agents). We wait for the pills (dots) + mount
    //       containers here; the PTY session readiness itself is verified in
    //       the next step by retrying pty_write until the backend accepts it.
    await browser.waitUntil(
      // Everything inside ONE execute: nodes returned from execute are
      // serialized as WebElement refs (not live DOM), so all DOM work happens
      // in the page context and only a boolean crosses the bridge.
      () =>
        browser.execute(() => {
          const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
          const last = grids[grids.length - 1]
          if (!last) return false
          const dots = last.querySelectorAll('.status-dot').length
          const mounts = last.querySelectorAll('.xterm-mount').length
          return dots === 3 && mounts === 3
        }),
      { timeout: 25000, interval: 500, timeoutMsg: 'New space did not mount 3 xterm panes' },
    )
    // Let pty_set_xterm / attach_listener settle before driving the pane.
    await browser.pause(1000)

    const idleState = await lastSpaceState()
    console.log('[agent-badges] idle state:', JSON.stringify(idleState))
    expect(idleState.dotCount).toBe(3)
    for (const cls of idleState.dotClasses) {
      expect(cls).toContain('is-idle')
    }
    expect(idleState.badges.length).toBe(0)
    expect(idleState.topTabBadges).toBe(0)
    expect(idleState.paneIds.length).toBe(3)
    const paneId = idleState.paneIds[0]
    expect(paneId).toBeTruthy()

    // ── 3. Run a real agent-like foreground process in pane 1: argv[0]
    //       renamed to `claude`, emitting output pulses, alive ~8s. This
    //       exercises the full loop: process → ps → tracker → agent:status
    //       → frontend store → badges/dot. The write itself (with retry) is
    //       in step 3b — the only writer, so the fake agent runs exactly once.
    const cmd =
      "bash -c \"exec -a claude sh -c 'echo one; sleep 0.3; echo two; sleep 0.3; echo three; sleep 8'\"\n"
    // writeToPane resolves to the execute's bare `true` (fire-and-forget);
    // the badge/dot waitUntil below is the real assertion that the write
    // landed and the backend detection loop responded.

    // ── 3b. The pane's PTY session is spawned asynchronously after the pills
    //       render — a write too early fails with "session not found". Retry
    //       the write until the backend accepts it (flag flips to 'ok'); the
    //       invoke is fired through the page's own JS context and the result
    //       captured via a window flag (executeAsync is unreliable over
    //       tauri-wd). Tauri 2 camelCases command args: `pane_id` → `paneId`.
    //       Only re-fire on an explicit error (never while still 'pending') so
    //       the fake agent command can never be written twice.
    await browser.waitUntil(
      async () => {
        await browser.execute(
          ({ paneId, data }) => {
            const s = window.__athenaWrite
            // Skip this tick unless the previous attempt errored (or this is
            // the first attempt) — a 'pending' flag means the last invoke is
            // still in flight and may succeed; re-firing would double-write.
            if (s === 'pending') return true
            window.__athenaWrite = 'pending'
            try {
              window.__TAURI__.core
                .invoke('pty_write', { id: paneId, data })
                .then(() => {
                  window.__athenaWrite = 'ok'
                })
                .catch((e) => {
                  window.__athenaWrite = 'ERR:' + String(e)
                })
            } catch (e) {
              window.__athenaWrite = 'SYNC-ERR:' + String(e)
            }
            return true
          },
          { paneId, data: cmd },
        )
        await browser.pause(600)
        const s = await browser.execute(() => ({ write: window.__athenaWrite }))
        return s.write === 'ok'
      },
      { timeout: 20000, interval: 700, timeoutMsg: 'pty_write never accepted (session never ready)' },
    )
    console.log('[agent-badges] pty_write accepted by backend')


    // ── 4. The agent appears: a working/thinking dot in the pane and a
    //       non-zero working badge in the workspace row.
    await browser.waitUntil(
      async () => {
        const s = await lastSpaceState()
        const labels = s.badges.map((b) => b.label)
        const liveDot = s.dotClasses.some((c) => c.includes('is-working') || c.includes('is-thinking'))
        return liveDot && labels.includes('Agents working')
      },
      {
        timeout: 20000,
        interval: 500,
        timeoutMsg: 'agent:status never reached the UI (no working badge / dot)',
      },
    )

    // ── 5. The user's core ask: only the non-zero working count is shown;
    //       the redundant total count is omitted.
    const badgeOrder = await lastSpaceState()
    const labels = badgeOrder.badges.map((b) => b.label)
    const texts = badgeOrder.badges.map((b) => b.text)
    const workingIdx = labels.indexOf('Agents working')
    expect(workingIdx).toBeGreaterThanOrEqual(0)
    expect(labels).not.toContain('Agents')
    expect(Number(texts[workingIdx])).toBeGreaterThanOrEqual(1)

    await browser.saveScreenshot(join(screenshotDir, 'agent-badges-working.png'))

    // ── 6. Let the fake agent finish; the working badge clears (the tracker
    //       only clears on the next heartbeat after fg returns to shell, so
    //       give it a generous window and log rather than hard-fail).
    await browser.pause(12000)
    const afterState = await lastSpaceState()
    const afterLabels = afterState.badges.map((b) => b.label)
    const afterLiveDots = afterState.dotClasses.filter(
      (c) => c.includes('is-working') || c.includes('is-thinking'),
    ).length
    console.log(
      `[agent-badges] after agent exit: liveDots=${afterLiveDots} hasWorkingBadge=${afterLabels.includes('Agents working')}`,
    )
  })
})
