import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = dirname(fileURLToPath(import.meta.url));
const screenshotDir = join(__dirname, '..', 'screenshots');

describe('Athena app launch', () => {
  it('loads the Dioxus WASM frontend and renders the empty state', async () => {
    const title = await browser.getTitle();
    expect(title).toContain("Athena's Core");

    await browser.saveScreenshot(join(screenshotDir, 'launch.png'));
  });

  it('renders the New Workspace button on the empty state', async () => {
    // The redesigned empty state renders the plus as an SVG icon (no literal
    // "+" text), so match the label text only.
    const btn = await $('button=New Workspace');
    await btn.waitForExist({ timeout: 15000 });
    const text = await btn.getText();
    expect(text).toContain('New Workspace');
  });

  it('clicks the New Workspace button and opens the workspace modal', async () => {
    // tauri-wd has a Node.contains compatibility issue with WDIO's isDisplayed check,
    // so we use executeScript to dispatch click events directly.
    const result = await browser.execute(() => {
      const btns = document.querySelectorAll('button');
      for (const btn of btns) {
        if (btn.textContent.includes('New Workspace')) {
          btn.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
          btn.dispatchEvent(new MouseEvent('mouseup', { bubbles: true, cancelable: true }));
          btn.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
          return 'clicked: ' + btn.textContent.trim();
        }
      }
      return 'not found';
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
