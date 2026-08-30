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

async function notificationTitles() {
  const raw = await browser.execute(() =>
    window.__TAURI__.core
      .invoke('notification_history', { limit: 50 })
      .then((json) => json)
      .catch((error) => 'ERR:' + String(error)),
  )
  if (typeof raw !== 'string' || raw.startsWith('ERR:')) return []
  try {
    return JSON.parse(raw).map((n) => `${n.title}: ${n.message}`)
  } catch {
    return []
  }
}

describe('OMP turn-finished notification', () => {
  it('notifies when an OMP turn ends after typing hello', async function () {
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
      async () => browser.execute(() => !!document.getElementById('add-omp')),
      { timeout: 10000, interval: 250, timeoutMsg: 'OMP agent row did not appear' },
    )
    expect(
      await browser.execute(() => {
        const button = document.getElementById('add-omp')
        if (!button) return false
        button.click()
        return true
      }),
    ).toBe(true)
    expect((await clickButtonByText('Launch Space')).ok).toBe(true)

    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const mount = document.querySelector('.xterm-mount[data-terminal-renderer="xterm"]')
          return !!mount?.querySelector('.xterm-helper-textarea')
        }),
      { timeout: 30000, interval: 500, timeoutMsg: 'OMP pane did not mount xterm' },
    )

    // Let OMP's TUI settle; the proven omp-typing spec shows focus lands on
    // the xterm helper textarea on its own.
    await browser.pause(3000)

    // Baseline: no stale "finished" notification from earlier tests in the
    // same app instance.
    const baseline = await notificationTitles()

    // Type "hello" and submit the prompt.
    await browser.keys('hello')
    await browser.keys([''])

    // Wait for OMP to answer: the turn end must surface an "Agent finished"
    // notification in the backend history (via the OSC 6337 marker pushed by
    // the athena-notify OMP extension, or the session-log poll).
    await browser.waitUntil(
      async () => {
        const titles = await notificationTitles()
        return titles.some((t) => t.startsWith('Agent finished')) && titles.length > baseline.length
      },
      {
        timeout: 90000,
        interval: 1500,
        timeoutMsg: `No "Agent finished" notification appeared. Seen: ${JSON.stringify(await notificationTitles())}`,
      },
    )

    const final = await notificationTitles()
    console.log('[omp-finish] notifications:', JSON.stringify(final))
    expect(final.some((t) => t.startsWith('Agent finished'))).toBe(true)
  })
})
