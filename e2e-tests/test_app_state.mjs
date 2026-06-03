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
    await browser.pause(3000)

    // Click New Workspace first
    await browser.execute(() => {
        const btns = document.querySelectorAll('button');
        for (const btn of btns) {
            if (btn.textContent.includes('New Workspace')) {
                btn.dispatchEvent(new MouseEvent('click', { bubbles: true }));
                return 'clicked workspace';
            }
        }
        return 'no workspace btn';
    });

    await browser.pause(2000);

    // Now check the DOM for terminal elements
    const domState = await browser.execute(() => {
        const terminalContainers = document.querySelectorAll('[id^="xterm-container"]');
        const terminalPanes = document.querySelectorAll('.terminal-pane');
        const allIds = Array.from(document.querySelectorAll('[id]')).map(el => el.id);
        return {
            terminalContainers: terminalContainers.length,
            terminalPanes: terminalPanes.length,
            allIds: allIds.filter(id => id.includes('term') || id.includes('xterm')),
        };
    });

    console.log('DOM STATE:', JSON.stringify(domState, null, 2));

    // Now check window globals again
    const globals = await browser.execute(() => {
        return {
            athenaKeys: Object.keys(window).filter(k => k.startsWith('__athena')),
            xtermReady: typeof window.__athenaXtermReady !== 'undefined' ? window.__athenaXtermReady : 'undefined',
        }
    });

    console.log('GLOBALS:', JSON.stringify(globals, null, 2));

} finally {
    await browser.deleteSession();
}
