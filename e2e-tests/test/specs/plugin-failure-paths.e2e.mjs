// Plugin failure paths end-to-end (stub harness only — no live plugins).
//
// Roadmap coverage item (2026-08-27, item 4): malformed-manifest handling and
// plugin crash isolation are unit-tested in `athena-plugins` but had no e2e.
// This spec drives the failure surfaces through the real app without any
// third-party plugin processes:
//
//   1. Install failure: a plugin directory mixing one malformed manifest with
//      one valid manifest is discovered via `plugin_host_discover_plugins`
//      (the same command the host setup flow uses). The bad manifest must
//      report an error entry; the valid sibling must still be discovered —
//      install failure is isolated, not fatal to the slate.
//
//   2. Crash after install: a stub plugin registers successfully, then the
//      host reports a launch crash via `plugin_set_error` (the same command a
//      crashing plugin host uses). The backend must record status `error`
//      with the crash message (asserted via `plugin_get`), the plugin must
//      appear in the Plugins slate (dashboard card), and the card's toggle —
//      the retry affordance on a failed entry — must re-enable it, flipping
//      `plugin_get` back to `enabled`.
//
// UI notes (surfaces that use these commands): the Plugins slate is the
// sidebar "Plugins" section (`PluginDashboard`). There is no separate
// install-wizard UI; discovery/registration are command-driven, so the spec
// invokes those commands through the frontend's own Tauri bridge and asserts
// the renderer surfaces that exist (the slate list + card retry toggle).
//
// Safety: plugins registered here are unregistered in `after`, the trusted
// root is removed, and the temp manifest dir is deleted.

import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// `browser.execute` is a synchronous WebDriver script — a pending Promise
// serializes as `{}` — so promises are settled on `window.__e2eBridge` inside
// the page and polled from Node with the ticket threaded via a closure.
const invoke = async function invokeCmd(cmd, args = {}) {
  const ticket = `t${Date.now()}-${Math.random().toString(36).slice(2)}`;
  await browser.execute((t, c, a) => {
    window.__e2eBridge = window.__e2eBridge || {};
    window.__e2eBridge[t] = { status: 'pending' };
    window.__TAURI__.core.invoke(c, a).then(
      (v) => {
        window.__e2eBridge[t] = { status: 'ok', value: v === undefined ? null : v };
      },
      (e) => {
        window.__e2eBridge[t] = { status: 'err', error: String(e) };
      },
    );
  }, ticket, cmd, args);
  await browser.waitUntil(
    async () => browser.execute(
      (t) => {
        const entry = window.__e2eBridge?.[t];
        return entry && entry.status !== 'pending';
      },
      ticket,
    ),
    { timeout: 30000, interval: 200, timeoutMsg: `invoke ticket ${ticket} never settled` },
  );
  const res = await browser.execute((t) => {
    const entry = window.__e2eBridge[t];
    delete window.__e2eBridge[t];
    return entry;
  }, ticket);
  if (res.status === 'err') {
    throw new Error(`invoke failed: ${res.error}`);
  }
  return res.value;
};

describe('Plugin failure paths (stub harness)', () => {
  let pluginDir; // temp dir holding the stub manifests
  let addedRoot; // trusted root we registered, to remove in after
  const CRASHED_ID = `e2e-crashy-${Date.now()}`;
  const GOOD_ID = `e2e-good-${Date.now()}`;

  before(async () => {
    await browser.execute(() => {
      window.__athenaE2E = true;
    });

    // Stub plugin directory: one malformed manifest (invalid JSON) beside one
    // valid manifest. Discovery scans *.json in the directory.
    pluginDir = mkdtempSync(join(tmpdir(), 'athena-e2e-plugins-'));
    writeFileSync(join(pluginDir, 'broken.plugin.json'), '{ "id": "e2e-broken", not-json ');
    writeFileSync(
      join(pluginDir, 'good.plugin.json'),
      JSON.stringify({
        id: GOOD_ID,
        name: 'E2E Good Plugin',
        version: '0.0.1',
        description: 'stub',
        author: 'e2e',
      }),
    );

    // Discover requires the dir to live under a trusted root.
    await invoke('workspace_add_trusted_root', { dir: pluginDir });
    addedRoot = pluginDir;
  });

  after(async () => {
    for (const id of [CRASHED_ID, GOOD_ID]) {
      try {
        await invoke('plugin_unregister', { pluginId: id });
      } catch { /* not registered / app closing */ }
    }
    try {
      await invoke('workspace_remove_trusted_root', { dir: addedRoot });
    } catch { /* best effort */ }
    try {
      rmSync(pluginDir, { recursive: true, force: true });
    } catch { /* best effort */ }
  });

  it('reports a malformed manifest as failed without breaking discovery of the valid sibling', async function () {
    this.timeout(60000);

    const json = await invoke('plugin_host_discover_plugins', { dir: pluginDir });
    const results = JSON.parse(json);
    expect(Array.isArray(results)).toBe(true);
    expect(results.length).toBeGreaterThanOrEqual(2);

    // The malformed scan entry is reported as an error object, not a throw
    // that nukes the whole discovery result.
    const failed = results.find(r => typeof r?.error === 'string');
    expect(failed).toBeTruthy();
    expect(failed.error.toLowerCase()).toMatch(/manifest|parse/);

    // The valid sibling still installs — failure is contained to the bad file.
    const good = results.find(r => r?.id === GOOD_ID);
    expect(good).toBeTruthy();
    expect(good.name).toBe('E2E Good Plugin');
  });

  it('surfaces a post-install launch crash on the slate and recovers via the retry toggle', async function () {
    this.timeout(60000);

    // Register the stub plugin (install succeeds — this is the "installs" half
    // of "installs but crashes on launch").
    await invoke('plugin_register', { pluginId: CRASHED_ID, name: 'E2E Crashy Plugin', version: '0.1.0' });

    // Open the Plugins slate (sidebar section) and confirm the card renders.
    const pluginsOpened = await browser.execute(() => {
      for (const btn of document.querySelectorAll('button[title]')) {
        if (btn.title === 'Plugins') {
          btn.click();
          return true;
        }
      }
      return false;
    });
    expect(pluginsOpened).toBe(true);
    await browser.waitUntil(
      async () => browser.execute(
        (id) => !!document.querySelector('.plugin-dashboard')
          && document.querySelector('.plugin-dashboard').textContent.includes(id),
        CRASHED_ID,
      ),
      { timeout: 15000, interval: 300, timeoutMsg: 'registered plugin never appeared in the Plugins slate' },
    );

    // Simulate the launch crash: the host reports the plugin errored.
    const crashMessage = 'stub plugin crashed on launch';
    await invoke('plugin_set_error', { pluginId: CRASHED_ID, error: crashMessage });

    // Launch-outcome state: status is `error` and carries the crash message.
    const erroredInfo = JSON.parse(await invoke('plugin_get', { pluginId: CRASHED_ID }));
    expect(erroredInfo.status).toBe('error');
    expect(erroredInfo.error).toBe(crashMessage);

    // Retry: click the failed card's toggle (the slate's recovery affordance)
    // — a failed (disabled) entry shows the OFF state; clicking enables it.
    const clickedToggle = await browser.execute((id) => {
      const dashboard = document.querySelector('.plugin-dashboard');
      if (!dashboard) return false;
      for (const card of dashboard.querySelectorAll('.card')) {
        if (!(card.textContent || '').includes(id)) continue;
        const toggle = card.querySelector('button.btn-secondary');
        if (toggle) {
          toggle.click();
          return true;
        }
      }
      return false;
    }, CRASHED_ID);
    expect(clickedToggle).toBe(true);

    // The retry takes the error out: plugin_get reports the plugin enabled
    // again with the error cleared.
    await browser.waitUntil(
      async () => {
        try {
          const info = JSON.parse(await invoke('plugin_get', { pluginId: CRASHED_ID }));
          return info.status === 'enabled' && !info.error;
        } catch {
          return false;
        }
      },
      { timeout: 10000, interval: 300, timeoutMsg: 'plugin did not recover from its error state via the retry toggle' },
    );
  });
});
