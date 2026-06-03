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
                return 'clicked';
            }
        }
        return 'no btn';
    });

    // Wait longer for xterm to load
    await browser.pause(8000);

    // Check multiple states
    for (let i = 0; i < 5; i++) {
        await browser.pause(2000);

        const state = await browser.execute(() => {
            return {
                xtermReady: window.__athenaXtermReady,
                bootstrapping: window.__athenaXtermBootstrapping,
                terminalCount: window.__athenaTerminals ? window.__athenaTerminals.size : 0,
                hasTerminal: typeof window.Terminal !== 'undefined',
                hasFitAddon: typeof window.FitAddon !== 'undefined',
            };
        });

        console.log(`Check ${i+1}:`, JSON.stringify(state));

        if (state.xtermReady === true && state.hasTerminal) {
            console.log('✅ xterm.js is fully loaded and ready!');
            break;
        }
    }

} finally {
    await browser.deleteSession();
}
