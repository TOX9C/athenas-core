import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { mkdirSync } from 'node:fs';

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
      // Binary is at workspace root target/ (not src-tauri/target/)
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
};
