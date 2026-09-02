// Settings persistence round-trip end-to-end.
//
// Proves the full settings tree is navigable and that a change made in the
// UI persists through the store contract and survives a modal re-open:
//
//   settings gear button -> SettingsContent (7 codex sections, floating index)
//     -> click every "Jump to section <I..VII>" entry (full tree walk)
//     -> Themes section (s-iv): click a ThemeSwatch
//       -> apply_theme_and_persist -> store_set('theme', <id>)
//     -> close the modal, re-open it ("launcher" round trip)
//       -> swatch shows the ACTIVE badge again
//     -> store_get('theme') still returns the new value.
//
// Safety: the previous `theme` store value is snapshotted in `before` and
// restored in `after` (store.json is shared across all serial specs).
//
// No workspace is required — the settings gear lives in the title bar and is
// always rendered.

// browser.execute NEVER awaits a returned Promise (WebDriver sync script
// serialization turns a Promise into `{}`), so Tauri invoke calls must be
// bridged: fire-and-forget in the page, park the result on `window`, then
// poll for it synchronously.
let ipcTokenCounter = 0;
async function ipcInvoke(cmd, args = {}) {
  const token = `__e2e_ipc_${Date.now()}_${ipcTokenCounter++}`;
  await browser.execute((t, c, a) => {
    // Park on localStorage, not window: the app may reload mid-bootstrap,
    // and individual `browser.execute` scripts do not share window state
    // reliably across the navigation.
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

// store_get rejects with CommandError::NotFound when the key is absent
// (KeyValueStore::get -> None) — map that rejection to null.
const storeGet = async (key) => (await ipcInvoke('store_get', { key })).value ?? null;
const storeSet = async (key, value) => ipcInvoke('store_set', { key, value });
const storeDelete = async (key) => ipcInvoke('store_delete', { key });

const THEMES = {
  nyx: 'Nyx',
  pentelic: 'Pentelic',
};

describe('Settings round-trip persistence', () => {
  let prevTheme; // string | null | undefined (undefined => probe failed)

  before(async () => {
    prevTheme = await storeGet('theme');
  });

  after(async () => {
    // Restore the pre-test theme (best effort; app may be closing).
    try {
      if (prevTheme === null || prevTheme === undefined) {
        await storeDelete('theme');
      } else {
        await storeSet('theme', prevTheme);
      }
    } catch { /* session may already be gone */ }
  });

  const openSettings = async () => {
    const clicked = await browser.execute(() => {
      const btn = document.querySelector('button[title="Settings (Cmd+,)"]');
      if (!btn) return false;
      btn.click();
      return true;
    });
    expect(clicked).toBe(true);
    await browser.waitUntil(
      async () => browser.execute(() => !!document.querySelector('.modal-overlay .modal-card')),
      { timeout: 10000, interval: 250, timeoutMsg: 'Settings modal did not open' },
    );
  };

  const closeSettings = async () => {
    await browser.execute(() =>
      document.querySelector('.modal-card button[aria-label="Close dialog"]')?.click());
    await browser.waitUntil(
      async () => browser.execute(() => !document.querySelector('.modal-overlay')),
      { timeout: 5000, interval: 250, timeoutMsg: 'Settings modal did not close' },
    );
  };

  // Select a swatch by its footer label ("Nyx", "Pentelic", ...) and report
  // whether the ACTIVE badge is rendered on it.
  const swatchState = (label) => browser.execute(l => {
    const btn = [...document.querySelectorAll('button.theme-swatch-btn')]
      .find(b => (b.querySelector('span')?.textContent || '').trim() === l);
    if (!btn) return { found: false, active: false };
    const active = [...btn.querySelectorAll('span')]
      .some(s => (s.textContent || '').trim() === 'ACTIVE');
    return { found: true, active };
  }, label);

  it('navigates the full settings tree and persists the theme change', async function () {
    this.timeout(60000);

    await openSettings();

    // --- Walk the entire settings tree via the floating index -------------
    const numerals = ['I', 'II', 'III', 'IV', 'V', 'VI', 'VII'];
    const sectionIds = ['s-i', 's-ii', 's-iii', 's-iv', 's-v', 's-vi', 's-vii'];
    for (const [i, numeral] of numerals.entries()) {
      const clicked = await browser.execute(n => {
        const btn = document.querySelector(`button[aria-label="Jump to section ${n}"]`);
        if (!btn) return false;
        setTimeout(() => btn.click(), 0);
        return true;
      }, numeral);
      expect(clicked).toBe(true);
      await browser.pause(300);
      const sectionExists = await browser.execute(
        id => !!document.getElementById(id), sectionIds[i]);
      expect(sectionExists).toBe(true);
    }

    // --- Change a persisted setting (theme) --------------------------------
    // Current theme: stored value, or the boot default "nyx" when unset.
    const currentTheme = await storeGet('theme') ?? 'nyx';
    const targetTheme = currentTheme === 'pentelic' ? 'nyx' : 'pentelic';
    const targetLabel = THEMES[targetTheme];

    const clickedSwatch = await browser.execute(l => {
      const btn = [...document.querySelectorAll('button.theme-swatch-btn')]
        .find(b => (b.querySelector('span')?.textContent || '').trim() === l);
      if (!btn) return false;
      btn.click();
      return true;
    }, targetLabel);
    expect(clickedSwatch).toBe(true);

    // apply_theme_and_persist spawns the store_set asynchronously — poll.
    await browser.waitUntil(
      async () => (await storeGet('theme')) === targetTheme,
      { timeout: 10000, interval: 300, timeoutMsg: `theme did not persist as ${targetTheme}` },
    );
    // In-modal feedback: the chosen swatch carries the ACTIVE badge.
    expect((await swatchState(targetLabel)).active).toBe(true);

    // --- Close and re-open: persisted value back-fills the UI -------------
    await closeSettings();
    await openSettings();
    await browser.waitUntil(
      async () => (await swatchState(targetLabel)).active,
      { timeout: 10000, interval: 300, timeoutMsg: `${targetLabel} swatch lost ACTIVE badge after re-open` },
    );
    // And the store contract still reports the round-tripped value.
    expect(await storeGet('theme')).toBe(targetTheme);

    await closeSettings();
  });
});
