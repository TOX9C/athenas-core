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
