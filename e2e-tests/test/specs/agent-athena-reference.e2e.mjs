import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

import { e2eTrustRoot, seedTrustedRoots, setE2E, fillSpaceDir, trustViaProjectRoot } from '../helpers/trust.mjs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

async function clickButton(text, partial = false) {
  return browser.execute(
    ({ text, partial }) => {
      for (const button of document.querySelectorAll('button')) {
        const content = (button.textContent || '').trim()
        if ((partial && content.includes(text)) || (!partial && content === text)) {
          button.click()
          return true
        }
      }
      return false
    },
    { text, partial },
  )
}

// The 'New Workspace' entry point depends on app state this spec must not
// depend on (store.json is shared by every serial spec): with persisted
// spaces the empty-state button is gone and the only trigger is the
// WorkspaceTabs icon button, which has aria-label but no text. Match either
// and wait for it to render instead of clicking blind.
async function openWorkspaceModal() {
  return browser.waitUntil(
    () =>
      browser.execute(() => {
        for (const el of document.querySelectorAll('[aria-label="New Workspace"]')) {
          el.click()
          return true
        }
        for (const button of document.querySelectorAll('button')) {
          if ((button.textContent || '').includes('New Workspace')) {
            button.click()
            return true
          }
        }
        return false
      }),
    { timeout: 15000, interval: 250, timeoutMsg: 'no New Workspace entry point rendered' },
  )
}

describe('Athena agent references', () => {
  it('accepts an agent drop through the closed-sidebar fallback and deduplicates repeats', async function () {
    this.timeout(90000)

    await setE2E()
    // A permitted dir (the backend rejects /tmp); seeded so the add is a
    // no-op that skips the blocking confirmation dialog. `.catch` keeps a
    // rejection from hanging the sync browser script.
    const trustDir = e2eTrustRoot('agent-athena-reference')
    await seedTrustedRoots(browser, [trustDir])
    await trustViaProjectRoot(browser)

    await openWorkspaceModal()
    await browser.waitUntil(
      async () => browser.execute(() => document.body.textContent.includes('Terminal Workspace')),
      { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
    )
    expect(await clickButton('Terminal Workspace', true)).toBe(true)
    await fillSpaceDir(browser, trustDir)
    expect(await clickButton('Next >')).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.getElementById('add-claude')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Agent configuration step did not appear' },
    )
    await browser.execute(() => document.getElementById('add-claude')?.click())
    expect(await clickButton('Launch Space')).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('[data-agent-pill="true"]')),
      { timeout: 25000, interval: 500, timeoutMsg: 'Claude agent pane pill did not mount' },
    )

    // Open Athena once to clear persisted context, then close it so this test
    // exercises the temporary fallback target and automatic panel activation.
    await browser.execute(() => document.querySelector('[data-athena-toggle]')?.click())
    await browser.waitUntil(
      async () => browser.execute(() => [...document.querySelectorAll('button')].some(button => (button.textContent || '').trim() === 'Athena')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Right sidebar did not mount' },
    )
    expect(await clickButton('Athena')).toBe(true)
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.athena-panel[data-athena-drop="true"]')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Athena drop target did not mount' },
    )
    await browser.execute(() => document.querySelector('.athena-panel button[title="Clear context"]')?.click())
    await browser.execute(() => document.querySelector('[data-athena-toggle]')?.click())
    await browser.waitUntil(
      async () => browser.execute(() => !document.querySelector('.athena-panel[data-athena-drop="true"]')),
      { timeout: 5000, interval: 250, timeoutMsg: 'Athena sidebar did not close' },
    )

    const source = await browser.execute(() => {
      const pill = document.querySelector('[data-agent-pill="true"]')
      if (!pill) return { ok: false, reason: 'missing source pill' }
      const rect = pill.getBoundingClientRect()
      pill.dispatchEvent(new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        pointerId: 41,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }))
      return {
        ok: true,
        sourceLabel: pill.getAttribute('data-agent-label') || '',
        sourcePaneId: pill.getAttribute('data-agent-pane-id') || '',
      }
    })
    expect(source.ok).toBe(true)
    expect(source.sourcePaneId).not.toBe('')
    expect(source.sourceLabel).not.toBe('')

    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.athena-dnd-fallback[data-athena-drop="true"]')),
      { timeout: 5000, interval: 100, timeoutMsg: 'Closed-sidebar Athena fallback did not mount' },
    )

    const fallbackPoint = await browser.execute(() => {
      const target = document.querySelector('.athena-dnd-fallback[data-athena-drop="true"]')
      if (!target) return null
      const rect = target.getBoundingClientRect()
      return { x: rect.left + rect.width / 2, y: rect.top + rect.height / 2 }
    })
    expect(fallbackPoint).not.toBe(null)

    const moveResult = await browser.execute(({ x, y }) => {
      const overlay = document.querySelector('.dnd-overlay')
      if (!overlay) return { ok: false, reason: 'drag overlay did not mount' }
      overlay.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 41,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: x,
        clientY: y,
      }))
      return { ok: true }
    }, fallbackPoint)
    expect(moveResult.ok).toBe(true)
    await browser.pause(100)

    const highlighted = await browser.execute(() =>
      !!document.querySelector('.athena-dnd-fallback[data-athena-drop="true"].is-dnd-target'),
    )
    expect(highlighted).toBe(true)

    const upResult = await browser.execute(({ x, y }) => {
      const overlay = document.querySelector('.dnd-overlay')
      if (!overlay) return false
      overlay.dispatchEvent(new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId: 41,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: x,
        clientY: y,
      }))
      return true
    }, fallbackPoint)
    expect(upResult).toBe(true)

    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.athena-panel[data-athena-drop="true"] [data-agent-pane-id]')),
      { timeout: 5000, interval: 250, timeoutMsg: 'Athena did not auto-open with pinned context' },
    )

    const firstContext = await browser.execute(() => ({
      text: document.querySelector('.athena-panel')?.textContent || '',
      referencedPaneId: document.querySelector('.athena-panel [data-agent-pane-id]')?.getAttribute('data-agent-pane-id') || '',
      referenceCount: document.querySelectorAll('.athena-panel [data-agent-pane-id]').length,
    }))
    expect(firstContext.text).toContain(`Agent: ${source.sourceLabel}`)
    expect(firstContext.referencedPaneId).toBe(source.sourcePaneId)
    expect(firstContext.referenceCount).toBe(1)

    // Repeat the same drag against the now-open Athena panel. The store should
    // reject the duplicate by pane ID rather than rendering a second chip.
    const duplicatePoint = await browser.execute(() => {
      const pill = document.querySelector('[data-agent-pill="true"]')
      const target = document.querySelector('.athena-panel[data-athena-drop="true"]')
      if (!pill || !target) return null
      const sourceRect = pill.getBoundingClientRect()
      const targetRect = target.getBoundingClientRect()
      pill.dispatchEvent(new PointerEvent('pointerdown', {
        bubbles: true,
        cancelable: true,
        pointerId: 42,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: sourceRect.left + sourceRect.width / 2,
        clientY: sourceRect.top + sourceRect.height / 2,
      }))
      return { x: targetRect.left + targetRect.width / 2, y: targetRect.top + targetRect.height / 2 }
    })
    expect(duplicatePoint).not.toBe(null)
    await browser.pause(100)
    const duplicateMove = await browser.execute(({ x, y }) => {
      const overlay = document.querySelector('.dnd-overlay')
      if (!overlay) return false
      overlay.dispatchEvent(new PointerEvent('pointermove', {
        bubbles: true,
        cancelable: true,
        pointerId: 42,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: x,
        clientY: y,
      }))
      return true
    }, duplicatePoint)
    expect(duplicateMove).toBe(true)
    await browser.pause(100)
    const duplicateUp = await browser.execute(({ x, y }) => {
      const overlay = document.querySelector('.dnd-overlay')
      if (!overlay) return false
      overlay.dispatchEvent(new PointerEvent('pointerup', {
        bubbles: true,
        cancelable: true,
        pointerId: 42,
        pointerType: 'mouse',
        isPrimary: true,
        button: 0,
        clientX: x,
        clientY: y,
      }))
      return true
    }, duplicatePoint)
    expect(duplicateUp).toBe(true)
    await browser.pause(250)

    const finalCount = await browser.execute(() => document.querySelectorAll('.athena-panel [data-agent-pane-id]').length)
    expect(finalCount).toBe(1)
    await browser.saveScreenshot(join(screenshotDir, 'agent-athena-reference.png'))
  })
})
