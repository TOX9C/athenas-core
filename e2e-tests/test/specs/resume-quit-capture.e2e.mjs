// App-quit (Cmd+Q / window close) resume-capture end-to-end regression.
//
// Guards the graceful-shutdown capture path the frontend live scanner cannot
// cover: the backend scans pane output buffers during RunEvent::ExitRequested.
// Specifically protects against the budget/early-break regression where the
// outer timeout fired before the poll+merge finished whenever any non-agent
// pane existed, discarding every captured id.
//
// Chain under test:
//   hint line is printed into a mounted pane, its captured resume id is
//   wiped from `workspaces` (frontend scanner dedups — it will not restore
//   it) and a non-shell foreground process (`claude`-named sleep) makes the
//   pane classifiable as an agent
//   -> window close → RunEvent::ExitRequested → capture_resume_ids_on_exit
//      scans buffers + merges via set_sync
//   -> process exits; store.json on disk must contain the resume id again.
//
// NOTE: this spec QUITS the app, so it must run in its own wdio session and
// restores the `workspaces` key by editing store.json directly afterwards
// (no live app to IPC against). Ordering: keep it last alphabetically among
// resume specs is not guaranteed; the restore makes it safe anywhere.

import { readFileSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { join } from 'node:path';

const RESUME_ID = `e2e-quit-resume-${Date.now().toString(36)}-abcdef`;
const STORE_PATH = join(homedir(), 'Library', 'Application Support', 'athena-core', 'store.json');
const LOG_PATH = join(homedir(), 'Library', 'Logs', 'com.athena.core', "Athena's Core.log");

let ipcTokenCounter = 0;
async function ipcInvoke(cmd, args = {}) {
  const token = `__e2e_ipc_${Date.now()}_${ipcTokenCounter++}`;
  await browser.execute((t, c, a) => {
    localStorage.setItem(t, 'PENDING');
    window.__TAURI__.core.invoke(c, a).then(
      v => { localStorage.setItem(t, JSON.stringify({ ok: true, value: v === undefined ? null : v })); },
      e => { localStorage.setItem(t, JSON.stringify({ ok: false, error: typeof e === 'string' ? e : (e && e.message) || String(e) })); },
    );
  }, token, cmd, args);
  for (let i = 0; i < 40; i++) {
    const raw = await browser.execute(t => localStorage.getItem(t), token);
    if (raw !== null && raw !== 'PENDING') {
      await browser.execute(t => localStorage.removeItem(t), token);
      try { return JSON.parse(raw); } catch { return { ok: false, error: raw }; }
    }
    await browser.pause(250);
  }
  throw new Error(`invoke(${cmd}) never settled`);
}

const storeGet = async (key) => {
  const r = await ipcInvoke('store_get', { key });
  return r.ok ? r.value : null;
};

const findPane = (json, paneId) => {
  if (!json) return null;
  for (const space of JSON.parse(json).spaces || []) {
    for (const pane of space.panes || []) {
      if (pane.id === paneId) return pane;
    }
  }
  return null;
};

describe('app-quit resume capture', () => {
  let priorWorkspaces = null;

  it('persists the resume id into store.json when the app quits', async () => {
    priorWorkspaces = await storeGet('workspaces');

    await browser.waitUntil(
      async () => await browser.execute(() => {
        const m = document.querySelector('.xterm-mount');
        return !!m && !!m.getAttribute('data-pane-id');
      }),
      { timeout: 25000, timeoutMsg: 'no mounted xterm pane appeared (25s)' },
    );
    const paneId = await browser.execute(
      () => document.querySelector('.xterm-mount').getAttribute('data-pane-id'),
    );
    expect(paneId).toBeTruthy();

    // Print the hint, then wait for the frontend scanner to persist it.
    const write = await ipcInvoke('pty_write', {
      id: paneId,
      data: `printf 'omp --resume ${RESUME_ID}\\n'\r`,
    });
    if (!write.ok) throw new Error(`pty_write failed: ${write.error}`);
    await browser.waitUntil(
      async () => findPane(await storeGet('workspaces'), paneId)?.resume_id === RESUME_ID,
      { timeout: 15000, timeoutMsg: `frontend capture never persisted ${RESUME_ID}` },
    );

    // Wipe the persisted id (simulates the capture gap). Every later write
    // of this id can only come from the backend quit-capture path.
    const live = JSON.parse(await storeGet('workspaces'));
    for (const space of live.spaces || []) {
      for (const pane of space.panes || []) {
        if (pane.id === paneId) {
          delete pane.resume_id;
          delete pane.resume_cmd;
          pane.resume_dismissed = false;
        }
      }
    }
    const cleared = await ipcInvoke('store_set', {
      key: 'workspaces',
      value: JSON.stringify(live),
    });
    if (!cleared.ok) throw new Error(`store_set failed: ${cleared.error}`);

    // Give the pane a non-shell foreground so the quit path classifies it as
    // an agent (process name matches AGENT_FG_NAMES via the temp binary name).
    const mkagent = await ipcInvoke('pty_write', {
      id: paneId,
      data: `mkdir -p /tmp/e2e-agent && cp /bin/sleep /tmp/e2e-agent/claude && exec /tmp/e2e-agent/claude 30\r`,
    });
    if (!mkagent.ok) throw new Error(`pty_write failed: ${mkagent.error}`);
    // Honour the heartbeat/classify tick (1.5 s) so the pane is tracked as
    // agent-foreground before quit.
    await new Promise((r) => setTimeout(r, 2000));

    // Quit via the production path: `debug_request_exit` posts a real Cmd+Q
    // keystroke → RunEvent::ExitRequested → graceful-shutdown worker
    // (capture → merge → shutdown_all → rescan → flush → exit).
    // The browser session dies with the app; a mid-invoke throw is expected.
    await browser.pause(500);
    // Scope all subsequent log assertions to THIS run's tail: the log is
    // append-only across app launches (and old runs legitimately contain
    // "capture timed out" from the pre-fix 800 ms code).
    let logOffset = 0;
    try {
      logOffset = readFileSync(LOG_PATH, 'utf8').length;
    } catch {
      /* first run */
    }
    let exitInvokeOk = false;
    try {
      const r = await ipcInvoke('debug_request_exit');
      exitInvokeOk = r.ok === true;
    } catch {
      /* session died with the app — acceptable outcome */
    }

    // Proof the QUIT path ran (not the heartbeat): the app log must show the
    // ExitRequested → worker → capture sequence and must NOT show the outer
    // stall-guard timeout discarding captures.
    const logDeadline = Date.now() + 30000;
    let logText = '';
    while (Date.now() < logDeadline) {
      try {
        logText = readFileSync(LOG_PATH, 'utf8').slice(logOffset);
        if (
          logText.includes('debug_request_exit invoked') &&
          logText.includes('Exit requested -- scheduling bounded graceful shutdown') &&
          logText.includes('capture begin wait_ms=1200')
        ) {
          break;
        }
      } catch {
        /* log briefly unavailable */
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    expect(exitInvokeOk || logText.includes('debug_request_exit invoked')).toBe(true);
    expect(logText).toContain('Exit requested -- scheduling bounded graceful shutdown');
    expect(logText).toContain('[resume-debug] capture begin wait_ms=1200');
    expect(logText).not.toContain('capture timed out after');

    // The webdriver session dies with the app; wait for the store file to
    // contain the id (poll from the Node side, not the dead browser).
    const deadline = Date.now() + 30000;
    let storeJson = null;
    while (Date.now() < deadline) {
      try {
        storeJson = readFileSync(STORE_PATH, 'utf8');
        if (storeJson.includes(RESUME_ID)) break;
      } catch {
        /* store briefly locked/absent mid-write */
      }
      await new Promise((r) => setTimeout(r, 500));
    }
    expect(storeJson && storeJson.includes(RESUME_ID)).toBe(true);

    // Restore the prior workspaces key so other specs see a clean state.
    try {
      const disk = JSON.parse(readFileSync(STORE_PATH, 'utf8'));
      if (priorWorkspaces) {
        // Restore the pre-test layout, but strip every pane's resume fields —
        // restoring them wholesale would write an object (wrong shape) or
        // seed phantom resume banners into later runs.
        const restored = JSON.parse(priorWorkspaces);
        for (const space of restored.spaces || []) {
          for (const pane of space.panes || []) {
            delete pane.resume_id;
            delete pane.resume_cmd;
            delete pane.resume_dismissed;
          }
        }
        // The store persists `workspaces` as a JSON STRING, not an object.
        disk.workspaces = JSON.stringify(restored);
        writeFileSync(STORE_PATH, JSON.stringify(disk));
      }
    } catch (e) {
      // Best-effort cleanup; a missed restore only leaves a phantom banner.
      console.warn('quit-capture cleanup failed:', e);
    } finally {
      // Clean up the fake agent binary.
      try {
        const { rmSync } = await import('node:fs');
        rmSync('/tmp/e2e-agent', { recursive: true, force: true });
      } catch { /* already gone */ }
    }
  });
});
