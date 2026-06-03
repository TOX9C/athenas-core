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
        'tauri:options': {
            binary: binaryPath
        }
    }
})

let failed = false
const fail = (msg, ctx) => {
    failed = true
    console.error('❌', msg)
    if (ctx !== undefined) console.error('   ctx:', JSON.stringify(ctx, null, 2))
}

async function openTerminalWorkspaceStep1() {
    await browser.pause(5000)

    const opened = await browser.execute(() => {
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.includes('New Workspace')) {
                btn.click()
                break
            }
        }
        for (const btn of document.querySelectorAll('button')) {
            if (btn.textContent.includes('Terminal Workspace') && !btn.textContent.includes('Launch')) {
                btn.click()
                return { ok: true }
            }
        }
        return { ok: false }
    })

    if (!opened.ok) {
        fail('Could not reach Terminal Workspace Step 1')
        return false
    }

    await browser.pause(1200)
    return true
}

try {
    const ready = await openTerminalWorkspaceStep1()
    if (!ready) {
        throw new Error('setup failed')
    }

    // Step 1: Next is blocked when Working Directory is empty.
    const step1Before = await browser.execute(() => {
        const nextBtn = Array.from(document.querySelectorAll('button'))
            .find(btn => btn.textContent.trim() === 'Next >')
        if (!nextBtn) return { ok: false, reason: 'next_missing' }

        const style = nextBtn.getAttribute('style') || ''
        const beforeStep1Visible = !!Array.from(document.querySelectorAll('label'))
            .find(l => l.textContent.includes('Working Directory'))
        nextBtn.click()
        const afterStep1Visible = !!Array.from(document.querySelectorAll('label'))
            .find(l => l.textContent.includes('Working Directory'))
        const addShellExists = !!document.getElementById('add-shell')

        return {
            ok: true,
            blockedAffordance: style.includes('not-allowed') || style.includes('var(--bgTertiary)'),
            stayedOnStep1: beforeStep1Visible && afterStep1Visible && !addShellExists,
            style
        }
    })

    if (!step1Before.ok) {
        fail('Step 1: "Next >" button missing', step1Before)
    } else {
        if (!step1Before.blockedAffordance) {
            fail('Step 1: "Next >" did not present blocked affordance while empty', step1Before)
        }
        if (!step1Before.stayedOnStep1) {
            fail('Step 1: "Next >" advanced despite empty required field', step1Before)
        } else {
            console.log('✅ Step 1 empty state keeps Next blocked')
        }
    }

    // Fill valid Working Directory and verify Next becomes enabled + advances.
    const step1After = await browser.execute(() => {
        const dirInput = Array.from(document.querySelectorAll('input'))
            .find(i => (i.getAttribute('placeholder') || '').includes('/path/to/project'))
        const nextBtn = Array.from(document.querySelectorAll('button'))
            .find(btn => btn.textContent.trim() === 'Next >')
        if (!dirInput || !nextBtn) return { ok: false, reason: 'missing_input_or_next' }

        dirInput.value = '/tmp'
        dirInput.dispatchEvent(new Event('input', { bubbles: true }))
        dirInput.dispatchEvent(new Event('change', { bubbles: true }))

        const styleAfterInput = nextBtn.getAttribute('style') || ''
        nextBtn.click()

        const addShellExists = !!document.getElementById('add-shell')
        const stillStep1 = !!Array.from(document.querySelectorAll('label'))
            .find(l => l.textContent.includes('Working Directory'))

        return {
            ok: true,
            enabledAffordance: !styleAfterInput.includes('not-allowed'),
            advancedToStep2: addShellExists && !stillStep1,
            styleAfterInput
        }
    })

    if (!step1After.ok) {
        fail('Step 1: Could not set Working Directory or find Next', step1After)
    } else {
        if (!step1After.enabledAffordance) {
            fail('Step 1: "Next >" remained blocked after valid Working Directory', step1After)
        }
        if (!step1After.advancedToStep2) {
            fail('Step 1: "Next >" did not advance after valid Working Directory', step1After)
        } else {
            console.log('✅ Step 1 valid Working Directory enables Next and advances')
        }
    }

    await browser.pause(800)

    // Step 2 Terminal: Launch Space is blocked with total agents = 0.
    const step2Before = await browser.execute(() => {
        const launchBtn = Array.from(document.querySelectorAll('button'))
            .find(btn => btn.textContent.trim() === 'Launch Space')
        if (!launchBtn) return { ok: false, reason: 'launch_missing' }

        const style = launchBtn.getAttribute('style') || ''
        const hasZeroSummary = !!Array.from(document.querySelectorAll('label, div, span'))
            .find(el => (el.textContent || '').includes('Agents (0/16)'))
        launchBtn.click()
        const stillHasAddShell = !!document.getElementById('add-shell')

        return {
            ok: true,
            blockedAffordance: style.includes('not-allowed') || style.includes('var(--bgTertiary)'),
            remainedOnStep2: stillHasAddShell,
            hasZeroSummary,
            style
        }
    })

    if (!step2Before.ok) {
        fail('Step 2: "Launch Space" button missing', step2Before)
    } else {
        if (!step2Before.blockedAffordance) {
            fail('Step 2: "Launch Space" did not present blocked affordance at 0 agents', step2Before)
        }
        if (!step2Before.remainedOnStep2) {
            fail('Step 2: "Launch Space" should remain blocked when 0 agents', step2Before)
        }
        if (!step2Before.hasZeroSummary) {
            fail('Step 2: expected to observe Agents (0/16) summary', step2Before)
        } else {
            console.log('✅ Step 2 with 0 agents keeps Launch Space blocked')
        }
    }

    // Add one agent, then Launch Space becomes enabled.
    const step2After = await browser.execute(() => {
        const plusBtn = document.getElementById('add-shell')
        const launchBtn = Array.from(document.querySelectorAll('button'))
            .find(btn => btn.textContent.trim() === 'Launch Space')
        if (!plusBtn || !launchBtn) return { ok: false, reason: 'missing_plus_or_launch' }

        plusBtn.click()

        const styleAfter = launchBtn.getAttribute('style') || ''
        const hasOneSummary = !!Array.from(document.querySelectorAll('label, div, span'))
            .find(el => (el.textContent || '').includes('Agents (1/16)'))

        return {
            ok: true,
            enabledAffordance: !styleAfter.includes('not-allowed'),
            hasOneSummary,
            styleAfter
        }
    })

    if (!step2After.ok) {
        fail('Step 2: Could not increment agent or read launch button', step2After)
    } else {
        if (!step2After.enabledAffordance) {
            fail('Step 2: "Launch Space" stayed blocked after incrementing an agent', step2After)
        }
        if (!step2After.hasOneSummary) {
            fail('Step 2: expected to observe Agents (1/16) after increment', step2After)
        } else {
            console.log('✅ Step 2 increment enables Launch Space')
        }
    }
} finally {
    await browser.deleteSession()
}

if (failed) {
    console.error('\n❌ test_new_workspace_terminal_modal_states FAILED')
    process.exit(1)
} else {
    console.log('\n✅ test_new_workspace_terminal_modal_states PASSED')
}
