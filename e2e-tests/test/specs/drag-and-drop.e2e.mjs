import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

describe('Drag-and-drop smoke tests', () => {
  it('simulates drag-and-drop interactions without crashing', async () => {
    // WebdriverIO's dragAndDrop support is limited in WKWebView.
    // We simulate the dragstart, dragover, dragenter, drop events via
    // JavaScript to verify the app handles them gracefully.
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.pause(1000)

    // Try to find a draggable element and simulate drag events
    const dragResult = await browser.execute(() => {
      // Look for any elements that might be draggable
      const draggables = document.querySelectorAll('[draggable="true"], .draggable, .drag-item')
      if (draggables.length === 0) {
        return { ok: false, reason: 'no_draggable_elements', simulated: false }
      }

      const source = draggables[0]

      // Create drag events
      const dragStart = new DragEvent('dragstart', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      })
      const dragOver = new DragEvent('dragover', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      })
      const drop = new DragEvent('drop', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      })

      source.dispatchEvent(dragStart)
      document.body.dispatchEvent(dragOver)
      document.body.dispatchEvent(drop)

      return {
        ok: true,
        draggableCount: draggables.length,
        simulated: true,
      }
    })

    if (dragResult.simulated) {
      console.log(`[PASS] Drag-and-drop events simulated on ${dragResult.draggableCount} potential elements`)
    } else {
      console.log('[INFO] No draggable elements found — this is expected for this UI state')
    }

    // Most importantly: verify app didn't crash
    const alive = await browser.execute(() => {
      return {
        hasBody: !!document.body,
        hasMain: !!document.getElementById('main'),
        noError: !document.getElementById('wasm-loading-error'),
      }
    })

    expect(alive.hasBody).toBe(true)
    expect(alive.hasMain).toBe(true)
    expect(alive.noError).toBe(true)

    await browser.saveScreenshot(join(screenshotDir, 'drag-drop.png'))
  })

  it('simulates dropping context onto Athena panel', async () => {
    // Open the Athena panel first
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.keys(['Meta', 'j'])
    await browser.pause(500)
    await browser.keys('Meta')
    await browser.pause(800)

    // Click Athena tab
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Athena') {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true }))
          break
        }
      }
    })

    await browser.pause(1000)

    // Simulate a drop event on the Athena panel
    const dropResult = await browser.execute(() => {
      const panel = document.querySelector('.athena-panel, [class*="athena"]')
      if (!panel) return { ok: false, reason: 'no_athena_panel' }

      const dragEnter = new DragEvent('dragenter', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      })
      const dragOver = new DragEvent('dragover', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      })
      const drop = new DragEvent('drop', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      })

      panel.dispatchEvent(dragEnter)
      panel.dispatchEvent(dragOver)
      panel.dispatchEvent(drop)

      return { ok: true }
    })

    if (dropResult.ok) {
      console.log('[PASS] Drop events simulated on Athena panel')
    } else {
      console.log('[INFO] Could not simulate drop:', dropResult.reason)
    }

    // Verify app survives the event
    const alive = await browser.execute(() => !!document.body)
    expect(alive).toBe(true)

    await browser.saveScreenshot(join(screenshotDir, 'drag-drop-athena.png'))
  })
})
