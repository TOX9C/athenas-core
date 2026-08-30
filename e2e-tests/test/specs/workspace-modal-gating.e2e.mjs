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

describe('workspace modal gating', () => {
  before(async () => {
    // Wait for app + Dioxus mount.
    await browser.pause(5000)
    // Skip modal validation (E2E flag honored by the app).
    await browser.execute(() => {
      window.__athenaE2E = true
    })
    // Open modal and pick the Terminal Workspace card (not "Launch Space").
    const opened = await browser.execute(() => {
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.includes('New Workspace')) btn.click()
      }
      for (const btn of document.querySelectorAll('button')) {
        if (btn.textContent.includes('Terminal Workspace') && !btn.textContent.includes('Launch')) {
          btn.click()
          return true
        }
      }
      return false
    })
    expect(opened).toBe(true)
    await browser.pause(1200)
  })

  it('blocks Next while Working Directory is empty', async () => {
    const step1 = await browser.execute(() => {
      const nextBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Next >',
      )
      if (!nextBtn) return { ok: false, reason: 'next_missing' }
      const style = nextBtn.getAttribute('style') || ''
      const onStep1 = () =>
        !![...document.querySelectorAll('label')].find((l) =>
          l.textContent.includes('Working Directory'),
        )
      nextBtn.click()
      return {
        ok: true,
        blockedAffordance: style.includes('not-allowed') || style.includes('var(--bgTertiary)'),
        stayedOnStep1: onStep1() && !document.getElementById('add-shell'),
      }
    })
    expect(step1.ok, 'Next > button missing on Step 1').toBe(true)
    expect(step1.blockedAffordance, 'Next > lacks blocked affordance while empty').toBe(true)
    expect(step1.stayedOnStep1, 'Next > advanced despite empty required field').toBe(true)
  })

  it('advances to pane selection after a valid Working Directory', async () => {
    const step1 = await browser.execute(() => {
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
      const style = nextBtn.getAttribute('style') || ''
      nextBtn.click()
      return {
        ok: true,
        enabledAffordance: !style.includes('not-allowed'),
        advancedToStep2: !!document.getElementById('add-shell'),
      }
    })
    expect(step1.ok, 'could not set Working Directory or find Next').toBe(true)
    expect(step1.enabledAffordance, 'Next > stayed blocked after valid input').toBe(true)
    expect(step1.advancedToStep2, 'Next > did not advance after valid input').toBe(true)
    await browser.pause(800)
  })

  it('blocks Launch Space at 0 agents', async () => {
    const step2 = await browser.execute(() => {
      const launchBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Launch Space',
      )
      if (!launchBtn) return { ok: false, reason: 'launch_missing' }
      const style = launchBtn.getAttribute('style') || ''
      const hasZeroSummary = [...document.querySelectorAll('label, div, span')].some((el) =>
        (el.textContent || '').includes('Agents (0/16)'),
      )
      launchBtn.click()
      return {
        ok: true,
        blockedAffordance: style.includes('not-allowed') || style.includes('var(--bgTertiary)'),
        remainedOnStep2: !!document.getElementById('add-shell'),
        hasZeroSummary,
      }
    })
    expect(step2.ok, 'Launch Space button missing on Step 2').toBe(true)
    expect(step2.blockedAffordance, 'Launch Space lacks blocked affordance at 0 agents').toBe(true)
    expect(step2.remainedOnStep2, 'Launch Space launched despite 0 agents').toBe(true)
    expect(step2.hasZeroSummary, 'expected Agents (0/16) summary').toBe(true)
  })

  it('enables Launch Space after adding one pane', async () => {
    const step2 = await browser.execute(() => {
      const plusBtn = document.getElementById('add-shell')
      const launchBtn = [...document.querySelectorAll('button')].find(
        (b) => b.textContent.trim() === 'Launch Space',
      )
      if (!plusBtn || !launchBtn) return { ok: false, reason: 'missing_plus_or_launch' }
      plusBtn.click()
      const style = launchBtn.getAttribute('style') || ''
      const hasOneSummary = [...document.querySelectorAll('label, div, span')].some((el) =>
        (el.textContent || '').includes('Agents (1/16)'),
      )
      return { ok: true, enabledAffordance: !style.includes('not-allowed'), hasOneSummary }
    })
    expect(step2.ok, 'could not increment agent or read launch button').toBe(true)
    expect(step2.enabledAffordance, 'Launch Space stayed blocked after increment').toBe(true)
    expect(step2.hasOneSummary, 'expected Agents (1/16) after increment').toBe(true)
  })
})
