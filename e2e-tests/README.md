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
   cargo build --manifest-path src-tauri/Cargo.toml --features webdriver
   ```

4. **Start the driver** (listens on port 4444; `wdio.conf.mjs` targets it):

   ```bash
   tauri-wd &
   ```

   Without this step `npm run test:e2e` fails with
   `Unable to connect to localhost:4444` — that is an environment setup issue,
   not a test failure.

   The driver binary must also be built with `--features webdriver` (step 3).
   Without it the app registers no WebDriver endpoint, and every spec fails
   with `WebDriverError: ... POST http://localhost:4444/session` timing out
   after 30s. The `web content process terminated` line in the app log is
   unrelated Wry teardown noise.

## Running

From the repo root:

```bash
npm run test:e2e          # all specs (31 specs, serial — maxInstances: 1)
```

Or from `e2e-tests/`:

```bash
npm test                                # all specs
npm run test:headed                     # visible window (WDIO_HEADED=1)
npm run test:soak                       # release soak (SOAK_ITERATIONS=10 default)
npm run test:metrics                    # perf metrics spec only
wdio run wdio.conf.mjs --spec test/specs/<name>.e2e.mjs   # single spec
```

**Capturing results.** `bail: 0` means one whole-suite run already reports
every failing spec in a single app session. Capture it with `tee` rather than
eyeballing scrollback:

```bash
cd e2e-tests && npx wdio run wdio.conf.mjs 2>&1 | tee /tmp/e2e-head.txt
```

Per-spec runs (`--spec` × N) are for debugging one spec, not for comparison:
they cost N app boots, and a hard app crash mid-suite refuses the next run's
store clear on a live `pgrep` match, cascading failures.

A **baseline comparison run is not possible** against a pre-harness commit:
`reset-store.mjs` and `helpers/trust.mjs` are untracked additions, so an older
checkout has no store reset at all. Copying them in makes it a current-harness
run, not a baseline. Compare per-spec results against the untouched control
specs (`pty-broadcast`, `resume-*-capture`,
`verify-confirm-and-palette`, `zsh-zle-replay`) instead: they exercise identical
code in every run, so a failure there is environmental, not a regression.

## Store reset protocol

`wdio.conf.mjs` resets the persisted store around every run:

- `onPrepare` runs `test/reset-store.mjs clear` **before the app launches**.
  `clear` refuses while any `athenas-core` process is alive — the app holds
  `store.json` open and re-persists on shutdown, resurrecting anything you
  change. So the app must be **fully quit** before a run (a `before`-hook reset
  is too late: the hook runs after launch).
- `onComplete` runs `restore`, putting the newest backup back after teardown.

Two consequences for authors:

- **Never use `clear` as a recovery step.** It *moves* the store aside (a
  timestamped backup, never overwritten). `backup` is the non-destructive
  snapshot; `restore` picks the newest backup — so take a `backup` before a
  session and run suites strictly sequentially, or `restore` returns the
  previous run's state rather than the user's original.
- Specs must not depend on user-persisted state; start from the cleared store.

## Notes for authors

- **Serial only.** `maxInstances: 1` is deliberate: parallel workers would share
  the persisted `store.json` and clobber each other's workspace state.
- **WASM mount gate.** The global `before` hook waits up to 25 s for Dioxus to
  mount; specs can assume `[data-dioxus-id]` exists.
- **No skipping.** The suite is kept green — no `it.skip`/`describe.skip` in
  specs; quarantine a flaky test by fixing it, not by disabling it (the release
  roadmap tracks e2e gaps).
- **Trusted roots.** A permitted launch dir is required: the backend rejects
  `/tmp` and `$TMPDIR` (`is_forbidden_root`). Use `e2eTrustRoot(label)` +
  `seedTrustedRoots(browser, [dir])` from `test/helpers/trust.mjs`, then
  `fillSpaceDir(browser, dir)` — see `swarm-lifecycle.e2e.mjs` for the full
  pattern. `__athenaE2E` is a frontend-only bypass and never authorizes a spawn.
- Screenshots land in `test/screenshots/`.

If you add a new spec, prefer an existing spec's boot helpers (e.g.
`athena-chat-stub.e2e.mjs` shows the LLM stub + `store_set` config-injection
pattern, with snapshot/restore of the user store and keyring).
