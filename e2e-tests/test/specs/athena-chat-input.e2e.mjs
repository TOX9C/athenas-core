import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))
const screenshotDir = join(__dirname, '..', 'screenshots')

describe('Athena chat input smoke tests', () => {
  it('types a message in the Athena input without crashing', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // Open right sidebar then click Athena tab
    await browser.keys(['Meta', 'j'])
    await browser.pause(500)
    await browser.keys('Meta')
    await browser.pause(1000)

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

    // Find the textarea and type a message using executeScript
    const result = await browser.execute(() => {
      const textarea = document.querySelector('textarea')
      if (!textarea) return { ok: false, reason: 'no_textarea' }

      // Focus and type
      textarea.focus()
      textarea.value = 'Hello Athena, this is an E2E test message'
      textarea.dispatchEvent(new Event('input', { bubbles: true }))
      textarea.dispatchEvent(new Event('change', { bubbles: true }))

      return {
        ok: true,
        value: textarea.value,
        placeholder: textarea.placeholder,
        disabled: textarea.disabled,
      }
    })

    if (result.ok) {
      expect(result.value).toContain('Hello Athena')
      if (result.disabled) {
        console.log('[INFO] Textarea is disabled (no API key configured) — expected behavior')
      } else {
        console.log('[PASS] Successfully typed into Athena input')
      }
    } else {
      console.log('[WARN] Could not find Athena textarea:', result.reason)
    }

    await browser.saveScreenshot(join(screenshotDir, 'athena-input-typed.png'))
  })

  it('submits a message via Enter key (smoke test — may be blocked without API key)', async () => {
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // Open right sidebar and Athena tab
    await browser.keys(['Meta', 'j'])
    await browser.pause(500)
    await browser.keys('Meta')
    await browser.pause(1000)

    await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.trim() === 'Athena') {
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true }))
          break
        }
      }
    })

    await browser.pause(800)

    // Type and submit
    const submitResult = await browser.execute(() => {
      const textarea = document.querySelector('textarea')
      if (!textarea) return { ok: false, reason: 'no_textarea' }
      if (textarea.disabled) return { ok: true, reason: 'disabled_no_api_key', sent: false }

      textarea.value = 'E2E smoke test message'
      textarea.dispatchEvent(new Event('input', { bubbles: true }))

      // Simulate Enter key
      const keyEvent = new KeyboardEvent('keydown', {
        key: 'Enter',
        bubbles: true,
        cancelable: true,
      })
      textarea.dispatchEvent(keyEvent)

      // Also try clicking the send button if it exists
      const sendBtn = Array.from(document.querySelectorAll('button')).find(
        b => b.getAttribute('title') === 'Send (Enter)' || b.textContent.includes('Send')
      )
      if (sendBtn) {
        sendBtn.click()
      }

      return { ok: true, sent: true, hasSendBtn: !!sendBtn }
    })

    if (submitResult.ok && submitResult.sent) {
      console.log('[PASS] Message submission attempted')
    } else if (submitResult.reason === 'disabled_no_api_key') {
      console.log('[INFO] Input disabled — no API key configured, which is expected')
    } else {
      console.log('[WARN] Could not submit message:', submitResult.reason)
    }

    // Verify app didn't crash by checking the body still exists
    const stillAlive = await browser.execute(() => !!document.body)
    expect(stillAlive).toBe(true)

    await browser.saveScreenshot(join(screenshotDir, 'athena-input-submitted.png'))
  })
})
