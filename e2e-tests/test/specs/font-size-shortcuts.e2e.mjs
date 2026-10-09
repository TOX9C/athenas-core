import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), '..', '..', '..')

describe('Shared font-size shortcuts', () => {
  const dispatchShortcut = async (key, shift = false) => browser.execute(
    ({ key, shift }) => {
      const target = document.querySelector('.app-root')
      if (!target) return { ok: false, error: 'app root not found' }

      const event = new KeyboardEvent('keydown', {
        key,
        code: key === '=' ? 'Equal' : key === '+' ? 'Equal' : 'Minus',
        metaKey: true,
        shiftKey: shift,
        bubbles: true,
        cancelable: true,
      })
      target.dispatchEvent(event)
      return {
        ok: true,
        defaultPrevented: event.defaultPrevented,
        fontSize: getComputedStyle(document.documentElement)
          .getPropertyValue('--fontSize')
          .trim(),
      }
    },
    { key, shift },
  )

  it('maps Command equals/plus/minus to shared font-size changes', async () => {
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.app-root')),
      { timeout: 25000, interval: 250, timeoutMsg: 'App root did not mount' },
    )

    const initial = await browser.execute(() => Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
    ))
    expect(initial).toBeGreaterThanOrEqual(10)
    expect(initial).toBeLessThanOrEqual(24)

    const increasedByEquals = await dispatchShortcut('=')
    expect(increasedByEquals.ok).toBe(true)
    expect(increasedByEquals.defaultPrevented).toBe(true)
    expect(Number.parseFloat(increasedByEquals.fontSize)).toBe(Math.min(initial + 1, 24))

    const increasedByPlus = await dispatchShortcut('+', true)
    expect(increasedByPlus.defaultPrevented).toBe(true)
    expect(Number.parseFloat(increasedByPlus.fontSize)).toBe(Math.min(initial + 2, 24))

    const decreasedByMinus = await dispatchShortcut('-')
    expect(decreasedByMinus.defaultPrevented).toBe(true)
    expect(Number.parseFloat(decreasedByMinus.fontSize)).toBe(Math.min(initial + 1, 24))

    // Restore the persisted preference so this spec does not affect later
    // specs that share the local app data directory.
    await browser.execute(({ initial }) => {
      const target = document.querySelector('.app-root')
      if (!target) return
      const key = initial < Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
      ) ? '-' : '='
      const steps = Math.abs(initial - Number.parseFloat(
        getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
      ))
      for (let i = 0; i < steps; i += 1) {
        target.dispatchEvent(new KeyboardEvent('keydown', {
          key,
          code: key === '=' ? 'Equal' : 'Minus',
          metaKey: true,
          bubbles: true,
          cancelable: true,
        }))
      }
    }, { initial })
  })

  it('keeps the viewport pinned at bottom during Cmd+plus/minus zoom', async function () {
    this.timeout(120000)

    // Authorize PTY spawn, then open a one-shell Terminal Workspace (same
    // modal flow as layout-scroll-regression).
    // Trust a guaranteed-present, non-forbidden dir. The e2e runner always
    // executes from the repo, and the repo dir is already in the persisted
    // trusted roots, so this is a no-op that skips the native dialog.
    await browser.execute((repoRoot) => {
      window.__athenaE2E = true
      window.__athenaTrust = 'pending'
      window.__TAURI__.core
        .invoke('workspace_add_trusted_root', { dir: repoRoot })
        .then(() => { window.__athenaTrust = 'ok' })
        .catch((error) => { window.__athenaTrust = 'ERR:' + String(error) })
    }, repoRoot)
    const trusted = await browser
      .waitUntil(
        async () => browser.execute(() => window.__athenaTrust === 'ok'),
        { timeout: 10000, interval: 250 },
      )
      .then(() => true)
      .catch(() => false)
    if (!trusted) {
      const flag = await browser.execute(() => String(window.__athenaTrust))
      throw new Error(`trusted root not authorized (flag=${flag})`)
    }

    const clickButtonByText = (text, partial = true) => browser.execute(
      ({ text, partial }) => {
        for (const button of document.querySelectorAll('button')) {
          const content = (button.textContent || '').trim()
          if ((partial ? content.includes(text) : content === text)) {
            button.click()
            return true
          }
        }
        return false
      },
      { text, partial },
    )
    // The app may restore a previous session's workspace (specs share the
    // persisted store). Only drive the New Workspace modal when no terminal
    // pane is already mounted.
    const hasTerminal = () => browser.execute(() =>
      !!document.querySelector('.xterm-mount[data-terminal-renderer="xterm"] .xterm'),
    )
    if (!(await hasTerminal())) {
      expect(await clickButtonByText('New Workspace')).toBe(true)
      await browser.waitUntil(
        () => browser.execute(() =>
          Array.from(document.querySelectorAll('button')).some((btn) =>
            (btn.textContent || '').includes('Terminal Workspace'),
          ),
        ),
        { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
      )
      expect(await clickButtonByText('Terminal Workspace')).toBe(true)
      await browser.pause(250)
      expect(await clickButtonByText('Next >', false)).toBe(true)
      expect(await clickButtonByText('Launch Space')).toBe(true)
    }
    await browser.waitUntil(
      () => browser.execute(() =>
        !!document.querySelector('.xterm-mount[data-terminal-renderer="xterm"] .xterm canvas'),
      ),
      { timeout: 30000, interval: 500, timeoutMsg: 'terminal pane did not mount' },
    )
    await browser.pause(3000) // initial fits + font settle

    // Deep scrollback so a reflow clamp would visibly land at the top. Probed
    // via .xterm-viewport DOM metrics (no __athenaTermMap — restored panes
    // mounted before this spec armed __athenaE2E and are absent from it).
    const paneId = await browser.execute(() => {
      const m = document.querySelector('.xterm-mount[data-terminal-renderer="xterm"]')
      return m ? m.getAttribute('data-pane-id') || m.id : null
    })
    expect(paneId).not.toBe(null)
    await browser.execute(
      (id) => window.__TAURI__.core.invoke('pty_write', { id, data: 'seq 1 300; echo FILL_DONE\n' }),
      paneId,
    )
    const readViewport = () => browser.execute(() => {
      const m = document.querySelector('.xterm-mount[data-terminal-renderer="xterm"]')
      if (!m) return { exists: false }
      const vp = m.querySelector('.xterm-viewport')
      const screen = m.querySelector('.xterm-screen')
      return {
        exists: true,
        opacity: screen ? getComputedStyle(screen).opacity : '',
        hasScroll: vp ? vp.scrollHeight > vp.clientHeight + 50 : false,
        atBottom: vp ? vp.scrollTop >= vp.scrollHeight - vp.clientHeight - 2 : null,
      }
    })
    await browser.waitUntil(
      async () => (await readViewport()).hasScroll,
      { timeout: 15000, interval: 250, timeoutMsg: 'scrollback did not fill' },
    )

    const fontBefore = await browser.execute(() => Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
    ))

    // The regression invariant: at NO sample during the zoom settle may the
    // screen be VISIBLE while the viewport is not at the bottom. (The bug
    // presented reflow-clamped frames — viewport at scrollback top — between
    // the options set and the deferred viewport restore.)
    const sample = readViewport

    const violations = []
    const watchSettle = async () => {
      for (let i = 0; i < 30; i += 1) {
        const s = await sample()
        if (s.exists && s.opacity !== '0' && s.atBottom === false) {
          violations.push(s)
        }
        await browser.pause(50)
      }
    }

    await dispatchShortcut('-')
    await watchSettle()
    expect(violations).toEqual([])

    // Settled state: screen revealed again, still pinned at bottom.
    const settled = await sample()
    expect(settled.opacity).not.toBe('0')
    expect(settled.atBottom).toBe(true)

    // Cmd+= back up: same invariant on the grow direction, and restores the
    // persisted preference for later specs.
    await dispatchShortcut('=')
    await watchSettle()
    expect(violations).toEqual([])
    expect((await sample()).atBottom).toBe(true)
    const fontAfter = await browser.execute(() => Number.parseFloat(
      getComputedStyle(document.documentElement).getPropertyValue('--fontSize'),
    ))
    expect(fontAfter).toBe(fontBefore)
  })
})
