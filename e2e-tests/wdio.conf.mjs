import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';

const __dirname = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(__dirname, '..');

export const config = {
  // tauri-wd listens on port 4444
  port: 4444,
  host: '127.0.0.1',

  // Run one app instance at a time: specs launched in parallel workers would
  // share the persisted `store.json` and clobber each other's workspace
  // state. Top-level so the local runner honors it (capability-level
  // maxInstances is not applied by the runner in WDIO 9).
  maxInstances: 1,

  capabilities: [{
    'tauri:options': {
      // Binary is at workspace root target/ (not src-tauri/target/).
      // Name has the "s": the Tauri app package produces `athenas-core`.
      binary: join(projectRoot, 'target', 'debug', 'athenas-core'),
    },
  }],

  specs: [join(__dirname, 'test', 'specs', '*.e2e.mjs')],

  logLevel: 'info',
  bail: 0,
  waitforTimeout: 10000,
  connectionRetryTimeout: 30000,
  connectionRetryCount: 3,

  reporters: ['spec'],

  framework: 'mocha',
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },

  // Runs in the launcher process BEFORE the app is started. `reset-store.mjs`
  // refuses to touch the store while the app runs (it holds the store open and
  // re-persists on shutdown), so the snapshot has to happen here, not in
  // `before` — which only runs once the app is already up. A run then starts
  // from a known empty state and `onComplete` puts the user's store back.
  onPrepare: () => {
    // A previous run may have been interrupted (Ctrl+C, crash, runner kill)
    // before its own onComplete sweep ran — clean its orphans first so the
    // new run starts at zero and the app spawn doesn't race a wedged app.
    // Best-effort like the onComplete sweep: cleanup must never be able to
    // abort the run it is preparing for.
    try {
      execFileSync('node', [join(__dirname, 'test', 'sweep-orphans.mjs')], {
        stdio: 'inherit',
      });
    } catch (err) {
      console.warn(`[wdio] pre-run orphan sweep skipped: ${err.message ?? err}`);
    }
    execFileSync('node', [join(__dirname, 'test', 'reset-store.mjs'), 'clear'], {
      stdio: 'inherit',
    });
  },

  before: async () => {
    mkdirSync(join(__dirname, 'test', 'screenshots'), { recursive: true });

    // Wait for Dioxus WASM to fully mount.
    // Checks for [data-dioxus-id] attribute (set by Dioxus on mount) or
    // any non-loader children in #main (fallback for different build modes).
    await browser.waitUntil(
      async () => {
        const mounted = await browser.execute(() => {
          const main = document.getElementById('main');
          if (!main) return false;
          if (main.querySelector('[data-dioxus-id]')) return true;
          for (const child of main.children) {
            if (child.id !== 'wasm-loading' && child.id !== 'console-log') return true;
          }
          return false;
        });
        return mounted;
      },
      { timeout: 25000, interval: 500, timeoutMsg: 'Dioxus WASM did not mount within 25s' },
    );
  },

  // Restore the user's persisted store after the app has been shut down by
  // the runner. `onComplete` runs after the worker/app teardown, so reset-store's
  // "app must not be running" guard is satisfied. Best-effort: a missing backup
  // (first-ever run) logs and exits non-zero without failing the suite.
  onComplete: () => {
    // The runner tears the app down with SIGTERM/SIGKILL rather than a
    // graceful quit, so pane shells can survive teardown. Sweep them (and
    // any omp agents they were running) before restoring the store.
    try {
      execFileSync('node', [join(__dirname, 'test', 'sweep-orphans.mjs')], {
        stdio: 'inherit',
      });
    } catch (err) {
      console.warn(`[wdio] orphan sweep skipped: ${err.message ?? err}`);
    }
    try {
      execFileSync('node', [join(__dirname, 'test', 'reset-store.mjs'), 'restore'], {
        stdio: 'inherit',
      });
    } catch (err) {
      console.warn(`[wdio] store restore skipped: ${err.message ?? err}`);
    }
  },
};
