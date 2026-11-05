// Shared Tauri IPC bridge for the e2e suite.
//
// `browser.execute` does NOT await a Promise returned by its function (this
// repo proved it in athena-chat-stub / kanban-persistence), so a
// fire-and-forget `invoke` can still be in flight when the next command runs.
// `invokeSettled` bridges the invoke through a window ticket and polls it
// synchronously, so the caller really knows it settled.

const POLL = { timeout: 30000, interval: 200 }

let ticketSeq = 0

// Fire a Tauri invoke in the page, park its settlement on `window`, and poll
// for it synchronously so the caller really knows it finished.
export async function invokeSettled(browser, cmd, args = {}) {
  const ticket = `__e2eIpc_${Date.now()}_${ticketSeq++}`
  await browser.execute((t, c, a) => {
    window[t] = { status: 'pending' }
    window.__TAURI__.core.invoke(c, a).then(
      (v) => { window[t] = { status: 'ok', value: v === undefined ? null : v } },
      (e) => { window[t] = { status: 'err', error: String(e) } },
    )
  }, ticket, cmd, args)
  await browser.waitUntil(
    async () => browser.execute(
      (t) => {
        const entry = window[t]
        return entry && entry.status !== 'pending'
      },
      ticket,
    ),
    { ...POLL, timeoutMsg: `invoke(${cmd}) never settled` },
  )
  const entry = await browser.execute((t) => {
    const e = window[t]
    delete window[t]
    return e
  }, ticket)
  return entry
}

// Per-spec clean state. The runner spawns a fresh app per spec file, but every
// app reads the persisted store at launch, so without this each spec inherits
// the previous spec's spaces, pane list and persisted trust list and results
// depend on spec order.
//
// Harness-only: reuses the app's existing commands, no test-only backend
// surface.
//   1. `pty_kill` every pane id the page still has in `__athenaTermMap`
//      (errors from already-dead ids are tolerated and not counted);
//   2. `store_delete` the two persisted keys a spec can dirty — `workspaces`
//      (frontend WORKSPACES_KEY, workspace.rs:21) and
//      `workspace.trusted_roots` (backend TRUSTED_ROOTS_KEY, mod.rs:266);
//   3. `location.reload()` so the frontend re-reads the now-empty store.
//
// The caller MUST wait for the app to mount again after this returns: the
// reload tears the page down.
export async function resetAppState(browser) {
  // Read the pane ids before the reload destroys the page.
  const paneIds = await browser.execute(() => Object.keys(window.__athenaTermMap || {}))

  let killedPtys = 0
  for (const id of paneIds) {
    const res = await invokeSettled(browser, 'pty_kill', { id })
    if (res.status === 'ok') killedPtys += 1
  }

  const deletedKeys = []
  for (const key of ['workspaces', 'workspace.trusted_roots']) {
    const res = await invokeSettled(browser, 'store_delete', { key })
    if (res.status === 'ok') deletedKeys.push(key)
  }

  // Plant a token the reload destroys, then reload. The navigation destroys
  // the execution context, so the driver may reject the call even though the
  // reload started — swallow that.
  const token = `__e2eReload_${Date.now()}_${ticketSeq++}`
  await browser.execute((t) => { window[t] = true }, token)
  await browser.execute(() => { location.reload() }).catch(() => {})

  // The OLD document keeps answering driver calls until the navigation
  // commits (~10ms later), so a mount wait issued now can be satisfied by the
  // pre-reload page — every following spec would then race the reload. Pause
  // with no driver traffic so the commit is not raced, then wait until the
  // token is gone, i.e. the NEW document is the one answering.
  await browser.pause(300)
  await browser.waitUntil(
    async () => {
      try {
        return !(await browser.execute((t) => window[t] === true, token))
      } catch {
        return false
      }
    },
    { timeout: 15000, interval: 250, timeoutMsg: 'app did not reload' },
  )

  return { killedPtys, deletedKeys, reloaded: true }
}
