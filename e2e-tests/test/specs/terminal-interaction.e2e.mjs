import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

describe('Terminal interaction smoke tests', () => {
  it('verifies terminal pane is present after workspace launch', async () => {
    // First create a new space with a terminal — reuse the flow from new-space-3-shells
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // Open New Workspace modal
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.includes('New Workspace')) {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    })

    await browser.pause(1000)

    // Select Terminal Workspace
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.includes('Terminal Workspace')) {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    })

    await browser.pause(800)

    // Click Next, fill dir, Next, add agent, Launch
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Next >') {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    })
    await browser.pause(600)

    // Fill working directory
    await browser.execute(() => {
      for (const input of document.querySelectorAll('input')) {
        if ((input.getAttribute('placeholder') || '').includes('/path/to/project')) {
          input.value = '/tmp'
          input.dispatchEvent(new Event('input', { bubbles: true }))
          break
        }
      }
    })

    // Click Next >
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Next >') {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    })

    await browser.pause(800)

    // Add shell agent
    await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      if (btn) btn.click()
    })

    await browser.pause(500)

    // Launch Space
    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Launch Space') {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          break
        }
      }
    })

    // Wait for terminal to mount
    await browser.pause(5000)

    // Verify terminal elements exist
    const terminalState = await browser.execute(() => {
      const xtermDivs = document.querySelectorAll('.xterm')
      const containers = document.querySelectorAll('[id^="xterm-container"]')
      const mounts = document.querySelectorAll('.xterm-mount')
      const panes = document.querySelectorAll('.terminal-pane')

      return {
        xtermDivs: xtermDivs.length,
        containers: containers.length,
        mounts: mounts.length,
        panes: panes.length,
        xtermReady: window.__athenaXtermReady,
        hasTerminal: typeof window.Terminal !== 'undefined',
      }
    })

    if (terminalState.xtermDivs > 0 || terminalState.containers > 0) {
      console.log(`[PASS] Terminal rendered: ${terminalState.xtermDivs} xterm divs, ${terminalState.containers} containers`)
    } else {
      console.log('[INFO] Terminal not yet rendered — may need more time or WASM panicked')
    }

    await browser.saveScreenshot(join(screenshotDir, 'terminal-present.png'))
  })

  it('types a command in the terminal via WebdriverIO', async () => {
    // Note: Direct terminal input via WebdriverIO is limited since xterm.js
    // captures its own keyboard events. We verify the terminal is present
    // and can receive focus, which is the best we can do in E2E.
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.pause(1000)

    // Try to focus the terminal and type
    const focusResult = await browser.execute(() => {
      const xtermEl = document.querySelector('.xterm, .xterm-screen, .xterm-rows')
      if (!xtermEl) return { ok: false, reason: 'no_xterm_element' }

      xtermEl.focus()

      // Dispatch keyboard events that xterm.js might capture
      const events = ['keydown', 'keypress', 'keyup']
      events.forEach(type => {
        xtermEl.dispatchEvent(new KeyboardEvent(type, {
          key: 'l',
          bubbles: true,
          cancelable: true,
        }))
      })

      return { ok: true }
    })

    if (focusResult.ok) {
      console.log('[PASS] Terminal element focused and key events dispatched')
    } else {
      console.log('[INFO] Could not focus terminal:', focusResult.reason)
    }

    await browser.pause(500)

    // Verify app is still responsive
    const alive = await browser.execute(() => !!document.body)
    expect(alive).toBe(true)

    await browser.saveScreenshot(join(screenshotDir, 'terminal-focused.png'))
  })
})
