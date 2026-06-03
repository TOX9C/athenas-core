import { remote } from 'webdriverio'
import path from 'path'
import { fileURLToPath } from 'url'

const __dirname = path.dirname(fileURLToPath(import.meta.url))
const projectRoot = path.resolve(__dirname, '..')
const binaryPath = path.join(projectRoot, 'target', 'debug', 'athenas-core')

const browser = await remote({
    hostname: '127.0.0.1',
    port: 4444,
    capabilities: {
        browserName: 'safari',
        'tauri:options': { binary: binaryPath }
    }
})

try {
    await browser.pause(5000)
    const state1 = await browser.execute(() => {
        const buttons = Array.from(document.querySelectorAll('button'))
            .map(b => b.textContent.trim().slice(0, 60))
        return {
            buttonCount: buttons.length,
            buttons,
            bodyLen: document.body.innerHTML.length,
            hasModal: !!document.querySelector('[class*="modal"], [class*="Modal"]'),
            hasGrid: !!document.querySelector('[style*="display: grid"]'),
            title: document.title,
        }
    })
    console.log('STATE 1 (after 5s):', JSON.stringify(state1, null, 2))

    // Try real .click() instead of dispatchEvent
    const clickResult = await browser.execute(() => {
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.includes('New Workspace')) {
                try {
                    btn.click()
                    return { ok: true, text: btn.textContent.trim() }
                } catch (e) {
                    return { ok: false, error: String(e) }
                }
            }
        }
        return { ok: false, error: 'no matching button' }
    })
    console.log('CLICK:', JSON.stringify(clickResult))

    await browser.pause(2000)

    const state2 = await browser.execute(() => {
        const buttons = Array.from(document.querySelectorAll('button'))
            .map(b => b.textContent.trim().slice(0, 60))
        return {
            buttonCount: buttons.length,
            buttons,
            hasModal: !!document.querySelector('[class*="modal"], [class*="Modal"]'),
            hasNewSpaceTrigger: !!document.querySelector('[data-new-space-trigger]'),
        }
    })
    console.log('STATE 2 (after click + 2s):', JSON.stringify(state2, null, 2))

} finally {
    await browser.deleteSession()
}
