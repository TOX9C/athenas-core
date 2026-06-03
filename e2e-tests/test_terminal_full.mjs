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

try {
    await browser.pause(2000)

    // Click New Workspace
    await browser.execute(() => {
        const btns = document.querySelectorAll('button');
        for (const btn of btns) {
            if (btn.textContent.includes('New Workspace')) {
                btn.dispatchEvent(new MouseEvent('click', { bubbles: true }));
                return 'clicked workspace';
            }
        }
        return 'no btn';
    });

    await browser.pause(3000);

    // Check terminal state
    const state = await browser.execute(() => {
        return {
            xtermReady: window.__athenaXtermReady,
            terminalCount: window.__athenaTerminals ? window.__athenaTerminals.size : 'no map',
            terminalHandles: window.__athenaTerminals ? Array.from(window.__athenaTerminals.keys()) : [],
            hasTerminalPanes: document.querySelectorAll('.terminal-pane').length,
            hasTerminalContainers: document.querySelectorAll('[id^="xterm-container"]').length,
        };
    });

    console.log('TERMINAL STATE:', JSON.stringify(state, null, 2));

    if (state.xtermReady === true) {
        console.log('✅ xterm.js is bootstrapped and ready!');
    }
    if (state.hasTerminalPanes > 0 || state.hasTerminalContainers > 0) {
        console.log('✅ Terminal panes are rendered in DOM!');
    }
    if (state.terminalCount > 0) {
        console.log('✅ Terminal instances created:', state.terminalHandles);
    }

} finally {
    await browser.deleteSession();
}
