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

    await browser.execute(() => {
        window.__testLogs = [];
        const originalLog = console.log;
        const originalError = console.error;
        const originalWarn = console.warn;

        console.log = (...args) => {
            window.__testLogs.push({ type: 'log', args: args.map(a => String(a)).join(' ') });
            originalLog.apply(console, args);
        };
        console.error = (...args) => {
            window.__testLogs.push({ type: 'error', args: args.map(a => String(a)).join(' ') });
            originalError.apply(console, args);
        };
        console.warn = (...args) => {
            window.__testLogs.push({ type: 'warn', args: args.map(a => String(a)).join(' ') });
            originalWarn.apply(console, args);
        };

        return 'console capture started';
    });

    await browser.pause(3000);

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

    const result = await browser.execute(() => {
        return {
            logs: window.__testLogs,
            athenaKeys: Object.keys(window).filter(k => k.startsWith('__athena')),
        };
    });

    console.log('Captured logs:', JSON.stringify(result.logs, null, 2));
    console.log('Athena keys:', result.athenaKeys);

} finally {
    await browser.deleteSession();
}
