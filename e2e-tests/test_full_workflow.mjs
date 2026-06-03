import { remote } from 'webdriverio'

const browser = await remote({
    hostname: '127.0.0.1',
    port: 4444,
    capabilities: {
        browserName: 'safari',
        'tauri:options': {
            binary: '/Users/apollo/Documents/athenas-core/target/debug/athenas-core'
        }
    }
})

function wait(ms) {
    return new Promise(resolve => setTimeout(resolve, ms));
}

try {
    // Wait for app to fully render
    await wait(3000);

    // Set E2E flag to bypass modal validation
    await browser.execute(() => {
        window.__athenaE2E = true;
        return 'E2E flag set';
    });

    await wait(1000);

    // Step 1: Click New Workspace
    await browser.execute(() => {
        const btns = document.querySelectorAll('button');
        for (const btn of btns) {
            if (btn.textContent.includes('New Workspace') && !btn.id) {
                btn.click();
                return 'clicked New Workspace via native click';
            }
        }
        return 'no btn found';
    });

    await wait(1500);

    // Step 2: Click Terminal Workspace mode
    await browser.execute(() => {
        const btns = document.querySelectorAll('button');
        for (const btn of btns) {
            if (btn.textContent.includes('Terminal Workspace')) {
                btn.click();
                return 'clicked Terminal Workspace';
            }
        }
        return 'no mode btn';
    });

    await wait(1500);

    // Step 3: Click Next
    await browser.execute(() => {
        const btns = document.querySelectorAll('button');
        for (const btn of btns) {
            if (btn.textContent.includes('Next')) {
                btn.click();
                return 'clicked Next';
            }
        }
        return 'no next btn';
    });

    await wait(1500);

    // Step 4: Click Shell + button using native click
    await browser.execute(() => {
        window.__athenaClickFired = false;
        window.__athenaClickDone = false;
        const btn = document.querySelector('button#add-shell');
        if (!btn) return { result: 'no add-shell button found' };
        btn.click();
        return { result: 'clicked add-shell via btn.click()' };
    });

    await wait(2000);

    // Check if the handler fired and count changed
    const state = await browser.execute(() => {
        const spans = Array.from(document.querySelectorAll('span')).map(s => s.textContent.trim()).filter(t => t.match(/^[0-9]+$/));
        return {
            numbers: spans,
            clickFired: !!window.__athenaClickFired,
            clickDone: !!window.__athenaClickDone,
            paneAgents: window.__athenaPaneAgents || null,
        };
    });

    console.log('After + click:', JSON.stringify(state, null, 2));

    if (!state.clickFired) {
        console.log('WARNING: onclick handler did NOT fire (clickFired is false)');
    }
    if (!state.clickDone) {
        console.log('WARNING: onclick handler did NOT complete (clickDone is false)');
    }

    // Step 5: Click Launch Space
    await browser.execute(() => {
        const btns = document.querySelectorAll('button');
        for (const btn of btns) {
            if (btn.textContent.includes('Launch Space')) {
                btn.click();
                return 'clicked Launch Space';
            }
        }
        return 'no launch btn';
    });

    await wait(8000);

    // Check final state
    const final = await browser.execute(() => {
        return {
            xtermReady: window.__athenaXtermReady,
            hasTerminal: typeof window.Terminal !== 'undefined',
            hasFitAddon: typeof window.FitAddon !== 'undefined',
            containers: document.querySelectorAll('[id^="xterm-container"]').length,
            panes: document.querySelectorAll('.terminal-pane').length,
            xtermDivs: document.querySelectorAll('.xterm').length,
            terminalCount: window.__athenaTerminals ? window.__athenaTerminals.size : 0,
        };
    });

    console.log('FINAL STATE:', JSON.stringify(final, null, 2));

    if (final.xtermReady && final.hasTerminal && (final.containers > 0 || final.xtermDivs > 0)) {
        console.log('Terminal is working!');
    } else if (final.xtermReady && final.hasTerminal) {
        console.log('xterm.js loaded but DOM elements not found');
    } else {
        console.log('xterm.js not ready');
    }

} finally {
    await browser.deleteSession();
}
