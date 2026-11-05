// Ported from e2e-tests/test_new_workspace_terminal_modal_states.mjs (deleted 2026-08-24).
//
// Contract under defense: the Terminal Workspace wizard gates advancement —
// Step 1 "Next >" is blocked (and stays on Step 1) while Working Directory is
// empty, advances once a valid path is entered; Step 2 "Launch Space" is
// blocked at 0 agents (shows "Agents (0/16)") and becomes enabled after
// adding one pane.

const clickFirst = (predicate) => {
  for (const btn of document.querySelectorAll('button')) {
    if (predicate(btn.textContent.trim())) {
      btn.click()
      return true
    }
  }
  return false
}

// The 'New Workspace' entry point depends on app state this spec must not
// depend on (store.json is shared by every serial spec): with persisted
// spaces the empty-state button is gone and the only trigger is the
// WorkspaceTabs icon button, which has aria-label but no text. Match either
// and wait for it to render instead of clicking blind.
async function openWorkspaceModal() {
  return browser.waitUntil(
    () =>
      browser.execute(() => {
        for (const el of document.querySelectorAll('[aria-label="New Workspace"]')) {
          el.click()
          return true
        }
        for (const btn of document.querySelectorAll('button')) {
          if ((btn.textContent || '').includes('New Workspace')) {
            btn.click()
            return true
          }
        }
        return false
      }),
    { timeout: 15000, interval: 250, timeoutMsg: 'no New Workspace entry point rendered' },
  )
}

describe('workspace modal gating', () => {
  before(async () => {
    // Wait for app + Dioxus mount.
    await browser.pause(5000)
    // NO `window.__athenaE2E` here on purpose. The app honors that flag by
    // disabling the modal's validation (`next_disabled = !is_e2e && …`,
    // new_space_modal.rs:141) — i.e. it turns off the exact gate this spec
    // exists to defend. Specs that need to skip validation (they launch a
    // space) set it; a spec that asserts the gate must not.
    //
    // Open modal and pick the Terminal Workspace card. The card is rendered
    // by the modal, so wait for it instead of probing the same tick as the
    // click (the modal does not mount synchronously).
    //
    // Match on the card's own title only. The old predicate also required
    // `!text.includes('Launch')` to avoid stepping on the "Launch Space"
    // footer button, but the card's subtitle ("Launch multiple terminal
    // panes …") contains that word too, so the filter excluded the card it
    // was looking for and the wait timed out. "Launch Space" never contains
    // "Terminal Workspace", so no exclusion is needed.
    await openWorkspaceModal()
    const opened = await browser.waitUntil(
      () =>
        browser.execute(() => {
          for (const btn of document.querySelectorAll('button')) {
            const text = btn.textContent || ''
            if (text.includes('Terminal Workspace')) {
              btn.click()
              return true
            }
          }
          return false
        }),
      { timeout: 15000, interval: 250, timeoutMsg: 'Terminal Workspace card never rendered' },
    )
    expect(opened).toBe(true)
    await browser.pause(1200)
  })

  // The app expresses "blocked" with the button's HTML `disabled` attribute
  // (`disabled: next_disabled`, new_space_modal.rs:147) plus the shared
  // `button:disabled` CSS rule. That replaced the old inline
  // `opacity: 0.5`/`cursor: not-allowed` styles in 415c55b, so probing the
  // inline `style` attribute can only ever report "not blocked". Every probe
  // below therefore reads the `disabled` state instead.
  it('blocks Next while Working Directory is empty', async () => {
    const step1 = await browser.execute(() => {
      const nextBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Next >',
      )
      if (!nextBtn) return { ok: false, reason: 'next_missing' }
      const blocked = nextBtn.disabled || nextBtn.hasAttribute('disabled')
      nextBtn.click()
      return { ok: true, blockedAffordance: blocked }
    })
    expect(step1.ok).toBe(true)
    expect(step1.blockedAffordance).toBe(true)

    // Give a would-be advance a full frame to land, then assert it did not.
    await browser.pause(600)
    const stayed = await browser.execute(() => ({
      onStep1: !![...document.querySelectorAll('label')].find((l) =>
        l.textContent.includes('Working Directory'),
      ),
      addShell: !!document.getElementById('add-shell'),
    }))
    expect(stayed.onStep1 && !stayed.addShell).toBe(true)
  })

  it('advances to pane selection after a valid Working Directory', async () => {
    const filled = await browser.execute(() => {
      const dirInput = [...document.querySelectorAll('input')].find((i) =>
        (i.getAttribute('placeholder') || '').includes('/path/to/project'),
      )
      const nextBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Next >',
      )
      if (!dirInput || !nextBtn) return { ok: false, reason: 'missing_input_or_next' }
      dirInput.value = '/tmp'
      dirInput.dispatchEvent(new Event('input', { bubbles: true }))
      dirInput.dispatchEvent(new Event('change', { bubbles: true }))
      return { ok: true }
    })
    expect(filled.ok).toBe(true)

    // The controlled input's signal write lands on the next render, so the
    // unblock and the advance are each waited for rather than probed in the
    // same tick as the write/click.
    const enabled = await browser
      .waitUntil(
        () =>
          browser.execute(() => {
            const nextBtn = [...document.querySelectorAll('button')].find(
              (b) => b.textContent.trim() === 'Next >',
            )
            return !!nextBtn && !nextBtn.disabled
          }),
        { timeout: 5000, interval: 100, timeoutMsg: 'Next > stayed blocked after valid input' },
      )
      .then(() => true)
      .catch(() => false)
    expect(enabled).toBe(true)

    await browser.execute(() => {
      const nextBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Next >',
      )
      nextBtn.click()
    })
    const advanced = await browser
      .waitUntil(
        () => browser.execute(() => !!document.getElementById('add-shell')),
        { timeout: 5000, interval: 100, timeoutMsg: 'Next > did not advance after valid input' },
      )
      .then(() => true)
      .catch(() => false)
    expect(advanced).toBe(true)
  })

  it('blocks Launch Space at 0 agents', async () => {
    const step2 = await browser.execute(() => {
      const launchBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Launch Space',
      )
      if (!launchBtn) return { ok: false, reason: 'launch_missing' }
      const blocked = launchBtn.disabled || launchBtn.hasAttribute('disabled')
      const hasZeroSummary = [...document.querySelectorAll('label, div, span')].some((el) =>
        (el.textContent || '').includes('Agents (0/16)'),
      )
      launchBtn.click()
      return { ok: true, blockedAffordance: blocked, hasZeroSummary }
    })
    expect(step2.ok).toBe(true)
    expect(step2.blockedAffordance).toBe(true)
    expect(step2.hasZeroSummary).toBe(true)

    // The click must not have launched: still on Step 2 after a settle frame.
    await browser.pause(600)
    expect(await browser.execute(() => !!document.getElementById('add-shell'))).toBe(true)
  })

  it('enables Launch Space after adding one pane', async () => {
    const clicked = await browser.execute(() => {
      const plusBtn = document.getElementById('add-shell')
      if (!plusBtn) return false
      plusBtn.click()
      return true
    })
    expect(clicked).toBe(true)

    // The increment re-renders on the next frame; wait for the unblock before
    // reading the summary so the assertion observes settled state.
    const enabled = await browser
      .waitUntil(
        () =>
          browser.execute(() => {
            const launchBtn = [...document.querySelectorAll('button')].find(
              (b) => b.textContent.trim() === 'Launch Space',
            )
            return !!launchBtn && !launchBtn.disabled
          }),
        { timeout: 5000, interval: 100, timeoutMsg: 'Launch Space stayed blocked after increment' },
      )
      .then(() => true)
      .catch(() => false)
    expect(enabled).toBe(true)
    const hasOneSummary = await browser.execute(() =>
      [...document.querySelectorAll('label, div, span')].some((el) =>
        (el.textContent || '').includes('Agents (1/16)'),
      ),
    )
    expect(hasOneSummary).toBe(true)
  })
})
