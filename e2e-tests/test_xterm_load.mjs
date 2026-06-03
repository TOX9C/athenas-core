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

    // Trigger bootstrap and capture errors
    await browser.execute(() => {
        window.__bootstrapErrors = [];
        window.__bootstrapLogs = [];

        const originalLog = console.log;
        const originalError = console.error;

        console.log = (...args) => {
            window.__bootstrapLogs.push(args.map(a => String(a)).join(' '));
            originalLog.apply(console, args);
        };
        console.error = (...args) => {
            window.__bootstrapErrors.push(args.map(a => String(a)).join(' '));
            originalError.apply(console, args);
        };

        // Try to load xterm manually to see what happens
        __athenaEnsureXtermBootstrap().then(() => {
            window.__bootstrapLogs.push('BOOTSTRAP COMPLETE');
        }).catch(e => {
            window.__bootstrapErrors.push('BOOTSTRAP FAILED: ' + e.message);
        });

        return 'started';
    });

    await browser.pause(5000);

    const result = await browser.execute(() => {
        return {
            logs: window.__bootstrapLogs.slice(-15),
            errors: window.__bootstrapErrors,
            xtermReady: window.__athenaXtermReady,
            athenaKeys: Object.keys(window).filter(k => k.startsWith('__athena')),
        };
    });

    console.log('Logs:', JSON.stringify(result.logs, null, 2));
    console.log('Errors:', JSON.stringify(result.errors, null, 2));
    console.log('xtermReady:', result.xtermReady);
    console.log('Keys:', result.athenaKeys);

} finally {
    await browser.deleteSession();
}
