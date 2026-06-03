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

    const result = await browser.execute(() => {
        return {
            xtermReady: typeof window.__athenaXtermReady !== 'undefined' ? window.__athenaXtermReady : 'undefined',
            bootstrapFnExists: typeof window.__athenaEnsureXtermBootstrap !== 'undefined',
            createTerminalExists: typeof window.__athenaCreateTerminal !== 'undefined',
            terminalClassExists: typeof window.Terminal !== 'undefined',
        }
    });

    console.log('XTERM STATUS:', JSON.stringify(result, null, 2));

} catch (e) {
    console.error('Error:', e);
} finally {
    await browser.deleteSession();
}
