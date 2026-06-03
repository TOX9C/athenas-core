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

    // Try to fetch xterm files
    const result = await browser.execute(() => {
        return new Promise((resolve) => {
            const urls = [
                'tauri://localhost/xterm/xterm.min.js',
                '/xterm/xterm.min.js',
                './xterm/xterm.min.js',
                'xterm/xterm.min.js',
            ];

            const results = [];
            let pending = urls.length;

            urls.forEach(url => {
                fetch(url)
                    .then(r => results.push({ url, status: r.status, ok: r.ok }))
                    .catch(e => results.push({ url, error: String(e) }))
                    .finally(() => {
                        pending--;
                        if (pending === 0) resolve(results);
                    });
            });
        });
    });

    console.log('Fetch results:', JSON.stringify(result, null, 2));

} finally {
    await browser.deleteSession();
}
