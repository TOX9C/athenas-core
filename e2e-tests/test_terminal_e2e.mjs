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

    // Wait for terminal to initialize
    await browser.pause(5000);

    // Check DOM for terminals
    const domState = await browser.execute(() => {
        const containers = document.querySelectorAll('[id^="xterm-container"]');
        const panes = document.querySelectorAll('.terminal-pane');
        const xtermDivs = document.querySelectorAll('.xterm');
        return {
            containers: containers.length,
            containerIds: Array.from(containers).map(c => c.id),
            panes: panes.length,
            xtermDivs: xtermDivs.length,
            xtermReady: window.__athenaXtermReady,
            terminalCount: window.__athenaTerminals ? window.__athenaTerminals.size : 0,
        };
    });

    console.log('DOM STATE:', JSON.stringify(domState, null, 2));

    if (domState.containers > 0 || domState.xtermDivs > 0) {
        console.log('✅ Terminal is rendered in DOM!');
    } else {
        console.log('❌ Terminal not rendered yet, checking for errors...');
    }

} finally {
    await browser.deleteSession();
}
