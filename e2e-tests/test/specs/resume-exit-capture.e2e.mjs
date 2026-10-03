// Backend exit-transition resume-capture end-to-end regression.
//
// Guards the fix for the `pty:raw` suppression gap: when a pane's xterm
// listener is paused/detached (hidden workspace, remount, mobile), the
// frontend ResumeScanner never sees the agent's resume line, and the only
// backend backstop ran at app quit — so exiting an agent in a hidden pane
// lost the resume banner entirely. The heartbeat now detects the
// agent→shell foreground transition and scans the backend OutputBuffer.
//
// Chain under test:
//   pty_write echoes a harness-style `omp --resume <uuid>` line into a pane
//   -> frontend live scanner persists it (asserted, then CLEARED from the
//      store to simulate the hidden-pane miss; the scanner dedups per id so
//      it will not re-persist)
//   -> `sleep` runs as a non-shell foreground process and exits
//   -> heartbeat observes the foreground transition, scans the pane buffer,
//      merges the id back into `workspaces` and emits workspace:changed
//   -> store_get('workspaces') shows the captured id on that pane again.

const RESUME_ID = `e2e-backend-resume-${Date.now().toString(36)}-abcdef`;

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

describe('backend exit-transition resume capture', () => {
  let priorWorkspaces = null;
  let paneId = null;

  before(async () => {
    priorWorkspaces = await storeGet('workspaces');
  });

  it('re-captures resume_id from the output buffer after an agent exits', async () => {
    await browser.waitUntil(
      async () => await browser.execute(() => {
        const m = document.querySelector('.xterm-mount');
        return !!m && !!m.getAttribute('data-pane-id');
      }),
      { timeout: 25000, timeoutMsg: 'no mounted xterm pane appeared (25s)' },
    );
    paneId = await browser.execute(
      () => document.querySelector('.xterm-mount').getAttribute('data-pane-id'),
    );
    expect(paneId).toBeTruthy();

    // 1) Print the hint; the frontend live scanner persists it.
    const write = await ipcInvoke('pty_write', {
      id: paneId,
      data: `printf 'omp --resume ${RESUME_ID}\\n'\r`,
    });
    if (!write.ok) throw new Error(`pty_write failed: ${write.error}`);
    await browser.waitUntil(
      async () => findPane(await storeGet('workspaces'), paneId)?.resume_id === RESUME_ID,
      { timeout: 15000, timeoutMsg: `frontend capture never persisted ${RESUME_ID}` },
    );

    // 2) Simulate the hidden-pane miss: wipe the captured ids. The frontend
    //    scanner dedups per id per mount, so only the backend heartbeat can
    //    restore it.
    const clearedWorkspaces = (() => {
      const root = JSON.parse(priorWorkspaces ?? '{"spaces":[],"active_space_id":null}');
      return JSON.stringify(root);
    })();
    // Rebuild from the LIVE state minus this pane's resume fields so the
    // frontend's current workspace (space/pane layout) is untouched.
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
      value: JSON.stringify(live) || clearedWorkspaces,
    });
    if (!cleared.ok) throw new Error(`store_set failed: ${cleared.error}`);
    expect(findPane(JSON.stringify(live), paneId).resume_id).toBeUndefined();

    // 3) Run a non-shell foreground process so the heartbeat sees a
    //    foreground transition when it exits, then wait for re-capture.
    const sleepWrite = await ipcInvoke('pty_write', { id: paneId, data: 'sleep 1\r' });
    if (!sleepWrite.ok) throw new Error(`pty_write failed: ${sleepWrite.error}`);
    await browser.waitUntil(
      async () => findPane(await storeGet('workspaces'), paneId)?.resume_id === RESUME_ID,
      {
        timeout: 20000,
        timeoutMsg:
          `backend heartbeat never re-captured ${RESUME_ID} after agent exit ` +
          `(pane ${paneId})`,
      },
    );

    const pane = findPane(await storeGet('workspaces'), paneId);
    expect(pane.resume_cmd).toBe(`omp --resume ${RESUME_ID}`);
  });

  after(async () => {
    if (priorWorkspaces !== null) {
      await ipcInvoke('store_set', { key: 'workspaces', value: priorWorkspaces });
    }
  });
});
