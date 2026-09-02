// Mobile Mirror pairing end-to-end (stubbed phone, real relay).
//
// Roadmap coverage item (2026-08-27, item 3): the relay's ws auth/pairing was
// unit-tested in Rust, but the desktop↔phone approval flow had no e2e. This
// spec drives the real pipeline without a physical phone:
//
//   Settings UI toggle ("Enable mobile mirror" — the hotspot switch)
//     -> relay_start command (LAN server, ephemeral port, fresh token)
//       -> relay_status url (token embedded — the QR/link a phone would scan)
//         -> fake phone: WS upgrade with the `athena-relay.<token>` subprotocol
//           -> desktop `relay:pairingRequest` event
//             -> RelayPairingPrompt modal ("Approve device pairing?")
//               -> click Allow -> relay_pairing_respond(true)
//                 -> WS session opens; invoke round-trip over the relay proves
//                    the paired session is live.
//
// The "phone" is a Node WebSocket client speaking the shim protocol
// (`{t:'invoke'}` / `{t:'resp'}`); no live mobile client is needed.
//
// WebDriver note: `browser.execute` does NOT await returned promises — an
// async in-page function serializes as `{}`. `invoke()` therefore parks each
// Tauri call on `window.__e2eBridge` and the Node side polls for settlement.
//
// Safety: the spec snapshots the persisted `relay.enabled` flag on entry, and
// always stops the relay and restores the flag in `after` — running the suite
// on a developer machine must not leave the LAN surface enabled.

import WebSocket from 'ws';

const RELAY_ENABLED_KEY = 'relay.enabled';

// `browser.execute` is a synchronous WebDriver script — a pending Promise
// serializes as `{}` — so promises are settled on `window.__e2eBridge` inside
// the page and polled from Node with the ticket threaded via a closure.
async function invokeSettledFlow(ticket) {
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
}

/** Fire-and-settle wrapper around `window.__TAURI__.core.invoke`. */
async function invokeCmd(cmd, args = {}) {
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
  return invokeSettledFlow(ticket);
}

/** Open the relay socket as a stubbed phone: WS upgrade with token auth. */
function openPhoneSocket(port, token) {
  return new WebSocket(`ws://127.0.0.1:${port}/ws`, [`athena-relay.${token}`]);
}

/** Send a shim `invoke` frame and await its `resp`. */
function phoneInvoke(ws, cmd, args) {
  return new Promise((resolve, reject) => {
    const id = `e2e-${Date.now()}`;
    const timer = setTimeout(() => reject(new Error(`relay invoke '${cmd}' timed out`)), 15000);
    const onMessage = (data) => {
      let msg;
      try {
        msg = JSON.parse(data.toString());
      } catch {
        return;
      }
      if (msg.t === 'resp' && msg.id === id) {
        clearTimeout(timer);
        ws.off('message', onMessage);
        if (msg.ok) resolve(msg.result);
        else reject(new Error(typeof msg.error === 'string' ? msg.error : JSON.stringify(msg.error)));
      }
    };
    ws.on('message', onMessage);
    ws.send(JSON.stringify({ t: 'invoke', id, cmd, args: args ?? {} }));
  });
}

describe('Mobile Mirror relay pairing (stubbed phone)', () => {
  let prevRelayEnabled; // string | null — pre-test persisted flag
  let phoneSocket = null;

  before(async () => {
    await browser.execute(() => {
      window.__athenaE2E = true;
    });
    prevRelayEnabled = await invokeCmd('store_get', { key: RELAY_ENABLED_KEY }).catch(() => null);
  });

  after(async () => {
    try {
      phoneSocket?.close();
    } catch { /* already closed */ }
    // Always stop the relay, then restore the pre-test persisted flag so the
    // next boot does not inherit a stray `relay.enabled = true`.
    try {
      await invokeCmd('relay_stop');
    } catch { /* app may already be closing */ }
    try {
      if (prevRelayEnabled == null) {
        await invokeCmd('store_delete', { key: RELAY_ENABLED_KEY });
      } else {
        await invokeCmd('store_set', { key: RELAY_ENABLED_KEY, value: prevRelayEnabled });
      }
    } catch { /* session may be gone */ }
  });

  it('completes a pairing session through the desktop approval UI', async function () {
    this.timeout(120000);

    // ── Enable the hotspot through the Settings UI ────────────────────────
    // Open Settings via the toolbar button; the modal mounts all sections,
    // including section VII "Mobile Mirror".
    const opened = await browser.execute(() => {
      for (const btn of document.querySelectorAll('button[title]')) {
        if (btn.title.startsWith('Settings')) {
          btn.click();
          return true;
        }
      }
      return false;
    });
    expect(opened).toBe(true);
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('#s-vii button.toggle')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Mobile Mirror toggle did not mount' },
    );

    await browser.execute(() => document.querySelector('#s-vii button.toggle').click());

    // Renderer surfaces the running state: status pill flips to "Running" and
    // the pairing card renders the tokened URL (what the phone would scan).
    await browser.waitUntil(
      async () => browser.execute(() => {
        const section = document.querySelector('#s-vii');
        if (!section) return false;
        return [...section.querySelectorAll('span')].some(s => s.textContent.trim() === 'Running');
      }),
      { timeout: 20000, interval: 300, timeoutMsg: 'Mobile Mirror status pill never switched to "Running"' },
    );
    await browser.waitUntil(
      async () => browser.execute(() =>
        !!(document.querySelector('#s-vii .mobile-pairing-url')?.textContent || '').trim()),
      { timeout: 10000, interval: 300, timeoutMsg: 'pairing URL not rendered in the Settings UI' },
    );

    // ── Get the session token/port the same way the QR encodes them ───────
    const statusJson = await invokeCmd('relay_status');
    const status = typeof statusJson === 'string' ? JSON.parse(statusJson) : statusJson;
    expect(status.running).toBe(true);
    expect(status.url).toContain('token=');
    const url = new URL(status.url);
    const token = url.searchParams.get('token');
    const port = status.port;
    expect(token).toBeTruthy();
    expect(port).toBeGreaterThan(0);

    // ── Stubbed phone connects — the upgrade parks until a human approves ──
    phoneSocket = openPhoneSocket(port, token);
    const phoneOpened = new Promise((resolve, reject) => {
      phoneSocket.once('open', () => resolve(true));
      phoneSocket.once('error', reject);
      phoneSocket.once('close', () => reject(new Error('phone socket closed without pairing')));
    });

    // ── The desktop surfaces the pairing request in the renderer ──────────
    await browser.waitUntil(
      async () => browser.execute(() => {
        return [...document.querySelectorAll('.modal-overlay')].some(overlay =>
          (overlay.getAttribute('aria-label') || '').includes('Approve device pairing'));
      }),
      { timeout: 15000, interval: 300, timeoutMsg: 'pairing approval modal never appeared' },
    );

    // Approve exactly like the desktop operator would.
    const clickedAllow = await browser.execute(() => {
      for (const overlay of document.querySelectorAll('.modal-overlay')) {
        if (!(overlay.getAttribute('aria-label') || '').includes('Approve device pairing')) continue;
        for (const btn of overlay.querySelectorAll('button')) {
          if (btn.textContent.trim() === 'Allow') {
            btn.click();
            return true;
          }
        }
      }
      return false;
    });
    expect(clickedAllow).toBe(true);

    // Paired: the phone socket upgrade completes.
    expect(await phoneOpened).toBe(true);

    // The prompt disappears once the desktop's respond command succeeds —
    // i.e. the renderer no longer shows a pending pairing.
    await browser.waitUntil(
      async () => browser.execute(() => {
        return ![...document.querySelectorAll('.modal-overlay')].some(overlay =>
          (overlay.getAttribute('aria-label') || '').includes('Approve device pairing'));
      }),
      { timeout: 10000, interval: 300, timeoutMsg: 'pairing modal did not dismiss after Allow' },
    );

    // ── Prove the paired session is live with a real relay round-trip ─────
    // A completed pairing means the phone may invoke relay-allowlisted
    // commands; `plugin_list` is arg-free and read-only, so it exercises the
    // full dispatch path with no fixture setup.
    const pluginsJson = await phoneInvoke(phoneSocket, 'plugin_list');
    const plugins = typeof pluginsJson === 'string' ? JSON.parse(pluginsJson) : pluginsJson;
    expect(Array.isArray(plugins)).toBe(true);

    // Close the phone like a real disconnect; no pending pairing remains.
    phoneSocket.close();
    phoneSocket = null;
  });
});
