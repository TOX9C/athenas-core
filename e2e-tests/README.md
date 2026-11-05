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
`reset-store.mjs` and `helpers/trust.mjs` did not exist before the harness (both
are tracked files now), so an older checkout has no store reset at all. Copying
them in makes it a current-harness run, not a baseline. Compare per-spec results against the untouched control
specs (`pty-broadcast`, `resume-*-capture`,
`verify-confirm-and-palette`, `zsh-zle-replay`) instead: they exercise identical
code in every run, so a failure there is environmental, not a regression.

## Store reset protocol

The suite resets state at two levels: once per run (the whole `store.json`) and
once per spec (the keys a spec can dirty).

**Per run.** `wdio.conf.mjs` wraps the run around the persisted store:

- `onPrepare` runs `test/reset-store.mjs clear` **before the app launches**.
  `clear` refuses while any `athenas-core` process is alive — the app holds
  `store.json` open and re-persists on shutdown, resurrecting anything you
  change. So the app must be **fully quit** before a run (a `before`-hook reset
  is too late: the hook runs after launch).
- `onComplete` runs `restore`, putting the newest backup back after teardown.

**Schema restore (teardown).** A `--features webdriver` build also rewrites three
TRACKED ACL schema files under `src-tauri/gen/schemas/` — the automation
plugin's permission surface (`webdriver-automation:*`) shows up in them, while
the committed set is the default-feature set. `onComplete` restores just that
directory (`git checkout -- src-tauri/gen/schemas`) so a local run cannot leak
the diff into a commit. It is deliberately path-scoped — a tree dirty for an
unrelated reason keeps everything outside that directory — and best-effort: a
failure only warns, because a teardown that can turn a green run red is worse
than the diff it cleans up. Running on `onComplete`, it does not fire on
SIGINT/SIGKILL. A warm cargo tree will not regenerate the schemas at all (the
build script is cached), so `touch src-tauri/build.rs` before the run to
reproduce the wart.

**Per spec.** The runner spawns **one app process per spec file** — verified by
sampling `pgrep -x athenas-core` across a two-spec run (two distinct PIDs,
sequential, no overlap), and WDIO starts a fresh session per spec file. So
in-memory state does *not* carry over: each spec gets a new app. What does carry
over is `store.json`, because every new app reads it at launch. `wdio.conf.mjs`'s
`before` hook therefore calls `resetAppState(browser)`
(`test/helpers/ipc.mjs`) before every spec:

1. `pty_kill` every id left in `window.__athenaTermMap` (errors from already-dead
   ids are tolerated; the return value counts the ones that died);
2. `store_delete` the two keys a spec can dirty — `workspaces`
   (`frontend/src/stores/workspace.rs:21`) and `workspace.trusted_roots`
   (`src-tauri/src/commands/mod.rs:266`);
3. it reloads **only if the page actually held state** (`pre.panes > 0`, or a
   dirty key present); on a fresh app — every spec, since the runner spawns one
   per spec file — there is nothing to drop, so no reload is issued and no
   navigation-teardown window exists. When a reload does happen it is keyed to a
   driver-visible signal: a token planted before `location.reload()` must be
   gone, i.e. the *new* document is answering. (The shim cannot wait for the
   navigation itself — `browser.refresh()` and `browser.url()` both return
   before the new document exists — so no driver command may be issued until the
   commit has passed: an eval delivered across the boundary runs in both
   documents and resolves the same pending script id twice, which panics the
   patched plugin and wedges the app. That bridge is driver-traffic-free.)
4. the `before` hook repeats the WASM-mount wait, now against the new document.

The hook logs its **measured** result per spec. `pre`/`post` are read with the
real `store_has` command and the page's `__athenaTermMap`, never derived from
what the reset did. `deletedKeys` is unconditional — `store_delete` returns `Ok`
for an absent key (`crates/athena-store/src/store.rs:267`) — so it is **not**
evidence of deletion; `pre`/`post` are:

```
[wdio] resetAppState -> {"pre":{"panes":0,"hasWorkspaces":false,"hasTrustedRoots":false},"post":{"panes":0,"hasWorkspaces":false,"hasTrustedRoots":false},"killedPtys":0,"deletedKeys":["workspaces","workspace.trusted_roots"],"reloaded":false}
```

`test/specs/state-isolation.e2e.mjs` is the committed witness: it populates a
space, asserts `pre.panes > 0` / `pre.hasWorkspaces`, then asserts the post-reset
`panes`/`hasWorkspaces`/`hasTrustedRoots` are all clear — and goes red if the
reset stops doing anything.

Why no seeding is needed: a spec's launch directory only has to be permitted,
and `.e2e-sandbox/<label>` under the repo root always is, because the app's own
project root is implicitly trusted (`effective_roots_from`,
`src-tauri/src/commands/mod.rs:352`). `workspace_add_trusted_root` is never
called — its only early return is membership in the persisted list, so its
native dialog (which no spec can service) would otherwise block. `e2eTrustRoot`
is an alias of `e2eSandboxDir`; neither writes the store.

Two consequences for authors:

- **Never use `clear` as a recovery step.** It *moves* the store aside (a
  timestamped backup, never overwritten). `backup` is the non-destructive
  snapshot; `restore` picks the newest backup — so take a `backup` before a
  session and run suites strictly sequentially, or `restore` returns the
  previous run's state rather than the user's original.
- Specs must not depend on user-persisted state; start from the cleared store,
  and rely on the per-spec reset rather than assuming a virgin app.

## Notes for authors

- **Serial only.** `maxInstances: 1` is deliberate: parallel workers would share
  the persisted `store.json` and clobber each other's workspace state.
- **WASM mount gate.** The global `before` hook waits up to 25 s for Dioxus to
  mount; specs can assume `[data-dioxus-id]` exists.
- **No skipping.** The suite is kept green — no `it.skip`/`describe.skip` in
  specs; quarantine a flaky test by fixing it, not by disabling it (the release
  roadmap tracks e2e gaps).
- **Trusted roots.** A permitted launch dir is required: the backend rejects
  `/tmp` and `$TMPDIR` (`is_forbidden_root`). Call `e2eTrustRoot(label)` from
  `test/helpers/trust.mjs`, then `trustViaProjectRoot(browser)` and
  `fillSpaceDir(browser, dir)` — see `swarm-lifecycle.e2e.mjs` for the full
  pattern. `e2eTrustRoot` only creates `.e2e-sandbox/<label>`: sandbox dirs live
  under the repo root, which the app's implicit project root authorizes, so no
  seeding call is needed and `workspace_add_trusted_root` (with its blocking
  native dialog) is never invoked. `__athenaE2E` is a frontend-only bypass and
  never authorizes a spawn.
- **No hardcoded checkout paths.** Derive every repo path at runtime —
  `repoRoot()` / `e2eSandboxDir(label)` from `test/helpers/repo.mjs`. Absolute
  owner-specific paths made the suite runnable from exactly one checkout.
- **One IPC bridge.** Write no new spec-visible copy of the invoke bridge —
  import `invokeSettled` / `resetAppState` from `test/helpers/ipc.mjs`. Three
  older specs still carry byte-equivalent private copies from before the helper
  existed (`mobile-relay-pairing.e2e.mjs:35-72`,
  `plugin-failure-paths.e2e.mjs:39-60`, `athena-chat-stub.e2e.mjs:43-64`); they
  are grandfathered — do not add a fourth.
- Screenshots land in `test/screenshots/`.

If you add a new spec, prefer an existing spec's boot helpers (e.g.
`athena-chat-stub.e2e.mjs` shows the LLM stub + `store_set` config-injection
pattern, with snapshot/restore of the user store and keyring).
