// Shared Tauri IPC bridge for the e2e suite.
//
// `browser.execute` does NOT await a Promise returned by its function (this
// repo proved it in athena-chat-stub / kanban-persistence), so a
// fire-and-forget `invoke` can still be in flight when the next command runs.
// `invokeSettled` bridges the invoke through a window ticket and polls it
// synchronously, so the caller really knows it settled.

const POLL = { timeout: 30000, interval: 200 }

// The two persisted keys a spec can dirty: `workspaces` (frontend
// WORKSPACES_KEY, workspace.rs:21) and `workspace.trusted_roots` (backend
// TRUSTED_ROOTS_KEY, mod.rs:266).
const RESET_KEYS = ['workspaces', 'workspace.trusted_roots']

// No driver traffic may be issued during a document's teardown. `location.reload()`
// returns before the navigation commits, and an eval delivered across the
// boundary runs in BOTH documents, resolving the same pending script id twice —
// the patched plugin's `resolve` does `.remove(&id).expect(...)`
// (patches/tauri-plugin-webdriver-automation/src/lib.rs:28), so the second
// resolve panics with the pending-scripts mutex held and poisons it: every
// later eval dies at server.rs:103 and the app is wedged for good. Reproduced
// (see the fix report). The shim offers no driver-visible navigation signal to
// wait on — `browser.refresh()` and `browser.url()` both return in ~13 ms,
// before the new document exists (measured) — so the commit window is bridged
// client-side: `browser.pause` issues no driver command, so it cannot race the
// teardown. The commit is ~10-50 ms; 300 ms is a bounded, several-times margin.
const RELOAD_COMMIT_MS = 300

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

// MEASURED observable app state — never derived from what the reset itself
// did. `panes` is the count of live entries in the page's `__athenaTermMap`
// (the frontend creates it only when `window.__athenaE2E` is set, so this is
// honestly 0 when the flag was never set — do not read that as "no panes
// exist"). The two booleans come from the real `store_has` command, so they
// witness the store, not the reset's own bookkeeping.
async function measureState(browser) {
  const panes = await browser.execute(
    () => Object.keys(window.__athenaTermMap || {}).length,
  )
  const has = async (key) => {
    const res = await invokeSettled(browser, 'store_has', { key })
    return res.status === 'ok' && res.value === true
  }
  return {
    panes,
    hasWorkspaces: await has('workspaces'),
    hasTrustedRoots: await has('workspace.trusted_roots'),
  }
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
//   2. `store_delete` the two persisted keys a spec can dirty;
//   3. reload — but ONLY when the page actually held state, so the frontend
//      re-reads the emptied store. The remaining steps are conditional on
//      whether that happened.
//
// Returns `{ pre, post, killedPtys, deletedKeys, reloaded }`:
//   - `pre`/`post` are the MEASURED state above, captured before mutating and
//     after the reload commits — the witness that the reset actually reset.
//   - `killedPtys` counts the `pty_kill` calls that returned ok. It is 0 unless
//     `window.__athenaE2E` was already set when the reset ran, because the
//     pane map only exists under that flag.
//   - `deletedKeys` lists the `store_delete` calls that returned ok; it is
//     unconditional, because `store_delete` returns Ok for an absent key
//     (store.rs:267). Use `pre`/`post`, not this list, to prove deletion.
//   - `reloaded` is false whenever the page held nothing to drop.
//
// The caller MUST wait for the app to mount again when `reloaded` is true: the
// reload tears the page down.
export async function resetAppState(browser) {
  const pre = await measureState(browser)

  // Read the pane ids before the reload destroys the page.
  const paneIds = await browser.execute(() => Object.keys(window.__athenaTermMap || {}))

  let killedPtys = 0
  for (const id of paneIds) {
    const res = await invokeSettled(browser, 'pty_kill', { id })
    if (res.status === 'ok') killedPtys += 1
  }

  const deletedKeys = []
  for (const key of RESET_KEYS) {
    const res = await invokeSettled(browser, 'store_delete', { key })
    if (res.status === 'ok') deletedKeys.push(key)
  }

  // Reload only when the page held state to drop. A fresh app (every spec: the
  // runner spawns one per spec file) has no panes and nothing loaded from the
  // store, so there is nothing for a reload to clear — and skipping it skips
  // the only navigation-teardown window in the run. When state IS present
  // (a mid-spec reset, or an app that loaded a persisted workspace), reload so
  // the frontend re-reads the emptied store.
  const dirty = pre.panes > 0 || pre.hasWorkspaces || pre.hasTrustedRoots
  let reloaded = false
  if (dirty) {
    // Plant a token the reload destroys; the token is the driver-visible
    // navigation signal the wait below is keyed to.
    const token = `__e2eReload_${Date.now()}_${ticketSeq++}`
    await browser.execute((t) => { window[t] = true }, token)
    // The navigation destroys the execution context, so the driver may reject
    // this call even though the reload started — swallow that.
    await browser.execute(() => { location.reload() }).catch(() => {})
    // Bridge the commit window with NO driver traffic (see RELOAD_COMMIT_MS),
    // then — now that navigation has committed — wait for the new document to
    // answer, i.e. the token to be gone. Polling starts after the window, so
    // it never lands mid-teardown.
    await browser.pause(RELOAD_COMMIT_MS)
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
    reloaded = true
  }

  const post = await measureState(browser)
  return { pre, post, killedPtys, deletedKeys, reloaded }
}
