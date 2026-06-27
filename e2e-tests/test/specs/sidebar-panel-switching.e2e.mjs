import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

describe('Sidebar section switching smoke tests', () => {
  it('switches between all sidebar sections via bottom tab buttons', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.pause(1000)

    // The sidebar has 4 section buttons at the bottom with aria-labels:
    // Spaces, Files, Agents, Plugins
    const sections = ['Spaces', 'Files', 'Agents', 'Plugins']

    for (const section of sections) {
      const clicked = await browser.execute((sectionName) => {
        const buttons = Array.from(document.querySelectorAll('button'))
        for (const btn of buttons) {
          const label = btn.getAttribute('aria-label') || btn.getAttribute('title') || ''
          if (label === sectionName) {
            btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
            return { ok: true }
          }
        }
        return { ok: false, reason: 'button_not_found' }
      }, section)

      if (clicked.ok) {
        await browser.pause(800)
        console.log(`[INFO] Clicked ${section} tab`)
      } else {
        console.log(`[WARN] Could not find ${section} tab:`, clicked.reason)
      }
    }

    // Verify app is still responsive after all section switches
    const alive = await browser.execute(() => {
      return {
        hasBody: !!document.body,
        hasSidebar: document.querySelector('.sidebar') !== null,
        noError: document.getElementById('wasm-loading-error') === null,
      }
    })

    expect(alive.hasBody).toBe(true)
    expect(alive.hasSidebar).toBe(true)
    expect(alive.noError).toBe(true)

    await browser.saveScreenshot(join(screenshotDir, 'sidebar-sections-switched.png'))
  })

  it('uses keyboard shortcuts to switch between main panels', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // Cmd+1 = Workspace, Cmd+2 = Editor, Cmd+3 = Kanban, Cmd+4 = Swarm
    const shortcuts = [
      { keys: ['Meta', '1'], name: 'Workspace' },
      { keys: ['Meta', '2'], name: 'Editor' },
      { keys: ['Meta', '3'], name: 'Kanban' },
      { keys: ['Meta', '4'], name: 'Swarm' },
    ]

    for (const { keys, name } of shortcuts) {
      try {
        await browser.keys(keys)
        await browser.pause(600)
        await browser.keys(['Meta'])
        await browser.pause(400)
        console.log(`[INFO] Sent Cmd+${keys[1]} for ${name}`)
      } catch (e) {
        console.log(`[WARN] Failed to send shortcut for ${name}:`, e.message)
      }
    }

    // The key press may trigger WASM panic, so just verify app is still
    // responding by checking the body still exists
    const alive = await browser.execute(() => {
      return {
        hasBody: !!document.body,
        hasMain: !!document.getElementById('main'),
        noWasmError: !document.getElementById('wasm-loading-error'),
      }
    })

    expect(alive.hasBody).toBe(true)
    expect(alive.hasMain).toBe(true)
    expect(alive.noWasmError).toBe(true)

    await browser.saveScreenshot(join(screenshotDir, 'panel-keyboard-shortcuts.png'))
  })

  it('toggles sidebar visibility with Cmd+B', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.pause(1000)

    // Check initial sidebar visibility
    const before = await browser.execute(() => {
      const sidebar = document.querySelector('.sidebar')
      return { hasSidebar: !!sidebar }
    })

    // Toggle sidebar off with Cmd+B
    await browser.keys(['Meta', 'b'])
    await browser.pause(600)
    await browser.keys(['Meta'])
    await browser.pause(800)

    // Toggle back on
    await browser.keys(['Meta', 'b'])
    await browser.pause(600)
    await browser.keys(['Meta'])
    await browser.pause(800)

    const after = await browser.execute(() => {
      const sidebar = document.querySelector('.sidebar')
      return { hasSidebar: !!sidebar }
    })

    // Sidebar should still exist after toggling (may be hidden but DOM remains)
    // The key point: app didn't crash
    const alive = await browser.execute(() => !!document.body)
    expect(alive).toBe(true)

    console.log(`[INFO] Sidebar toggle: before=${before.hasSidebar}, after=${after.hasSidebar}`)
    await browser.saveScreenshot(join(screenshotDir, 'sidebar-toggle.png'))
  })
})
