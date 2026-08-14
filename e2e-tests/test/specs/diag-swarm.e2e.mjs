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

async function setField(selector, value) {
  return browser.execute(
    ({ selector, value }) => {
      const field = document.querySelector(selector)
      if (!field) return false
      const setter = Object.getOwnPropertyDescriptor(field.constructor.prototype, 'value')?.set
      setter?.call(field, value)
      field.dispatchEvent(new Event('input', { bubbles: true }))
      field.dispatchEvent(new Event('change', { bubbles: true }))
      return true
    },
    { selector, value },
  )
}

describe('Diag — swarm launch', () => {
  it('launches a swarm mission and dumps state', async function () {
    this.timeout(120000)
    await browser.execute(() => { window.__athenaE2E = true })
    await browser.pause(2000)

    await clickButtonByText('New Workspace', { partial: true })
    await browser.waitUntil(
      async () => browser.execute(() => document.body.textContent.includes('Swarm Mission')),
      { timeout: 10000, interval: 250 },
    )
    await clickButtonByText('Swarm Mission', { partial: true })
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('input[placeholder="/path/to/project"]')),
      { timeout: 10000, interval: 250 },
    )
    await setField('input[placeholder="my-project"]', 'mission')
    await setField('input[placeholder="/path/to/project"]', REPO_DIR)
    await setField('textarea[placeholder="Describe what the swarm should accomplish..."]', 'Diagnose swarm mission launch')
    await clickButtonByText('Next >')
    await browser.waitUntil(
      async () => browser.execute(() => document.body.textContent.includes('Team (3 agents)')),
      { timeout: 10000, interval: 250 },
    )
    await clickButtonByText('Launch Swarm')
    await browser.pause(8000)

    const dump = await browser.execute(() => {
      const body = document.body.textContent || ''
      const modalText = document.querySelector('.modal, [class*="modal"]')?.textContent || ''
      const tabs = Array.from(document.querySelectorAll('.workspace-tabs *'))
        .map((el) => (el.textContent || '').trim())
        .filter((t) => t.length > 0 && t.length < 30)
      const activeBtn = Array.from(document.querySelectorAll('button'))
        .filter((b) => (b.textContent || '').trim().length < 20)
        .map((b) => ({
          text: (b.textContent || '').trim(),
          color: b.style.color || '',
        }))
        .filter((b) => b.text.includes('workspace') || b.text.includes('kanban') || b.text.includes('swarm'))
      return {
        hasModal: body.includes('New Workspace') || modalText.length > 0,
        modalText: modalText.slice(0, 300),
        tabs,
        panelButtons: activeBtn,
        swarmBoard: !!document.querySelector('.swarm-board'),
        agentCards: document.querySelectorAll('.agent-card').length,
        errorInBody: body.includes('Could not') ? body.split('Could not')[1]?.slice(0, 200) : null,
      }
    })
    console.log('[DIAG]', JSON.stringify(dump, null, 1))

    const stateRaw = await browser.execute(async () => {
      try {
        return await window.__TAURI__.core.invoke('swarm_read_state', { dir: '/Users/apollo/Documents/athenas-core' })
      } catch (e) {
        return `ERROR: ${JSON.stringify(e)}`
      }
    })
    console.log('[DIAG] swarm_read_state:', typeof stateRaw === 'string' ? stateRaw.slice(0, 400) : stateRaw)
  })
})
