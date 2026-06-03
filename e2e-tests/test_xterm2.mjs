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

    // Check for our log messages in the console
    const result = await browser.execute(() => {
        // Check if eval was even called
        return {
            xtermReady: typeof window.__athenaXtermReady !== 'undefined' ? window.__athenaXtermReady : 'undefined',
            // Check console for our logs - access the captured console if available
            hasConsoleCapture: typeof window.__athenaConsoleCapture !== 'undefined',
            // Try to get all window keys that start with __athena
            athenaKeys: Object.keys(window).filter(k => k.startsWith('__athena')),
        }
    });

    console.log('STATUS:', JSON.stringify(result, null, 2));

    // Trigger the terminal creation by clicking on a terminal pane
    await browser.execute(() => {
        // Find and click a terminal-related element
        const allElements = document.querySelectorAll('*');
        const terminalElements = Array.from(allElements).filter(el =>
            el.textContent?.toLowerCase().includes('terminal') ||
            el.getAttribute('title')?.toLowerCase().includes('terminal')
        );
        if (terminalElements.length > 0) {
            terminalElements[0].click();
            return 'clicked terminal element';
        }
        return 'no terminal element found';
    });

    await browser.pause(2000);

    const result2 = await browser.execute(() => {
        return {
            athenaKeys: Object.keys(window).filter(k => k.startsWith('__athena')),
            xtermReady: typeof window.__athenaXtermReady !== 'undefined' ? window.__athenaXtermReady : 'undefined',
        }
    });

    console.log('STATUS AFTER CLICK:', JSON.stringify(result2, null, 2));

} catch (e) {
    console.error('Error:', e);
} finally {
    await browser.deleteSession();
}
