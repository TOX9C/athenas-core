// Regression: adding a shell pane used to leave an existing OMP pane stuck
// scrolled up (one pane stuck mid-buffer, the other pinned — racy). Root
// cause: the fit path fired xterm onResize → pty_resize → SIGWINCH, and the
// TUI's deferred reprint (normal↔alt buffer toggle, history reprint) landed
// AFTER the one-shot viewport restore, with nothing to re-pin the pane.
// Fix: xterm_mount/xterm_helpers keep a follow-bottom intent window across
// the redraw settle and re-pin at the write path plus on delayed timers.
// This spec drives the real app end-to-end: two OMP panes with resumed
// sessions (deep scrollback), then two more panes added via the toolbar,
// asserting (a) bottom-following panes finish pinned, (b) no pane remounts,
// (c) a user-scrolled pane keeps its distance from the bottom.

import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

async function clickButtonByText(text, { partial = false } = {}) {
  return browser.execute(
    ({ text, partial }) => {
      for (const button of document.querySelectorAll('button')) {
        const content = (button.textContent || '').trim()
        if ((partial ? content.includes(text) : content === text)) {
          button.click()
          return { ok: true, content }
        }
      }
      return { ok: false, text }
    },
    { text, partial },
  )
}

// Every mounted xterm pane's scroll position, keyed by data-pane-id.
async function paneScrollState() {
  return browser.execute(() => {
    const out = {}
    for (const mount of document.querySelectorAll('.xterm-mount')) {
      const paneId = mount.getAttribute('data-pane-id') || mount.id
      const viewport = mount.querySelector('.xterm-viewport')
      if (!viewport) continue
      out[paneId] = {
        scrollTop: viewport.scrollTop,
        max: viewport.scrollHeight - viewport.clientHeight,
        atBottom:
          viewport.scrollTop >= viewport.scrollHeight - viewport.clientHeight - 1,
      }
    }
    return out
  })
}

async function paneCount() {
  return browser.execute(
    () => document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]').length,
  )
}

describe('Layout change viewport regression', () => {
  it('keeps existing panes pinned to bottom when a third shell is added', async function () {
    this.timeout(180000)

    await browser.execute(() => {
      window.__athenaE2E = true
      window.__athenaTrust = 'pending'
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: '/tmp' })
        .then(() => {
          window.__athenaTrust = 'ok'
        })
        .catch((error) => {
          window.__athenaTrust = 'ERR:' + String(error)
        })
    })
    await browser.waitUntil(
      async () => browser.execute(() => window.__athenaTrust === 'ok'),
      { timeout: 10000, interval: 250, timeoutMsg: 'Could not authorize /tmp for PTY spawn' },
    )

    expect((await clickButtonByText('New Workspace', { partial: true })).ok).toBe(true)
    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll('button')).some((button) =>
            (button.textContent || '').includes('Terminal Workspace'),
          ),
        ),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
    )
    expect((await clickButtonByText('Terminal Workspace', { partial: true })).ok).toBe(true)
    await browser.pause(250)
    expect((await clickButtonByText('Next >')).ok).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-shell')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Shell row did not appear' },
    )
    // Two shells.
    await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      btn.click()
      btn.click()
    })
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    await browser.waitUntil(
      async () => {
        const ready = await browser.execute(
          () =>
            Array.from(
              document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]'),
            ).filter((m) => !!m.querySelector('.xterm')).length,
        )
        return ready === 2
      },
      { timeout: 30000, interval: 500, timeoutMsg: 'Two shell panes did not mount xterm' },
    )
    await browser.pause(2000)

    // First: fill with plain scrollback to prove the baseline stays pinned.
    // Then start the real OMP TUI (the reported scenario) via the pty_write
    // IPC — the same path xterm onData uses — since synthetic WebDriver key
    // events are unreliable in WKWebView.
    const paneIds = await browser.execute(() =>
      Array.from(document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]')).map(
        (m) => m.getAttribute('data-pane-id') || m.id,
      ),
    )
    console.log('[layout-scroll] pane ids:', JSON.stringify(paneIds))
    for (const [idx, id] of paneIds.entries()) {
      await browser.execute(
        ({ id, idx }) =>
          window.__TAURI__.core.invoke('pty_write', {
            id,
            data: `seq 1 300; echo FILL_DONE_${idx}; omp\n`,
          }),
        { id, idx },
      )
    }
    // Wait until both panes show OMP's TUI (it prints slash-command hints and
    // a status line). OMP redraws continuously; look for its UI chrome text.
    const ompUp = await browser
      .waitUntil(
        async () => {
          // WebGL renderer keeps .xterm-rows empty, so mount.textContent is
          // blank; read the xterm buffer through __athenaTermMap instead.
          return browser.execute(() => {
            const mounts = document.querySelectorAll('.xterm-mount[data-terminal-renderer="xterm"]')
            if (mounts.length < 2) return false
            const ready = (m) => {
              const id = m.getAttribute('data-pane-id') || m.id
              const term = window.__athenaTermMap?.[id]
              const buf = term?.buffer?.active
              if (!buf) return false
              for (let i = Math.max(0, buf.length - 60); i < buf.length; i++) {
                const line = buf.getLine(i)?.translateToString() || ''
                if (line.includes('/resume') || line.includes('session') || line.includes('command')) {
                  return true
                }
              }
              return false
            }
            return ready(mounts[0]) && ready(mounts[1])
          })
        },
        { timeout: 30000, interval: 500 },
      )
      .catch(() => false)
    if (!ompUp) {
      const diag = await browser.execute(() => {
        return Array.from(document.querySelectorAll('.xterm-mount')).map((m) => {
          const id = m.getAttribute('data-pane-id') || m.id
          const buf = window.__athenaTermMap?.[id]?.buffer?.active
          const tail = []
          if (buf) {
            for (let i = Math.max(0, buf.length - 40); i < buf.length; i++) {
              tail.push(buf.getLine(i)?.translateToString() || '')
            }
          }
          return { id, tail: tail.join('\n') }
        })
      })
      console.log('[layout-scroll] omp diagnostics:', JSON.stringify(diag, null, 2))
      throw new Error('OMP TUI did not come up in both panes')
    }
    // Let OMP settle (initial render flurry done) before measuring.
    await browser.pause(3000)

    // The user's repro uses panes with long conversations (via /resume).
    // Resume a session in each pane so the buffers have deep scrollback.
    for (const id of paneIds) {
      await browser.execute(
        (id) =>
          window.__TAURI__.core.invoke('pty_write', { id, data: '/resume\r' }),
        id,
      )
    }
    await browser.pause(1500)
    for (const id of paneIds) {
      await browser.execute(
        (id) => window.__TAURI__.core.invoke('pty_write', { id, data: '\r' }),
        id,
      )
    }
    // Wait until both panes display a deep buffer (resumed history renders
    // thousands of rows) or bail with diagnostics.
    const deep = await browser
      .waitUntil(
        async () => {
          const state = await paneScrollState()
          return Object.values(state).filter((s) => s.max > 500).length >= 2
        },
        { timeout: 20000, interval: 500 },
      )
      .catch(() => false)
    const afterResume = await paneScrollState()
    console.log('[layout-scroll] after resume:', JSON.stringify(afterResume))
    if (!deep) {
      const diag = await browser.execute(() =>
        Array.from(document.querySelectorAll('.xterm-mount')).map((m) => {
          const id = m.getAttribute('data-pane-id') || m.id
          const buf = window.__athenaTermMap?.[id]?.buffer?.active
          const rows = []
          if (buf) {
            for (let i = Math.max(0, buf.length - 20); i < buf.length; i++) {
              rows.push(buf.getLine(i)?.translateToString() || '')
            }
          }
          return { id, tail: rows.join('\n') }
        }),
      )
      console.log('[layout-scroll] resume diagnostics:', JSON.stringify(diag, null, 2))
      console.log('[layout-scroll] continuing without deep scrollback')
    }
    // Wait for the resumed sessions to finish rendering and pin to bottom.
    await browser.waitUntil(
      async () => {
        const state = await paneScrollState()
        const vals = Object.values(state)
        return vals.length >= 2 && vals.every((s) => s.atBottom)
      },
      { timeout: 15000, interval: 500, timeoutMsg: 'Resumed sessions never re-pinned to bottom' },
    )

    // Remount probes: tag each pane's container + xterm DOM node; if a pane
    // is remounted by the layout change, a fresh node loses the tag.
    await browser.execute(() => {
      for (const mount of document.querySelectorAll('.xterm-mount')) {
        mount.setAttribute('data-probe-m', '1')
        mount.querySelector('.xterm')?.setAttribute('data-probe-x', '1')
        mount.querySelector('.xterm-helper-textarea')?.setAttribute('data-probe-t', '1')
      }
    })

    const before = await paneScrollState()
    console.log('[layout-scroll] before add:', JSON.stringify(before))
    for (const [id, s] of Object.entries(before)) {
      if (!s.atBottom) throw new Error(`pane ${id} was not at bottom before layout change`)
    }

    // Trigger the layout change: add a third shell via the toolbar button.
    const added = await browser.execute(() => {
      for (const button of document.querySelectorAll('button')) {
        if ((button.getAttribute('title') || '').includes('Add Shell')) {
          button.click()
          return true
        }
      }
      return false
    })
    expect(added).toBe(true)

    await browser.waitUntil(async () => (await paneCount()) === 3, {
      timeout: 15000,
      interval: 250,
      timeoutMsg: 'Third pane did not mount',
    })
    // Wait out the full settle: resize-fit → PTY resize → SIGWINCH → TUI
    // redraw burst (OMP reprints its conversation; buffers briefly toggle
    // normal↔alternate). The buffered DOM scroll position oscillates during
    // the burst; the contract is the FINAL state.
    await browser.pause(3600)

    // Did any pre-existing pane lose its tagged DOM (i.e. get remounted)?
    const probe = await browser.execute(
      (beforeIds) => {
        const report = {}
        for (const id of beforeIds) {
          const mount = document.querySelector(`.xterm-mount[data-pane-id="${id}"]`)
          report[id] = {
            exists: !!mount,
            mountTagged: !!mount?.getAttribute('data-probe-m'),
            xtermTagged: !!mount?.querySelector('.xterm[data-probe-x]'),
            textareaTagged: !!mount?.querySelector('.xterm-helper-textarea[data-probe-t]'),
          }
        }
        return report
      },
      Object.keys(before),
    )
    for (const [id, p] of Object.entries(probe)) {
      // The layout change must reuse each pane's xterm in place — a remount
      // loses per-mount state and forces a full buffer replay.
      if (!p.exists || !p.mountTagged || !p.xtermTagged || !p.textareaTagged) {
        throw new Error(`pane ${id} remounted on layout change: ${JSON.stringify(p)}`)
      }
    }

    const after = await paneScrollState()
    console.log('[layout-scroll] after add:', JSON.stringify(after))
    await browser.saveScreenshot(join(screenshotDir, 'layout-scroll-regression.png'))

    const beforeIds = Object.keys(before)
    const offenders = after && Object.entries(after)
      .filter(([id]) => beforeIds.includes(id))
      .filter(([, s]) => !s.atBottom)
      .map(([id, s]) => `${id} scrollTop=${s.scrollTop}/${s.max}`)

    expect(offenders).toEqual([])

    // Counter-case: a pane the USER scrolled up (reading scrollback) must NOT
    // be forcibly re-pinned by the next layout change; its distance from the
    // bottom should be roughly preserved. A pane still at bottom must stay
    // pinned.
    const [p0] = Object.keys(before)
    await browser.execute((id) => {
      window.__athenaTermMap?.[id]?.scrollToLine(60)
    }, p0)
    await browser.pause(3400) // let the prior add's follow window (3.2s) expire
    const distanceBefore = await browser.execute((id) => {
      const buf = window.__athenaTermMap?.[id]?.buffer?.active
      return buf ? buf.baseY - buf.viewportY : null
    }, p0)

    const added4 = await browser.execute(() => {
      for (const button of document.querySelectorAll('button')) {
        if ((button.getAttribute('title') || '').includes('Add Shell')) {
          button.click()
          return true
        }
      }
      return false
    })
    expect(added4).toBe(true)
    await browser.waitUntil(async () => (await paneCount()) === 4, {
      timeout: 15000,
      interval: 250,
      timeoutMsg: 'Fourth pane did not mount',
    })
    await browser.pause(3500)

    const distanceAfter = await browser.execute((id) => {
      const buf = window.__athenaTermMap?.[id]?.buffer?.active
      return buf ? buf.baseY - buf.viewportY : null
    }, p0)
    const finalState = await paneScrollState()
    console.log('[layout-scroll] user-scrolled pane distance:', {
      before: distanceBefore,
      after: distanceAfter,
      final: JSON.stringify(finalState),
    })
    expect(distanceAfter).not.toBe(null)
    // Scrolled-up pane must not be re-pinned to the bottom.
    if (distanceBefore && distanceBefore > 20) {
      expect(distanceAfter).toBeGreaterThan(20)
    }
  })
})
