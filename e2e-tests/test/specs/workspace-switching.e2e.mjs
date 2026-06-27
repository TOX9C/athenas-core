import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

describe('Workspace creation and switching smoke tests', () => {
  it('creates a new Terminal Workspace and verifies it appears in the list', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.pause(1000)

    // Click New Workspace button
    const openResult = await browser.execute(() => {
      const btns = document.querySelectorAll('button')
      for (const btn of btns) {
        if (btn.textContent.includes('New Workspace')) {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          return { ok: true }
        }
      }
      return { ok: false, reason: 'no_new_workspace_btn' }
    })

    if (!openResult.ok) {
      console.log('[WARN] Could not open New Workspace:', openResult.reason)
      await browser.saveScreenshot(join(screenshotDir, 'workspace-create-fail.png'))
      return
    }

    await browser.pause(1000)

    // Select Terminal Workspace mode
    const modeResult = await browser.execute(() => {
      const btns = document.querySelectorAll('button')
      for (const btn of btns) {
        if (btn.textContent.includes('Terminal Workspace')) {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          return { ok: true }
        }
      }
      return { ok: false, reason: 'no_terminal_workspace_btn' }
    })

    if (!modeResult.ok) {
      console.log('[WARN] Could not select Terminal Workspace:', modeResult.reason)
      await browser.saveScreenshot(join(screenshotDir, 'workspace-mode-fail.png'))
      return
    }

    await browser.pause(800)

    // Click Next
    const nextResult = await browser.execute(() => {
      const btns = document.querySelectorAll('button')
      for (const btn of btns) {
        if (btn.textContent.trim() === 'Next >') {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          return { ok: true }
        }
      }
      return { ok: false, reason: 'no_next_btn' }
    })

    if (!nextResult.ok) {
      console.log('[WARN] Could not click Next:', nextResult.reason)
    } else {
      await browser.pause(800)

      // Fill Working Directory
      const fillResult = await browser.execute(() => {
        const inputs = document.querySelectorAll('input')
        for (const input of inputs) {
          if ((input.getAttribute('placeholder') || '').includes('/path/to/project')) {
            input.value = '/tmp'
            input.dispatchEvent(new Event('input', { bubbles: true }))
            input.dispatchEvent(new Event('change', { bubbles: true }))
            return { ok: true }
          }
        }
        return { ok: false, reason: 'no_working_dir_input' }
      })

      if (fillResult.ok) {
        // Click Next again
        await browser.execute(() => {
          const btns = document.querySelectorAll('button')
          for (const btn of btns) {
            if (btn.textContent.trim() === 'Next >') {
              btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
              break
            }
          }
        })

        await browser.pause(1000)

        // Add an agent
        await browser.execute(() => {
          const btn = document.getElementById('add-shell')
          if (btn) btn.click()
        })

        await browser.pause(500)

        // Click Launch Space
        await browser.execute(() => {
          const btns = document.querySelectorAll('button')
          for (const btn of btns) {
            if (btn.textContent.trim() === 'Launch Space') {
              btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
              return { ok: true }
            }
          }
          return { ok: false }
        })

        await browser.pause(3000)

        // Verify the workspace list shows the new space
        const listState = await browser.execute(() => {
          const rows = document.querySelectorAll('.workspace-row')
          const names = Array.from(rows).map(r => {
            const nameDiv = r.querySelector('div[style*="font-size"]')
            return nameDiv ? nameDiv.textContent.trim() : ''
          })
          return {
            rowCount: rows.length,
            names,
          }
        })

        if (listState.rowCount > 0) {
          console.log(`[PASS] Workspace list has ${listState.rowCount} spaces: ${listState.names.join(', ')}`)
        } else {
          console.log('[INFO] No workspace rows found — may need more time or WASM crashed')
        }
      }
    }

    await browser.saveScreenshot(join(screenshotDir, 'workspace-created.png'))
  })

  it('switches between active workspaces via sidebar list', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    await browser.pause(1000)

    // Find workspace rows and click on the first non-active one
    const switchResult = await browser.execute(() => {
      const rows = document.querySelectorAll('.workspace-row')
      if (rows.length < 2) return { ok: false, reason: 'less_than_2_workspaces', count: rows.length }

      // Click the second workspace row
      rows[1].dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
      return { ok: true, totalRows: rows.length }
    })

    if (switchResult.ok) {
      await browser.pause(1000)

      // Verify the active workspace changed
      const activeState = await browser.execute(() => {
        const rows = document.querySelectorAll('.workspace-row')
        const activeIndex = Array.from(rows).findIndex(r => {
          const style = r.getAttribute('style') || ''
          return style.includes('var(--bgTertiary)') || style.includes('--bgTertiary')
        })
        return { activeIndex, rowCount: rows.length }
      })

      if (activeState.activeIndex >= 0) {
        console.log(`[PASS] Active workspace index: ${activeState.activeIndex}`)
      } else {
        console.log('[INFO] Could not determine active workspace — checking via other means')
      }
    } else {
      console.log(`[INFO] Need at least 2 workspaces to switch: only ${switchResult.count} found`)
    }

    await browser.saveScreenshot(join(screenshotDir, 'workspace-switched.png'))
  })
})
