// Kanban worksheet persistence end-to-end.
//
// Proves the board survives a panel remount AND the store round-trip:
//
//   UI create (input + Add task button)
//     -> kanban_create_task -> KanbanBackend::create_task
//       -> store key `kanban.<workspace_id>` (store.json on disk)
//     -> move menu ("In Progress" -> "Done") -> kanban_update_task
//       -> panel unmount/remount (kanban -> workspace -> kanban)
//         -> KanbanBoard on-mount kanban_get_tasks reload
//     -> store_get('kanban.<id>') still lists the task as Complete.
//
// A workspace ("worksheet") is required because the backend scopes tasks to
// the active space (`get_active_workspace_id`); the bootstrap mirrors
// athena-chat-stub.e2e.mjs (reuse an existing space, else create one).
//
// Cleanup: the test deletes the task it created in `after` (best effort) so
// the shared store.json is left as it was found.
const TASK_TITLE = `KB_E2E_${Date.now()}`;
// browser.execute NEVER awaits a returned Promise (WebDriver sync script
// serialization turns a Promise into `{}`), so Tauri invoke calls must be
// bridged: fire-and-forget in the page, park the result on `window`, then
// poll for it synchronously.
let ipcTokenCounter = 0;
async function ipcInvoke(cmd, args = {}) {
  const token = `__e2e_ipc_${Date.now()}_${ipcTokenCounter++}`;
  await browser.execute((t, c, a) => {
    // Park on localStorage, not window: the app may reload mid-bootstrap,
    // and individual `browser.execute` scripts do not share window state
    // reliably across the navigation.
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

// Shared browser-side helpers (leap in one evaluate round-trip each).
// Throws on backend errors so persistence assertions fail loudly.
const invoke = async (cmd, args = {}) => {
  const r = await ipcInvoke(cmd, args);
  if (!r.ok) throw new Error(`invoke(${cmd}) failed: ${r.error}`);
  return r.value;
};
const clickButton = (text) => browser.execute(t => {
  for (const button of document.querySelectorAll('button')) {
    if ((button.textContent || '').trim().includes(t)) { button.click(); return true; }
  }
  return false;
}, text);

// Which column (by header text) currently holds our task card.
const columnOfTask = () => browser.execute(title => {
  for (const col of document.querySelectorAll('.kanban-column')) {
    const header = col.querySelector('div > span');
    const name = (header?.textContent || '').trim();
    const inCol = [...col.querySelectorAll('.kanban-card')]
      .some(card => (card.textContent || '').includes(title));
    if (inCol) return name;
  }
  return null;
}, TASK_TITLE);

const moveCardTo = async (menuLabel) => {
  const opened = await browser.execute(title => {
    const card = [...document.querySelectorAll('.kanban-card')]
      .find(c => (c.textContent || '').includes(title));
    if (!card) return false;
    const btn = card.querySelector('button[title="Move to column"]');
    if (!btn) return false;
    btn.click();
    return true;
  }, TASK_TITLE);
  expect(opened).toBe(true);
  await browser.pause(200);
  // The menu renders as floating buttons labelled with the column names.
  expect(await clickButton(menuLabel)).toBe(true);
};

describe('Kanban worksheet persistence', () => {
  let activeSpaceId = null;
  let createdTaskId = null;

  after(async () => {
    // Best-effort cleanup: drop the task this spec created from the store.
    if (createdTaskId) {
      try {
        await ipcInvoke('kanban_delete_task', { taskId: createdTaskId }).catch(() => {});
      } catch { /* session may already be gone */ }
    }
  });

  it('creates a task, advances to-do -> doing -> done, and persists across panel remounts', async function () {
    this.timeout(90000);

    // --- Bootstrap a worksheet (workspace) -------------------------------
    await browser.execute(() => {
      window.__athenaE2E = true;
      window.__TAURI__.core.invoke('workspace_add_trusted_root', { dir: '/tmp' }).catch(() => {});
    });
    const hasPill = async () => browser.execute(() => !!document.querySelector('[data-agent-pill]'));
    if (!(await hasPill())) {
      if (await clickButton('Space 1')) {
        await browser.pause(1500);
      }
    }
    if (!(await hasPill())) {
      expect(await clickButton('New Workspace')).toBe(true);
      await browser.waitUntil(
        async () => browser.execute(() => !!document.querySelector('.modal-overlay')),
        { timeout: 10000, interval: 250, timeoutMsg: 'New Workspace modal did not open' },
      );
      expect(await clickButton('Terminal Workspace')).toBe(true);
      await browser.pause(400);
      expect(await clickButton('Next >')).toBe(true);
      try {
        await browser.waitUntil(
          async () => browser.execute(() => !!document.getElementById('add-claude')),
          { timeout: 10000, interval: 250, timeoutMsg: 'Agent configuration step did not appear' },
        );
      } catch (e) {
        const dump = await browser.execute(() => ({
          overlays: document.querySelectorAll('.modal-overlay').length,
          ids: [...document.querySelectorAll('[id^="add-"]')].map(el => el.id),
          text: [...document.querySelectorAll('.modal-overlay')].map(m => (m.textContent || '').slice(0, 400)),
          pills: document.querySelectorAll('[data-agent-pill]').length,
        }));
        console.log('=== bootstrap dump ===', JSON.stringify(dump));
        throw e;
      }
      await browser.execute(() => document.getElementById('add-claude')?.click());
      expect(await clickButton('Launch Space')).toBe(true);
    }
    await browser.waitUntil(hasPill, {
      timeout: 25000, interval: 500, timeoutMsg: 'Agent pane pill did not mount',
    });

    // --- Open the kanban panel --------------------------------------------
    await browser.execute(() => {
      const btn = [...document.querySelectorAll('.tb-panel-switcher button')]
        .find(b => (b.textContent || '').trim().toLowerCase() === 'kanban');
      btn?.click();
    });
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.kanban-board')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Kanban board did not mount' },
    );

    // Snapshot tasks the board already has (from prior suite runs) so later
    // assertions only look at OUR task.
    // --- Create a task via the board UI ----------------------------------
    const inputFound = await browser.execute(title => {
      const board = document.querySelector('.kanban-board');
      const input = board?.querySelector('input.field');
      if (!input) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set;
      setter.call(input, title);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      return true;
    }, TASK_TITLE);
    expect(inputFound).toBe(true);
    await browser.execute(() => {
      document.querySelector('.kanban-board button[title="Add task"]')?.click();
    });
    await browser.waitUntil(
      async () => (await columnOfTask()) !== null,
      { timeout: 10000, interval: 500, timeoutMsg: `task "${TASK_TITLE}" never appeared on the board` },
    );
    expect(await columnOfTask()).toBe('To Do');

    // --- Advance: To Do -> In Progress ("doing") -> Done ------------------
    await moveCardTo('In Progress');
    await browser.waitUntil(
      async () => (await columnOfTask()) === 'In Progress',
      { timeout: 10000, interval: 500, timeoutMsg: 'task did not move to In Progress' },
    );
    await moveCardTo('Done');
    await browser.waitUntil(
      async () => (await columnOfTask()) === 'Done',
      { timeout: 10000, interval: 500, timeoutMsg: 'task did not move to Done' },
    );

    // --- Persisted in the store contract already ---------------------------
    const workspacesJson = await invoke('store_get', { key: 'workspaces' });
    activeSpaceId = JSON.parse(workspacesJson).active_space_id;
    expect(activeSpaceId).toBeTruthy();
    const kanbanJson = await invoke('store_get', { key: `kanban.${activeSpaceId}` });
    const persisted = JSON.parse(kanbanJson);
    const mine = persisted.find(t => t.title === TASK_TITLE);
    expect(mine).toBeTruthy();
    expect(mine.status).toBe('Complete');
    createdTaskId = mine.id;

    // --- Re-render: leave the panel and come back (full board remount) -----
    await browser.execute(() => {
      const btn = [...document.querySelectorAll('.tb-panel-switcher button')]
        .find(b => (b.textContent || '').trim().toLowerCase() === 'workspace');
      btn?.click();
    });
    await browser.waitUntil(
      async () => browser.execute(() => !document.querySelector('.kanban-board')),
      { timeout: 10000, interval: 250, timeoutMsg: 'kanban board did not unmount on workspace switch' },
    );
    await browser.execute(() => {
      const btn = [...document.querySelectorAll('.tb-panel-switcher button')]
        .find(b => (b.textContent || '').trim().toLowerCase() === 'kanban');
      btn?.click();
    });
    // Board remounts, reloads from the backend — our task must still be Done.
    await browser.waitUntil(
      async () => (await columnOfTask()) === 'Done',
      { timeout: 10000, interval: 500, timeoutMsg: 'task did not reload into Done after panel remount' },
    );
  });
});
