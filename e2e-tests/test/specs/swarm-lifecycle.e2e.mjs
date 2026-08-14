async function clickButton(text, partial = false) {
  return browser.execute(({ text, partial }) => {
    for (const btn of document.querySelectorAll('button')) {
      const content = (btn.textContent || '').trim()
      if ((partial ? content.includes(text) : content === text)) {
        btn.click()
        return { ok: true, content }
      }
    }
    return { ok: false, text }
  }, { text, partial })
}

async function setField(selector, value) {
  return browser.execute(({ selector, value }) => {
    const field = document.querySelector(selector)
    if (!field) return false
    const setter = Object.getOwnPropertyDescriptor(field.constructor.prototype, 'value')?.set
    setter?.call(field, value)
    field.dispatchEvent(new Event('input', { bubbles: true }))
    field.dispatchEvent(new Event('change', { bubbles: true }))
    return true
  }, { selector, value })
}

describe('Swarm lifecycle', () => {
  it('launches a persisted swarm mission and renders its board', async () => {
    await browser.execute(() => { window.__athenaE2E = true })

    expect((await clickButton('New Workspace', true)).ok).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => document.body.textContent.includes('Swarm Mission')),
      { timeout: 10000, interval: 250, timeoutMsg: 'workspace modal did not open' },
    )

    expect((await clickButton('Swarm Mission', true)).ok).toBe(true)
    expect(await setField('input[placeholder="/path/to/project"]', '/tmp')).toBe(true)
    expect(await setField('textarea[placeholder="Describe what the swarm should accomplish..."]', 'Validate the swarm lifecycle')).toBe(true)
    expect((await clickButton('Next >')).ok).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => document.body.textContent.includes('Team (3 agents)')),
      { timeout: 10000, interval: 250, timeoutMsg: 'swarm team step did not appear' },
    )

    expect((await clickButton('Launch Swarm')).ok).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => {
        const body = document.body.textContent || ''
        return body.includes('Swarm') && body.includes('active') && body.toLowerCase().includes('coordinator')
      }),
      { timeout: 15000, interval: 500, timeoutMsg: 'swarm board did not render after launch' },
    )

    const persisted = await browser.execute(async () => {
      const raw = await window.__TAURI__.core.invoke('swarm_read_state', { dir: '/tmp' })
      return JSON.parse(raw)
    })
    expect(persisted.workspaceDir).toBe('/tmp')
    expect(persisted.goal).toBe('Validate the swarm lifecycle')
    expect(persisted.status).toBe('active')
    expect(persisted.agents).toHaveLength(3)
    expect(persisted.agents.filter((agent) => agent.role === 'coordinator')).toHaveLength(1)
    expect(persisted.agents.filter((agent) => agent.role === 'builder')).toHaveLength(2)

    const boardState = await browser.execute(() => ({
      hasBoard: !!document.querySelector('.swarm-board'),
      hasAgents: document.querySelectorAll('.agent-card').length,
      hasPause: Array.from(document.querySelectorAll('button')).some((b) => (b.textContent || '').includes('Pause')),
      hasTasks: (document.body.textContent || '').includes('Tasks'),
    }))
    expect(boardState.hasBoard).toBe(true)
    expect(boardState.hasAgents).toBe(3)
    expect(boardState.hasPause).toBe(true)
    expect(boardState.hasTasks).toBe(true)
  })
})
