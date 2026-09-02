// Stubbed-provider end-to-end for the Athena chat path.
//
// Roadmap coverage item (2026-08-31): chat is the last major path with no
// end-to-end coverage. This spec proves the full pipeline without touching a
// real LLM:
//
//   Settings store (llm.model / llm.base_url)
//     + macOS keyring (service `athena`, account `api_key`)
//       -> build_provider_config_from_store
//         -> Orchestrator::stream_openai POST {base}/chat/completions
//           -> SSE deltas -> Tauri stream events -> Dioxus chat bubbles.
//
// The LLM is a loopback stub (permitted by `validate_base_url`'s loopback
// exception) that speaks just enough OpenAI SSE for the parser.
//
// Provider config is injected AT RUNTIME through the app's own `store_set`
// command — writing store.json on disk would race the app's in-memory copy
// (loaded at startup, re-persisted on shutdown). The `store_set` path also
// exercises the keyring write path the Settings UI uses.
//
// Safety: the spec snapshots the user's real keyring entry and the four
// `llm.*` store keys it touches, and restores everything in `after` —
// running the suite on a developer machine must not destroy real
// credentials or settings.
import { createServer } from 'node:http';
import { execFileSync } from 'node:child_process';

const KEYRING_SERVICE = 'athena';
const KEYRING_ACCOUNT = 'api_key';
const STUB_KEY = `sk-e2e-stub-${Date.now()}`;
const MARKER = `STUB_OK_${Date.now()}`;
// Legacy (unscoped) llm.* keys the stub config occupies during the test.
// `llm.api_key_status` is the "set"/"not_set" sentinel the UI probes; without
// deleting it, store_get short-circuits to a stale status from the real
// config before ever checking the keyring.
const TOUCHED_KEYS = ['llm.provider', 'llm.model', 'llm.base_url', 'llm.api_key', 'llm.api_key_status'];
// browser.execute NEVER awaits a returned Promise (WebDriver sync script
// serialization turns a Promise into `{}`), so Tauri invoke calls must be
// bridged: fire-and-forget in the page, park the result on localStorage,
// then poll for it synchronously. (Same pattern as kanban-persistence /
// settings-round-trip — those specs proved it in CI.)
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

function keyringGet() {
  try {
    return execFileSync('security', ['find-generic-password', '-s', KEYRING_SERVICE, '-a', KEYRING_ACCOUNT, '-w'], { encoding: 'utf8' }).trim();
  } catch {
    return null; // NoEntry
  }
}

function keyringSet(value) {
  execFileSync('security', ['add-generic-password', '-U', '-s', KEYRING_SERVICE, '-a', KEYRING_ACCOUNT, '-w', value]);
}

function keyringDelete() {
  try {
    execFileSync('security', ['delete-generic-password', '-s', KEYRING_SERVICE, '-a', KEYRING_ACCOUNT]);
  } catch { /* already gone */ }
}

describe('Athena chat against a stubbed OpenAI provider', () => {
  let stubServer;
  let stubPort;
  let capturedRequest = [];
  let prevKey = undefined;    // string | null
  let prevStoreValues = null; // { key: string | null }

  before(async () => {
    // Snapshot the legacy keyring entry BEFORE the test (or the app's
    // store_set) can overwrite it.
    prevKey = keyringGet();

    // Loopback OpenAI-compatible stub; capture every completion request so
    // the test can assert auth + model wiring.
    stubServer = createServer((req, res) => {
      if (req.method === 'POST' && req.url === '/v1/chat/completions') {
        let body = '';
        req.on('data', chunk => { body += chunk; });
        req.on('end', () => {
          capturedRequest.push({
            authorization: req.headers.authorization || '',
            body: JSON.parse(body || '{}'),
          });
          res.writeHead(200, { 'Content-Type': 'text/event-stream' });
          const chunk = delta => `data: ${JSON.stringify({ id: 'stub', object: 'chat.completion.chunk', choices: [{ index: 0, delta }] })}\n\n`;
          res.write(chunk({ role: 'assistant' }));
          res.write(chunk({ content: ` ${MARKER}` }));
          res.write(chunk({}));
          res.end('data: [DONE]\n\n');
        });
      } else {
        res.writeHead(404).end();
      }
    });
    await new Promise(resolve => stubServer.listen(0, '127.0.0.1', resolve));
    stubPort = stubServer.address().port;
  });

  after(async () => {
    // Restore the pre-test store values (best effort — app may be closing).
    if (prevStoreValues) {
      for (const [key, value] of Object.entries(prevStoreValues)) {
        try {
          if (value === null || key === 'llm.api_key' || key === 'llm.api_key_status') {
            // `llm.api_key` only ever holds the "set"/"not_set" sentinel —
            // the real secret lives in the keyring, restored below.
            await ipcInvoke('store_delete', { key });
          } else {
            await ipcInvoke('store_set', { key, value });
          }
        } catch { /* session may be gone already */ }
      }
      // Prove the restore actually landed: re-read every touched key through
      // the settled bridge and log the comparison. Silent restore failures
      // used to pass unnoticed because the unresolved-Promise snapshot was
      // written back as `{}` and nothing checked the result.
      try {
        const restored = {};
        for (const key of TOUCHED_KEYS) {
          const r = await ipcInvoke('store_get', { key });
          restored[key] = r.ok ? r.value : `(error: ${r.error})`;
        }
        console.log(`store restore round-trip: ${JSON.stringify(restored)}`);
      } catch { /* session may be gone already */ }
    }
    // Restore the real keyring entry (or remove the stub write).
    if (prevKey === null) keyringDelete();
    else if (prevKey !== undefined) keyringSet(prevKey);
    stubServer?.close();
  });

  it('streams a stubbed assistant reply into the chat log', async function () {
    this.timeout(120000);

    const clickButton = (text) => browser.execute(t => {
      for (const button of document.querySelectorAll('button')) {
        if ((button.textContent || '').trim().includes(t)) { button.click(); return true; }
      }
      return false;
    }, text);

    // The Athena toggle does not exist on the empty state, so we need a
    // workspace with an agent. Prior e2e runs persist `store.json`, so a
    // previous space may already exist — reuse it when possible, create one
    // otherwise (creation flow mirrors agent-athena-reference.e2e.mjs).
    await browser.execute(() => {
      window.__athenaE2E = true;
      window.__TAURI__.core.invoke('workspace_add_trusted_root', { dir: '/tmp' });
    });
    const hasPill = async () => browser.execute(() => !!document.querySelector('[data-agent-pill]'));
    if (!(await hasPill())) {
      // Try entering an existing space from the sidebar first.
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
      await browser.waitUntil(
        async () => browser.execute(() => !!document.getElementById('add-claude')),
        { timeout: 10000, interval: 250, timeoutMsg: 'Agent configuration step did not appear' },
      );
      await browser.execute(() => document.getElementById('add-claude')?.click());
      expect(await clickButton('Launch Space')).toBe(true);
    }
    await browser.waitUntil(hasPill, {
      timeout: 25000, interval: 500, timeoutMsg: 'Agent pane pill did not mount',
    });

    // Point the chat backend at the stub through the same command the
    // Settings UI uses. Snapshot the previous values first so `after` can
    // restore the user's real configuration.
    const storeGet = async (key) => {
      const r = await ipcInvoke('store_get', { key });
      return r.ok ? r.value : null;
    };
    const storeSet = async (key, value) => {
      const r = await ipcInvoke('store_set', { key, value });
      if (!r.ok) throw new Error(`store_set(${key}) failed: ${r.error}`);
    };
    prevStoreValues = {};
    for (const key of TOUCHED_KEYS) {
      prevStoreValues[key] = await storeGet(key);
    }
    await storeSet('llm.provider', 'custom'); // custom => legacy slots are authoritative
    await storeSet('llm.model', 'stub-model');
    await storeSet('llm.base_url', `http://127.0.0.1:${stubPort}/v1`);
    const delStatus = await ipcInvoke('store_delete', { key: 'llm.api_key_status' });
    if (!delStatus.ok) throw new Error(`store_delete(llm.api_key_status) failed: ${delStatus.error}`);
    await storeSet('llm.api_key', STUB_KEY);  // goes to the OS keyring

    // Open the Athena panel via its toggle, then wait for the composer.
    await browser.execute(() => document.querySelector('[data-athena-toggle]')?.click());
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('#athena-composer-input')),
      { timeout: 10000, timeoutMsg: 'Athena composer textarea did not mount (panel closed?)' },
    );

    // The placeholder must NOT say "Set an API key…" — that would mean the
    // keyring sentinel probe failed and the composer is blocked.
    const placeholder = await browser.execute(() => document.querySelector('#athena-composer-input')?.getAttribute('placeholder') || '');
    expect(placeholder).not.toContain('Set an API key');

    // Type via the native setter so Dioxus' controlled `value` stays in sync,
    // then dispatch a real Enter keydown to trigger submit.
    await browser.execute(text => {
      const textarea = document.querySelector('#athena-composer-input');
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
      setter.call(textarea, text);
      textarea.dispatchEvent(new Event('input', { bubbles: true }));
      textarea.focus();
    }, 'ping');
    await browser.execute(() => {
      document.querySelector('#athena-composer-input')?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Enter', bubbles: true, cancelable: true }),
      );
    });

    // The stub reply must render as an assistant bubble with the marker text.
    try {
      await browser.waitUntil(
        async () => browser.execute(marker => {
          return [...document.querySelectorAll('.athena-chat-row.is-assistant')]
            .some(row => (row.textContent || '').includes(marker));
        }, MARKER),
        { timeout: 30000, interval: 500, timeoutMsg: `assistant bubble with ${MARKER} never rendered` },
      );
    } catch (e) {
      const chatDump = await browser.execute(() => document.querySelector('.athena-message-log')?.innerText?.slice(0, 1500) || '(no message log)');
      console.log(`=== chat log dump (stub requests: ${capturedRequest.length}) ===\n${chatDump}\n===`);
      throw e;
    }

    // And the request that produced it went to the stub with the seeded
    // key/model — proving store+keyring -> orchestrator wiring end to end.
    // The orchestrator may fire a non-streaming title/summary call around the
    // chat call; assert auth on every captured request, and model/stream on
    // the chat one specifically.
    expect(capturedRequest.length).toBeGreaterThan(0);
    for (const req of capturedRequest) {
      expect(req.authorization).toBe(`Bearer ${STUB_KEY}`);
    }
    const chatCall = capturedRequest.find(r => r.body.stream === true);
    expect(chatCall).toBeTruthy(); // streaming chat completion call must exist
    expect(chatCall.body.model).toBe('stub-model');
  });
});
