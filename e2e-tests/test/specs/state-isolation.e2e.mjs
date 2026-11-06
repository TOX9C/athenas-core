// Acceptance proof for the harness-only per-spec reset (`resetAppState` in
// `test/helpers/ipc.mjs`, wired into `wdio.conf.mjs`'s `before` hook).
//
// The reset's return value is MEASURED: `pre`/`post` are captured with the
// real `store_has` command and the page's live `__athenaTermMap`, never from
// the reset's own bookkeeping. This spec is the witness — if the reset does
// nothing, test (a) below must go red on `post.panes`/`post.hasWorkspaces`.

import { e2eTrustRoot, setE2E, fillSpaceDir } from '../helpers/trust.mjs'
import { invokeSettled, resetAppState } from '../helpers/ipc.mjs'

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

// Pane count from the page's live terminal map. Only exists under
// `window.__athenaE2E` — 0 without it is honest, not "no panes".
const paneCount = () =>
  browser.execute(() => Object.keys(window.__athenaTermMap || {}).length)

const storeHas = async (key) => {
  const res = await invokeSettled(browser, 'store_has', { key })
  return res.status === 'ok' && res.value === true
}

describe('Per-spec state isolation', () => {
  // Runs first (mocha declaration order): whatever the global `before` hook's
  // reset did is visible here, before this spec mutates anything.
  it('starts a spec from the global before-hook reset', async () => {
    expect(await paneCount()).toBe(0)
    expect(await storeHas('workspaces')).toBe(false)
  })

  it('resetAppState clears a populated app and its persisted keys', async () => {
    await setE2E()
    const trustDir = e2eTrustRoot('state-isolation')

    expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll('button')).some((btn) =>
            (btn.textContent || '').includes('Terminal Workspace'),
          ),
        ),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Space modal did not open' },
    )

    expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
    await fillSpaceDir(browser, trustDir)
    expect((await clickButtonByText('Next >')).ok).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-shell')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Terminal configuration step did not appear' },
    )
    // The modal starts at 0 panes and disables Launch until one is added.
    await browser.execute(() => document.getElementById('add-shell')?.click())
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => {
          const label = Array.from(document.querySelectorAll('label, div, span'))
            .map((el) => (el.textContent || '').trim())
            .find((text) => /^Agents \(\d+\/16\)$/.test(text))
          const m = label && label.match(/Agents \((\d+)\/16\)/)
          return m ? Number(m[1]) : null
        })) > 0,
      { timeout: 5000, interval: 250, timeoutMsg: 'pane count did not reach 1' },
    )
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    await browser.waitUntil(
      async () => (await paneCount()) > 0,
      { timeout: 20000, interval: 250, timeoutMsg: 'no pane mounted after launching the space' },
    )
    await browser.waitUntil(
      async () => await storeHas('workspaces'),
      { timeout: 10000, interval: 250, timeoutMsg: 'workspaces key was never persisted' },
    )

    const reset = await resetAppState(browser)
    console.log(`[wdio] state-isolation reset -> ${JSON.stringify(reset)}`)

    // The app really was populated...
    expect(reset.pre.panes).toBeGreaterThan(0)
    expect(reset.pre.hasWorkspaces).toBe(true)
    // ...and the reset really cleared it.
    expect(reset.post.panes).toBe(0)
    expect(reset.post.hasWorkspaces).toBe(false)
    expect(reset.post.hasTrustedRoots).toBe(false)
  })
})
