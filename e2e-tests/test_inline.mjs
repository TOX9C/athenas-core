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

    await browser.pause(3000);

    // Check state
    const result = await browser.execute(() => {
        return {
            xtermReady: window.__athenaXtermReady,
            hasTerminal: typeof window.Terminal !== 'undefined',
            hasFitAddon: typeof window.FitAddon !== 'undefined',
            athenaKeys: Object.keys(window).filter(k => k.startsWith('__athena')),
            terminalCount: window.__athenaTerminals ? window.__athenaTerminals.size : 0,
        };
    });

    console.log('STATE:', JSON.stringify(result, null, 2));

    if (result.xtermReady === true && result.hasTerminal) {
        console.log('✅ SUCCESS: xterm.js is fully inlined and ready!');
    } else {
        console.log('❌ FAIL: xterm.js not ready');
    }

} finally {
    await browser.deleteSession();
}
