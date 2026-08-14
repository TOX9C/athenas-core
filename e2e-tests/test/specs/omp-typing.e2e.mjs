import { expect } from '@wdio/globals'

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

async function metricsSnapshot() {
  const raw = await browser.execute(() => {
    const snapshot = window.__athenaMetrics?.snapshot
    return typeof snapshot === 'string' ? snapshot : null
  })
  if (!raw) return null
  try {
    return JSON.parse(raw)
  } catch {
    return null
  }
}

async function ptyWriteCount() {
  const snapshot = await metricsSnapshot()
  return snapshot?.ipc?.pty_write ?? null
}

describe('OMP terminal input regression', () => {
  afterEach(async () => {
    await browser.execute(() => {
      const originals = window.__ompOriginalConsole || {}
      for (const level of ['log', 'warn', 'error']) {
        if (originals[level]) console[level] = originals[level]
      }
      delete window.__ompOriginalConsole
    })
  })

  it('mounts OMP through xterm and forwards keyboard input to the PTY', async function () {
    this.timeout(120000)

    await browser.execute(() => {
      window.__athenaE2E = true
      window.__ompLogs = []
      window.__ompOriginalConsole = {}
      for (const level of ['log', 'warn', 'error']) {
        const original = console[level]
        window.__ompOriginalConsole[level] = original
        console[level] = (...args) => {
          window.__ompLogs.push({ level, message: args.map((arg) => String(arg)).join(' ') })
          original.apply(console, args)
        }
      }
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

    // The app's E2E bypass supplies /tmp when the directory is blank. Keep
    // this flow aligned with the proven workspace specs; synthetic input
    // events in WKWebView can otherwise race the modal's Dioxus signals.
    await browser.pause(250)
    expect((await clickButtonByText('Next >')).ok).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-omp')),
      { timeout: 10000, interval: 250, timeoutMsg: 'OMP agent row did not appear' },
    )

    expect((await browser.execute(() => {
      const button = document.getElementById('add-omp')
      if (!button) return false
      button.click()
      return true
    }))).toBe(true)
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    try {
      await browser.waitUntil(
        async () =>
          browser.execute(() => {
            const mount = document.querySelector('.xterm-mount[data-terminal-renderer="xterm"]')
            return (
              !!mount &&
              !!mount.querySelector('.xterm') &&
              !!mount.querySelector('.xterm-helper-textarea')
            )
          }),
        {
          timeout: 30000,
          interval: 500,
          timeoutMsg: 'OMP pane did not mount xterm with its input textarea',
        },
      )
    } catch (error) {
      const diagnostics = await browser.execute(() => ({
        logs: window.__ompLogs || [],
        mounts: Array.from(document.querySelectorAll('.xterm-mount')).map((mount) => ({
          id: mount.id,
          renderer: mount.getAttribute('data-terminal-renderer'),
          hasXterm: !!mount.querySelector('.xterm'),
          hasHelper: !!mount.querySelector('.xterm-helper-textarea'),
        })),
      }))
      console.log('[omp-typing] mount diagnostics:', JSON.stringify(diagnostics))
      throw error
    }

    // OMP enters a full-screen raw-mode UI during startup. Give its first
    // screen time to settle, then verify the automatic focus handoff. Do not
    // click or manually focus the pane before this assertion: that would hide
    // the original regression.
    await browser.pause(2500)
    const focusState = await browser.execute(() => {
      const mount = document.querySelector('.xterm-mount[data-terminal-renderer="xterm"]')
      const active = document.activeElement
      return {
        helperFocused: !!active?.classList?.contains('xterm-helper-textarea'),
        helperInMount: !!mount?.querySelector('.xterm-helper-textarea'),
        paneId: mount?.getAttribute('data-pane-id') || null,
        agentType: (() => {
          const paneId = mount?.getAttribute('data-pane-id')
          return Array.from(document.querySelectorAll('[data-agent-pill]'))
            .find((pill) => pill.getAttribute('data-agent-pane-id') === paneId)
            ?.getAttribute('data-agent-type') || null
        })(),
        hasFocusRing: !!mount?.closest('.pane-wrap')?.querySelector('.pane-focus-ring'),
        activeElementClass: active?.getAttribute?.('class') || null,
      }
    })
    console.log('[omp-typing] focus state:', JSON.stringify(focusState))
    expect(focusState.helperInMount).toBe(true)
    expect(focusState.paneId).toBeTruthy()
    expect(focusState.agentType).toBe('omp')
    // The native helper focus is the behavioral contract. The decorative
    // focus ring is rendered by a separate grid subscription and is not a
    // reliable WebKit-level input signal.
    expect(focusState.helperFocused).toBe(true)

    // The metric snapshot is refreshed asynchronously by the app root. Take
    // the baseline only after it exists, then send real WebDriver keyboard
    // events while the xterm helper textarea is focused. This exercises:
    // WKWebView key event → xterm hidden textarea → xterm onData → queued
    // frontend pty_write IPC, rather than directly invoking pty_write.
    let beforeWrites = null
    await browser.waitUntil(
      async () => {
        beforeWrites = await ptyWriteCount()
        return beforeWrites !== null
      },
      { timeout: 10000, interval: 250, timeoutMsg: 'Athena metrics snapshot never became available' },
    )

    await browser.keys('x')
    await browser.keys(['\uE007'])

    await browser.waitUntil(
      async () => {
        const afterWrites = await ptyWriteCount()
        return afterWrites !== null && afterWrites > beforeWrites
      },
      {
        timeout: 10000,
        interval: 250,
        timeoutMsg: 'OMP keyboard input did not produce a pty_write IPC call',
      },
    )
  })
})
