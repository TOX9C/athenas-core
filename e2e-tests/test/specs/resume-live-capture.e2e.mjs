// Live resume-capture end-to-end regression.
//
// Guards the fix for the frontend ResumeScanner feed being dropped in the
// xterm_mount rework (8a9bb08): with no live scanner, `<cli> --resume <id>`
// lines printed to a running pane were never persisted, so the resume
// banner had nothing to show for OMP/Claude Code sessions unless the
// app-exit capture path happened to fire.
//
// Chain under test:
//   pty_write echoes a harness-style `omp --resume <uuid>` line into a real
//     mounted pane's PTY
//   -> xterm raw listener feeds bytes into ResumeScanner (xterm_mount.rs)
//   -> match -> update_space persists resume_id/resume_cmd into the
//     `workspaces` store
//   -> store_get('workspaces') shows the captured id on that pane.
//
// The write is match-only (one update_space per distinct id), so repeated
// output must NOT repeatedly rewrite the store; uniqueness of the uuid
// keeps the assertion free of false positives from other sessions.
//
// Cleanup: restores the pre-test `workspaces` JSON in `after` so the pane
// is left without a phantom resume banner.

const RESUME_ID = `e2e-resume-${Date.now().toString(36)}-abcdef`;

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

describe('live resume capture', () => {
  let priorWorkspaces = null;
  let paneId = null;

  before(async () => {
    // Snapshot persisted workspaces so `after` can undo the capture.
    priorWorkspaces = await storeGet('workspaces');
  });

  it('persists resume_id/resume_cmd from harness output in a live pane', async () => {
    // Wait for a mounted terminal pane. The e2e app bootstraps a workspace
    // with at least one pane; the raw listener (and thus the scanner) only
    // exists once an .xterm-mount is attached.
    await browser.waitUntil(
      async () =>
        await browser.execute(() => {
          const m = document.querySelector('.xterm-mount');
          return !!m && !!m.getAttribute('data-pane-id');
        }),
      { timeout: 25000, timeoutMsg: 'no mounted xterm pane appeared (25s)' },
    );
    paneId = await browser.execute(
      () => document.querySelector('.xterm-mount').getAttribute('data-pane-id'),
    );
    expect(paneId).toBeTruthy();

    // Shell echoes the printf arguments and then printf itself prints the
    // line — either copy reaching the scanner is a match.
    const write = await ipcInvoke('pty_write', {
      id: paneId,
      data: `printf 'omp --resume ${RESUME_ID}\\n'\r`,
    });
    if (!write.ok) throw new Error(`pty_write failed: ${write.error}`);

    // Workspace persistence is queued/coalesced: poll the store until the
    // captured id lands on this pane.
    let captured = null;
    await browser.waitUntil(
      async () => {
        const json = await storeGet('workspaces');
        if (!json) return false;
        const spaces = JSON.parse(json).spaces || [];
        for (const space of spaces) {
          for (const pane of space.panes || []) {
            if (pane.id === paneId && pane.resume_id === RESUME_ID) {
              captured = pane;
              return true;
            }
          }
        }
        return false;
      },
      {
        timeout: 15000,
        timeoutMsg: `resume_id ${RESUME_ID} never persisted for pane ${paneId}`,
      },
    );

    expect(captured.resume_cmd).toBe(`omp --resume ${RESUME_ID}`);
    // A fresh capture must reset the dismissed flag so the banner reappears.
    expect(captured.resume_dismissed).toBeFalsy();
  });

  after(async () => {
    if (priorWorkspaces !== null) {
      await ipcInvoke('store_set', { key: 'workspaces', value: priorWorkspaces });
    }
  });
});
