import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const screenshotDir = join(__dirname, '..', 'screenshots');

// Workspaces persist in the store across app launches (specs share one store),
// so on any run after the first the app restores prior workspaces and
// `active_space` is Some — the empty-state affordance (lib.rs, gated on
// `active_space.is_none()`) is never rendered. Never assume a global-fresh empty
// state: the sidebar's bottom "New Workspace" button renders regardless of
// whether spaces exist (sidebar.rs, no has_spaces gate), and it opens the same
// modal via the same `show_new_space_modal` signal. The sidebar starts collapsed
// (ui.rs sidebar_visible: false, UI state is not persisted), so expand it first.
async function ensureSidebarOpen() {
  return browser.execute(() => {
    if (document.querySelector('.sidebar')) return 'already-open';
    const expand = document.querySelector('button[title="Expand sidebar"]');
    if (!expand) return 'no-expand-button';
    expand.click();
    return 'expanded';
  });
}

async function waitForSidebarNewWorkspace(timeout = 15000) {
  return browser.waitUntil(
    async () =>
      browser.execute(() => {
        const sidebar = document.querySelector('.sidebar');
        if (!sidebar) return false;
        return Array.from(sidebar.querySelectorAll('button')).some(
          (btn) => (btn.textContent || '').trim() === 'New Workspace',
        );
      }),
    { timeout, interval: 250, timeoutMsg: 'sidebar New Workspace button never appeared' },
  );
}

describe('Athena app launch', () => {
  it('loads the Dioxus WASM frontend and renders the workspace shell', async () => {
    const title = await browser.getTitle();
    expect(title).toContain("Athena's Core");

    await browser.saveScreenshot(join(screenshotDir, 'launch.png'));
  });

  it('exposes the New Workspace affordance on launch', async () => {
    // Matches the sidebar's bottom action bar label; the empty-state variant
    // carries the same text but only renders when no workspace is active.
    await ensureSidebarOpen();
    await waitForSidebarNewWorkspace();

    const text = await browser.execute(() => {
      const sidebar = document.querySelector('.sidebar');
      const btn = Array.from(sidebar?.querySelectorAll('button') || []).find(
        (candidate) => (candidate.textContent || '').trim() === 'New Workspace',
      );
      return btn ? btn.textContent.trim() : '';
    });
    expect(text).toContain('New Workspace');
  });

  it('clicks the New Workspace button and opens the workspace modal', async () => {
    // tauri-wd has a Node.contains compatibility issue with WDIO's isDisplayed check,
    // so we use executeScript to dispatch click events directly.
    const result = await browser.execute(() => {
      const sidebar = document.querySelector('.sidebar');
      const btn = Array.from(sidebar?.querySelectorAll('button') || []).find(
        (candidate) => (candidate.textContent || '').trim() === 'New Workspace',
      );
      if (!btn) return 'not found';
      btn.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      btn.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, cancelable: true }));
      btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      return 'clicked: ' + btn.textContent.trim();
    });

    expect(result).toBe('clicked: New Workspace');
    await browser.pause(500);

    // The WASM event-handler panic (Dioxus 0.7 + WKWebView) is fixed upstream (v0.7.6)
    // and verified locally, so this is now a hard assertion: the modal must open.
    const state = await browser.execute(() => {
      const overlay = document.querySelector('.modal-overlay');
      const errEl = document.getElementById('wasm-loading-error');
      return {
        modalOpen: !!overlay,
        errorText: errEl?.textContent || '',
      };
    });

    await browser.saveScreenshot(join(screenshotDir, 'after-click.png'));
    expect(state.modalOpen).toBe(true);
    expect(state.errorText).toBe('');
  });
});
