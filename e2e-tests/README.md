# Athena's Core — E2E Tests

WebdriverIO specs that drive the real Tauri app via
[`tauri-wd`](https://v2.tauri.app/develop/tests/webdriver/) (macOS only — the app
ships for Apple Silicon and the PTY harness is macOS-specific).

## Prerequisites

1. **Install the WebDriver shim and driver (one time):**

   ```bash
   cargo install tauri-driver --locked
   ```

   Check in `src-tauri/tauri.conf.json` / the workspace Cargo files that the
   automation feature is compiled in — the repo patches provides
   `tauri-plugin-webdriver-automation` (see `patches/`).

2. **Install JS deps:**

   ```bash
   cd e2e-tests && npm install
   ```

3. **Build the frontend dist and debug binary** (the specs launch
   `target/debug/athenas-core` from the workspace root):

   ```bash
   bash frontend/build-dist.sh
   cargo build --manifest-path src-tauri/Cargo.toml
   ```

4. **Start the driver** (listens on port 4444; `wdio.conf.mjs` targets it):

   ```bash
   tauri-wd &
   ```

   Without this step `npm run test:e2e` fails with
   `Unable to connect to localhost:4444` — that is an environment setup issue,
   not a test failure.

## Running

From the repo root:

```bash
npm run test:e2e          # all specs (21 specs, serial — maxInstances: 1)
```

Or from `e2e-tests/`:

```bash
npm test                                # all specs
npm run test:headed                     # visible window (WDIO_HEADED=1)
npm run test:soak                       # release soak (SOAK_ITERATIONS=10 default)
npm run test:metrics                    # perf metrics spec only
wdio run wdio.conf.mjs --spec test/specs/<name>.e2e.mjs   # single spec
```

## Notes for authors

- **Serial only.** `maxInstances: 1` is deliberate: parallel workers would share
  the persisted `store.json` and clobber each other's workspace state.
- **WASM mount gate.** The global `before` hook waits up to 25 s for Dioxus to
  mount; specs can assume `[data-dioxus-id]` exists.
- **No skipping.** The suite is kept green — no `it.skip`/`describe.skip` in
  specs; quarantine a flaky test by fixing it, not by disabling it (the release
  roadmap tracks e2e gaps).
- Screenshots land in `test/screenshots/`.

If you add a new spec, prefer an existing spec's boot helpers (e.g.
`athena-chat-stub.e2e.mjs` shows the LLM stub + `store_set` config-injection
pattern, with snapshot/restore of the user store and keyring).
