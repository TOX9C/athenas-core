// Regression test for the Tauri `pty:raw` broadcast race bug.
//
// Bug: when 3+ concurrent PTY read loops called `app_handle.emit("pty:raw", &Value)`
// with a shared `&serde_json::Value` borrow, all frontend listeners received
// payloads whose `sessionId` field matched whichever task had last serialized.
// Only the last session's output reached the UI; other panes stopped updating.
//
// Fix: serialize to a fully-owned `String` before calling emit. This forces
// serialization onto the emitting task, eliminating the borrow-sharing race.
// Applied to all 12 emit sites in `src-tauri/src/commands/mod.rs` and
// `src-tauri/src/state.rs` (the 7 `wire_*_events` closures and the 4
// `pty_*` emit sites, plus `athena:askUser`).
//
// This test exercises the fix at the IPC layer (bypassing the xterm UI):
// 1. Registers a pty:raw listener (fire-and-forget)
// 2. Spawns 3 PTY sessions with distinct IDs via __TAURI__.core.invoke
// 3. Waits 6s for the read loops to emit several rounds
// 4. Verifies the listener received payloads for ALL 3 distinct sessionIds
//
// Run:
//   Terminal 1: tauri-wd --port 4444
//   Terminal 2: node e2e-tests/test_broadcast_direct.mjs
//
// Exit code 0 = fix holds; 1 = race regressed.

import { remote } from 'webdriverio';

const browser = await remote({
    hostname: '127.0.0.1',
    port: 4444,
    capabilities: {
        browserName: 'safari',
        'tauri:options': {
            binary: '/Users/apollo/Documents/athenas-core/target/debug/athenas-core',
        },
    },
});

let exitCode = 0;

try {
    // Wait for __TAURI__ to be available
    await browser.waitUntil(
        async () => {
            return await browser.execute(() => {
                return typeof window.__TAURI__ !== 'undefined' &&
                       typeof window.__TAURI__.core?.invoke === 'function' &&
                       typeof window.__TAURI__.event?.listen === 'function';
            });
        },
        { timeout: 25000, timeoutMsg: '__TAURI__ APIs never appeared (25s)' },
    );
    console.log('OK __TAURI__ APIs available');

    // Step 1: Initialize globals + register listener (fire-and-forget)
    const regResult = await browser.execute(() => {
        window.__ptyPayloads = [];
        window.__ptyErrors = [];
        try {
            // Don't await — fire and forget. The Promise resolves later.
            window.__TAURI__.event.listen('pty:raw', (event) => {
                try {
                    const p = event.payload;
                    if (typeof p === 'string') {
                        try { window.__ptyPayloads.push(JSON.parse(p)); }
                        catch { window.__ptyPayloads.push({ raw: p }); }
                    } else if (p && typeof p === 'object') {
                        window.__ptyPayloads.push(p);
                    } else {
                        window.__ptyPayloads.push({ unexpected: typeof p });
                    }
                } catch (e) {
                    window.__ptyErrors.push('callback err: ' + String(e));
                }
            });
            return 'registered-fire-and-forget';
        } catch (e) {
            return 'reg-error: ' + String(e);
        }
    });
    console.log('listener:', regResult);

    // Step 2: Spawn 3 PTY sessions (fire-and-forget)
    const t0 = Date.now();
    const ids = ['e2e-alpha', 'e2e-beta', 'e2e-gamma'];
    for (const id of ids) {
        const result = await browser.execute((sessionId) => {
            try {
                // Fire and forget — don't await
                window.__TAURI__.core.invoke('pty_spawn', {
                    id: sessionId,
                    cwd: '/tmp',
                    shell: '/bin/zsh',
                    cols: 80,
                    rows: 24,
                }).then(r => {
                    window.__ptyPayloads.push({ _spawnAck: sessionId, value: r });
                }).catch(e => {
                    window.__ptyErrors.push('spawn ' + sessionId + ': ' + String(e));
                });
                return 'spawn-fired-' + sessionId;
            } catch (e) {
                return 'spawn-sync-err: ' + String(e);
            }
        }, id);
        console.log(`[${Date.now() - t0}ms]`, result);
    }

    // Step 3: Wait for events to flow
    await browser.pause(6000);

    // Step 4: Read collected payloads (synchronous)
    const collected = await browser.execute(() => {
        const payloads = window.__ptyPayloads || [];
        const sessionIds = payloads
            .map(p => p && p.sessionId)
            .filter(s => typeof s === 'string');
        const uniqueIds = [...new Set(sessionIds)];
        const idCounts = {};
        for (const s of sessionIds) {
            idCounts[s] = (idCounts[s] || 0) + 1;
        }
        return {
            totalPayloads: payloads.length,
            uniqueSessionIds: uniqueIds,
            perIdCount: idCounts,
            first3: payloads.slice(0, 3).map(p => ({
                sessionId: p && p.sessionId,
                keys: p ? Object.keys(p) : [],
                dataPreview: p && p.data ? String(p.data).slice(0, 60) : null,
                isSpawnAck: !!p?._spawnAck,
            })),
            errors: window.__ptyErrors || [],
        };
    });

    console.log('\n=== COLLECTED ===');
    console.log(JSON.stringify(collected, null, 2));

    console.log('\n=== VERDICT ===');
    // Subtract _spawnAck entries from the count for fair comparison
    const realPtyRawCount = collected.totalPayloads - collected.first3.filter(p => p.isSpawnAck).length;
    // Actually let's check perIdCount for unique session IDs (excluding _spawnAck payloads)
    const realUniqueIds = Object.keys(collected.perIdCount).filter(k => k !== 'undefined');

    if (collected.errors.length > 0) {
        console.log('WARN: JS errors during test:');
        collected.errors.forEach(e => console.log('  ' + e));
    }

    if (collected.totalPayloads === 0) {
        console.log('FAIL: No pty:raw payloads received in 6s window.');
        console.log('   Either PTYs did not spawn, or listener was not registered in time.');
        exitCode = 1;
    } else if (realUniqueIds.length >= 3) {
        console.log(`PASS: ${realUniqueIds.length} distinct sessionIds in listener payloads: ${JSON.stringify(realUniqueIds)}`);
        console.log(`   Per-session payload counts: ${JSON.stringify(collected.perIdCount)}`);
        console.log('   -> Race fix works: backend emits 3 distinct payloads, IPC delivers them');
    } else {
        console.log(`FAIL: only ${realUniqueIds.length} distinct sessionId(s): ${JSON.stringify(realUniqueIds)}`);
        console.log(`   Per-session counts: ${JSON.stringify(collected.perIdCount)}`);
        console.log(`   Total payloads: ${collected.totalPayloads}`);
        console.log('   -> Race still present, or PTYs are clobbering each other');
        exitCode = 1;
    }
} catch (e) {
    console.error('Test error:', e);
    exitCode = 1;
} finally {
    await browser.deleteSession().catch(() => {});
    process.exit(exitCode);
}
