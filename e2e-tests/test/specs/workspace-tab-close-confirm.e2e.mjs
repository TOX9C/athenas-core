// Regression: titlebar "x" close on a space with an active agent must show
// the ConfirmDialog and the dialog must SURVIVE — previously it mounted for a
// single frame inside the remounting WorkspaceTab subtree and was torn down
// before paint (user saw only a brief blur on the topbar). The sidebar row "x"
// always worked; this spec pins the titlebar path to the same behavior.

import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mkdirSync, writeFileSync } from 'node:fs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const shotDir = join(__dirname, '..', 'screenshots', 'workspace-tab-close-confirm')
mkdirSync(shotDir, { recursive: true })

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

// Locate the titlebar tab for the most-recently-created workspace, then click
// its close affordance (the "Close workspace" icon-only button inside .workspace-tab).
async function clickLastTabClose() {
  return browser.execute(() => {
    const tabs = Array.from(document.querySelectorAll('.workspace-tab'))
    const tab = tabs[tabs.length - 1]
    if (!tab) return { ok: false, reason: 'no_tab' }
    const closeBtn = Array.from(tab.querySelectorAll('button')).find(
      (b) => b.getAttribute('aria-label') === 'Close workspace',
    )
    if (!closeBtn) return { ok: false, reason: 'no_close_btn' }
    closeBtn.click()
    return { ok: true }
  })
}

async function dialogState() {
  return browser.execute(() => {
    const dlg = document.querySelector('div[role="dialog"]')
    if (!dlg) return { open: false }
    const text = (dlg.textContent || '').replace(/\s+/g, ' ').trim()
    const hasCancel = Array.from(dlg.querySelectorAll('button')).some(
      (b) => (b.textContent || '').trim() === 'Cancel',
    )
    const hasConfirm = Array.from(dlg.querySelectorAll('button')).some(
      (b) => (b.textContent || '').trim() === 'Close Workspace',
    )
    return { open: true, text, hasCancel, hasConfirm }
  })
}

describe('workspace tab close confirmation (titlebar)', () => {
  it('shows a persistent ConfirmDialog when closing a space that has a working agent', async function () {
    this.timeout(180000)

    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // Make /tmp trusted so the new space's shell panes can spawn — same
    // pattern used by agent-badges.
    await browser.execute(() => {
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: '/tmp' })
        .catch((e) => console.error('[tab-close] trust-root failed:', String(e)))
      return true
    })

    // ── 1. Create a new workspace with one shell pane (Terminal Workspace) ─
    expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll('button')).some((b) =>
            (b.textContent || '').includes('Terminal Workspace'),
          ),
        ),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Space modal did not open' },
    )
    expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
    expect((await clickButtonByText('Next >')).ok).toBe(true)
    await browser.waitUntil(async () => browser.execute(() => !!document.getElementById('add-shell')), {
      timeout: 10000,
      interval: 250,
      timeoutMsg: 'Terminal configuration step did not appear',
    })
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
          const last = grids[grids.length - 1]
          return !!last && last.querySelectorAll('.xterm-mount').length >= 1
        }),
      { timeout: 25000, interval: 500, timeoutMsg: 'New space did not mount a pane' },
    )
    await browser.pause(1000)

    // ── 2. Simulate an active agent in that pane (same shape as agent-badges)
    const paneId = await browser.execute(() => {
      const grids = Array.from(document.querySelectorAll('.workspace-grid-root'))
      const last = grids[grids.length - 1]
      const wrap = last && last.querySelector('.pane-wrap')
      return wrap ? wrap.getAttribute('data-pane-id') : null
    })
    expect(paneId).toBeTruthy()

    const cmd =
      "bash -c \"exec -a claude sh -c 'echo one; sleep 0.3; echo two; sleep 0.3; echo three; sleep 8'\"\n"
    await browser.waitUntil(
      async () => {
        await browser.execute(
          ({ paneId, data }) => {
            const s = window.__athenaWrite
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
      { timeout: 20000, interval: 700, timeoutMsg: 'pty_write never accepted' },
    )

    // Wait until the UI registers a working agent in the newest space (this is
    // what flips close_requires_confirm on for that tab).
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const rows = Array.from(document.querySelectorAll('.workspace-row'))
          const row = rows[rows.length - 1]
          if (!row) return false
          return Array.from(row.querySelectorAll('.badge')).some(
            (b) => b.getAttribute('aria-label') === 'Agents working',
          )
        }),
      { timeout: 20000, interval: 500, timeoutMsg: 'Agents working badge never appeared' },
    )

    // ── 3. Click the titlebar tab's close affordance ──────────────────────
    const before = await browser.execute(() => document.querySelectorAll('.workspace-tab').length)
    const clicked = await clickLastTabClose()
    expect(clicked.ok).toBe(true)

    // The dialog must appear…
    await browser.waitUntil(
      async () => (await dialogState()).open,
      { timeout: 5000, interval: 100, timeoutMsg: 'ConfirmDialog never appeared after tab x click' },
    )

    // … and it must STILL be there after a beat (the regression was a
    // one-frame flash torn down by the tab remount).
    await browser.pause(800)
    const persisted = await dialogState()
    expect(persisted.open, 'ConfirmDialog disappeared — tab-close regression is back').toBe(true)
    expect(persisted.hasCancel, 'ConfirmDialog missing Cancel button').toBe(true)
    expect(persisted.hasConfirm, 'ConfirmDialog missing Close Workspace button').toBe(true)
    expect(persisted.text).toContain('Its agents are still active and will be stopped')

    await browser.saveScreenshot(join(shotDir, 'tab-close-confirm-open.png'))

    // ── 4. Cancel keeps the space ─────────────────────────────────────────
    expect((await clickButtonByText('Cancel')).ok).toBe(true)
    await browser.pause(400)
    const afterCancelCount = await browser.execute(() => document.querySelectorAll('.workspace-tab').length)
    expect(afterCancelCount, 'space was removed despite Cancel').toBe(before)

    // ── 5. Re-open via the same tab and confirm removes the space ─────────
    // The fake agent already exited; close_requires_confirm depends on the
    // badge having cleared, so only proceed if confirm is still required —
    // skip gracefully otherwise rather than flake the suite.
    const confirmAgain = await clickLastTabClose()
    expect(confirmAgain.ok).toBe(true)
    await browser.pause(600)
    const second = await dialogState()
    if (second.open) {
      expect((await clickButtonByText('Close Workspace')).ok).toBe(true)
      await browser.pause(600)
      const afterConfirmCount = await browser.execute(() => document.querySelectorAll('.workspace-tab').length)
      expect(afterConfirmCount, 'space still present after Close Workspace').toBe(before - 1)
    } else {
      // Agent already gone → close was immediate (also correct).
      const afterConfirmCount = await browser.execute(() => document.querySelectorAll('.workspace-tab').length)
      expect(afterConfirmCount).toBe(before - 1)
    }
  })
})
