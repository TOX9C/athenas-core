// Ported from e2e-tests/test_broadcast_direct.mjs (deleted 2026-08-24).
// Regression test for the Tauri `pty:raw` broadcast race bug: when 3+
// concurrent PTY read loops emitted a shared `&serde_json::Value` borrow,
// all listeners received payloads whose `sessionId` matched whichever task
// serialized last — only one pane updated. The fix serializes to an owned
// String before emit.
//
// Contract under defense: with 3 concurrent PTY sessions, frontend listeners
// on the session-scoped `pty:raw:<id>` channels receive payloads carrying
// >= 3 DISTINCT sessionIds.

const SESSION_IDS = ['e2e-alpha', 'e2e-beta', 'e2e-gamma']

describe('pty:raw broadcast race regression', () => {
  it('delivers distinct sessionIds to frontend listeners with 3 concurrent PTYs', async () => {
    // Wait for Tauri IPC surface.
    await browser.waitUntil(
      async () =>
        await browser.execute(
          () =>
            typeof window.__TAURI__ !== 'undefined' &&
            typeof window.__TAURI__.core?.invoke === 'function' &&
            typeof window.__TAURI__.event?.listen === 'function',
        ),
      { timeout: 25000, timeoutMsg: '__TAURI__ APIs never appeared (25s)' },
    )

    // Register one listener per session channel. The backend emits ONLY
    // session-scoped `pty:raw:<id>` events, so a bare 'pty:raw' subscription
    // would never fire. All listeners share one payload array.
    const reg = await browser.execute((ids) => {
      window.__ptyPayloads = []
      window.__ptyErrors = []
      for (const id of ids) {
        window.__TAURI__.event.listen(`pty:raw:${id}`, (event) => {
          try {
            const p = event.payload
            if (typeof p === 'string') {
              try {
                window.__ptyPayloads.push(JSON.parse(p))
              } catch {
                window.__ptyPayloads.push({ raw: p })
              }
            } else if (p && typeof p === 'object') {
              window.__ptyPayloads.push(p)
            } else {
              window.__ptyPayloads.push({ unexpected: typeof p })
            }
          } catch (e) {
            window.__ptyErrors.push('callback err: ' + String(e))
          }
        })
      }
      return 'registered'
    }, SESSION_IDS)
    expect(reg).toBe('registered')

    // Spawn 3 PTY sessions in the repo dir (the cwd sandbox rejects paths
    // outside the workspace). Fire-and-forget with error routing: a
    // rejected invoke must not reject execute() and abort the spec.
    for (const id of SESSION_IDS) {
      await browser.execute((sessionId) => {
        window.__TAURI__.core
          .invoke('pty_spawn', {
            id: sessionId,
            cwd: '/Users/apollo/Documents/athenas-core',
            shell: '/bin/zsh',
            cols: 80,
            rows: 24,
          })
          .catch((e) => window.__ptyErrors.push(`spawn ${sessionId}: ${String(e)}`))
      }, id)
    }

    // Collect payloads for a window.
    await browser.pause(6000)

    const collected = await browser.execute(() => {
      const payloads = window.__ptyPayloads || []
      const sessionIds = payloads.map((p) => p && p.sessionId).filter((s) => s !== undefined)
      const perIdCount = {}
      for (const s of sessionIds) perIdCount[s] = (perIdCount[s] || 0) + 1
      return {
        totalPayloads: payloads.length,
        uniqueSessionIds: [...new Set(sessionIds)],
        perIdCount,
        errors: window.__ptyErrors || [],
      }
    })

    expect(collected.errors, `spawn/listener errors: ${JSON.stringify(collected.errors)}`).toEqual(
      [],
    )
    expect(collected.totalPayloads).toBeGreaterThan(0)
    expect(
      collected.uniqueSessionIds,
      `expected >= 3 distinct sessionIds, got ${JSON.stringify(collected.perIdCount)}`,
    ).toEqual(expect.arrayContaining(SESSION_IDS))
  })

  after(async () => {
    // Kill spawned sessions so the next spec starts from a clean app state.
    for (const id of SESSION_IDS) {
      await browser.execute((sessionId) => {
        window.__TAURI__?.core?.invoke('pty_kill', { id: sessionId })?.catch?.(() => {})
      }, id)
    }
  })
})
