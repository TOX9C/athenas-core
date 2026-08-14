import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const REPO_DIR = '/Users/apollo/Documents/athenas-core'

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

describe('Diag — xterm mount after rebuild', () => {
  it('launches a terminal space and dumps DOM', async function () {
    this.timeout(120000)
    await browser.execute(() => { window.__athenaE2E = true })
    await browser.pause(2500)

    await clickButtonByText('New Workspace', { partial: true })
    await browser.waitUntil(
      async () => browser.execute(() =>
        Array.from(document.querySelectorAll('button')).some((b) => (b.textContent || '').includes('Terminal Workspace'))),
      { timeout: 10000, interval: 250 },
    )
    await clickButtonByText('Terminal Workspace', { partial: true })
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('input[placeholder="/path/to/project"]')),
      { timeout: 10000, interval: 250 },
    )
    await browser.execute(
      ({ name, dir }) => {
        const setVal = (el, value) => {
          const proto = Object.getPrototypeOf(el)
          const setter = Object.getOwnPropertyDescriptor(proto, 'value').set
          setter.call(el, value)
          el.dispatchEvent(new Event('input', { bubbles: true }))
        }
        setVal(document.querySelector('input[placeholder="my-project"]'), name)
        setVal(document.querySelector('input[placeholder="/path/to/project"]'), dir)
        return { ok: true }
      },
      { name: 'athenas-core', dir: REPO_DIR },
    )
    await clickButtonByText('Next >')
    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-shell')),
      { timeout: 10000, interval: 250 },
    )
    await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      for (let i = 0; i < 2; i += 1) btn.click()
      return { ok: true }
    })
    await browser.pause(600)
    await clickButtonByText('Launch Space')
    await browser.pause(12000)

    const dump = await browser.execute(() => {
      const mounts = Array.from(document.querySelectorAll('.xterm-mount'))
      return {
        mountCount: mounts.length,
        withXterm: mounts.filter((m) => !!m.querySelector('.xterm')).length,
        terminalGlobal: typeof window.Terminal,
        bodySample: (document.body.textContent || '').slice(0, 400),
        errorText: (document.body.textContent || '').includes('Could not')
          ? (document.body.textContent || '').split('Could not')[1]?.slice(0, 200)
          : null,
        modalOpen: !!document.querySelector('.modal'),
      }
    })
    console.log('[DIAG]', JSON.stringify(dump, null, 1))
  })
})
