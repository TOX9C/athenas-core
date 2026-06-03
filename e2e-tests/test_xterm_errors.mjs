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

    // Check what happens when we try to load xterm scripts manually
    const result = await browser.execute(() => {
        return new Promise((resolve) => {
            const s = document.createElement('script');
            s.src = './xterm/xterm.min.js';
            s.onload = () => resolve({ status: 'loaded', hasTerminal: typeof Terminal !== 'undefined' });
            s.onerror = (e) => resolve({ status: 'error', error: String(e), src: s.src });
            document.head.appendChild(s);
        });
    });

    console.log('Script load result:', JSON.stringify(result, null, 2));

    // Also try with full path
    const result2 = await browser.execute(() => {
        return new Promise((resolve) => {
            const s = document.createElement('script');
            s.src = 'xterm/xterm.min.js';
            s.onload = () => resolve({ status: 'loaded', hasTerminal: typeof Terminal !== 'undefined' });
            s.onerror = (e) => resolve({ status: 'error', error: String(e), src: s.src });
            document.head.appendChild(s);
        });
    });

    console.log('Script load result (no dot):', JSON.stringify(result2, null, 2));

    // Check the current URL
    const url = await browser.execute(() => {
        return {
            href: window.location.href,
            origin: window.location.origin,
            pathname: window.location.pathname,
        };
    });
    console.log('Current URL:', JSON.stringify(url));

} finally {
    await browser.deleteSession();
}
