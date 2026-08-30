// E2E verification for the UX-audit fixes:
//   1. Kanban card delete now goes through a ConfirmDialog; Cancel must keep
//      the card, confirming must delete it.
//   2. Agent colors/labels come from the single canonical palette — the
//      New Workspace modal's Claude row must render #f97316 / "Claude"
//      (the old divergent palette used #d97706 / "Claude Code").
// Screenshots land in test/screenshots/verify-confirm-and-palette/.

import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mkdirSync, writeFileSync } from 'node:fs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const shotDir = join(__dirname, '..', 'screenshots', 'verify-confirm-and-palette')
mkdirSync(shotDir, { recursive: true })

async function clickButtonByText(text, { partial = false } = {}) {
  return browser.execute(
    ({ text, partial }) => {
      for (const btn of document.querySelectorAll('button')) {
        const content = (btn.textContent || '').trim()
        const matches = partial ? content.includes(text) : content === text
        if (!matches) continue
        btn.click()
        return { ok: true, content }
      }
      return { ok: false, text }
    },
    { text, partial },
  )
}

async function clickBySelector(selector) {
  return browser.execute((sel) => {
    const el = document.querySelector(sel)
    if (!el) return { ok: false }
    el.click()
    return { ok: true }
  }, selector)
}

async function waitFor(desc, fn, timeout = 20000) {
  await browser.waitUntil(
    async () => browser.execute(fn),
    { timeout, interval: 250, timeoutMsg: `Timeout waiting for ${desc}` },
  )
}

// Fire a Tauri invoke from the page's own JS context and capture the result
// on a window flag (promises aren't reliably awaited over tauri-wd).
async function invoke(cmd, args) {
  await browser.execute(
    ({ cmd, args }) => {
      window.__athenaInvoke = { pending: true }
      window.__TAURI__.core
        .invoke(cmd, args)
        .then((value) => {
          window.__athenaInvoke = { ok: true, value }
        })
        .catch((err) => {
          window.__athenaInvoke = { ok: false, err: String(err) }
        })
    },
    { cmd, args },
  )
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const s = window.__athenaInvoke
        return s && !s.pending
      }),
    { timeout: 10000, interval: 100, timeoutMsg: `Timeout waiting for invoke ${cmd}` },
  )
  const result = await browser.execute(() => window.__athenaInvoke)
  if (!result.ok) throw new Error(`invoke ${cmd} failed: ${result.err}`)
  return result.value
}

// DOM->SVG->canvas screenshot. Inlines stylesheet rules (the plugin's SVG
// pipeline cannot load external sheets) and copies form values. Strips
// animation declarations because SVG-as-image freezes at the 0% keyframe.
async function capturePage(name) {
  await browser.execute(() => {
    window.__captureState = { pending: true }
    try {
      const liveDoc = document
      const clone = liveDoc.documentElement.cloneNode(true)
      const css = []
      const ruleText = (rule) => {
        if (rule.type === 1) {
          const props = []
          for (const p of rule.style) {
            if (/^animation/.test(p)) continue
            props.push(`${p}: ${rule.style.getPropertyValue(p)};`)
          }
          return `${rule.selectorText} { ${props.join(' ')} }`
        }
        if (rule.cssRules) {
          const inner = []
          for (const child of rule.cssRules) inner.push(ruleText(child))
          return rule.constructor && rule.constructor.name === 'CSSMediaRule'
            ? `@media ${rule.conditionText} { ${inner.join(' ')} }`
            : rule.cssText
        }
        return rule.cssText
      }
      for (const sheet of liveDoc.styleSheets) {
        try {
          for (const rule of sheet.cssRules) css.push(ruleText(rule))
        } catch (e) {
          // cross-origin / restricted sheet — skip
        }
      }
      const styleEl = liveDoc.createElement('style')
      styleEl.textContent = css.join('\n')
      clone.insertBefore(styleEl, clone.firstChild)

      const liveForm = liveDoc.querySelectorAll('input, textarea, select')
      const cloneForm = clone.querySelectorAll('input, textarea, select')
      for (let i = 0; i < cloneForm.length && i < liveForm.length; i += 1) {
        const value = liveForm[i].value
        if (value !== undefined && value !== '') {
          cloneForm[i].setAttribute('value', String(value))
        }
      }

      const xml = new XMLSerializer().serializeToString(clone)
      const w = Math.max(
        liveDoc.documentElement.scrollWidth,
        liveDoc.documentElement.clientWidth,
      )
      const h = Math.max(
        liveDoc.documentElement.scrollHeight,
        liveDoc.documentElement.clientHeight,
      )
      const svg =
        '<svg xmlns="http://www.w3.org/2000/svg" width="' +
        w +
        '" height="' +
        h +
        '"><foreignObject width="100%" height="100%">' +
        xml +
        '</foreignObject></svg>'
      const c = liveDoc.createElement('canvas')
      c.width = w
      c.height = h
      const ctx = c.getContext('2d')
      const img = new Image()
      img.onload = function () {
        try {
          ctx.drawImage(img, 0, 0)
          for (const cv of liveDoc.querySelectorAll('canvas')) {
            try {
              if (!cv.width || !cv.height) continue
              const r = cv.getBoundingClientRect()
              if (r.width < 1 || r.height < 1) continue
              ctx.drawImage(cv, r.left, r.top, r.width, r.height)
            } catch (e) {}
          }
          for (const im of liveDoc.querySelectorAll('img')) {
            try {
              if (!im.complete || !im.naturalWidth) continue
              const r = im.getBoundingClientRect()
              if (r.width < 1 || r.height < 1) continue
              ctx.drawImage(im, r.left, r.top, r.width, r.height)
            } catch (e) {}
          }
          window.__captureState = {
            done: true,
            b64: c.toDataURL('image/png').split(',')[1],
          }
        } catch (err) {
          window.__captureState = { done: true, error: String(err) }
        }
      }
      img.onerror = function () {
        window.__captureState = { done: true, error: 'SVG render failed' }
      }
      img.src = 'data:image/svg+xml;charset=utf-8,' + encodeURIComponent(svg)
    } catch (err) {
      window.__captureState = { done: true, error: String(err) }
    }
  })
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const s = window.__captureState
        return s && s.done
      }),
    { timeout: 30000, interval: 100, timeoutMsg: `Timeout capturing ${name}` },
  )
  const result = await browser.execute(() => window.__captureState)
  if (!result.b64) throw new Error(`capture ${name} failed: ${result.error || 'no data'}`)
  writeFileSync(join(shotDir, name), Buffer.from(result.b64, 'base64'))
}

describe('Verify audit fixes — confirm dialog + unified palette', () => {
  it('kanban delete confirms and cancel keeps the card; Claude row is the unified color/label', async function () {
    this.timeout(300000)
    await browser.execute(() => {
      window.__athenaE2E = true
    })

    // ---------- Kanban: seed a uniquely-titled card ----------
    // The kanban store is in-memory per app process, so a tauri-wd session
    // death mid-test also resets the card. ensureSeededCard() re-seeds and
    // remounts as needed, and the whole click->dialog flow retries.
    const seedTitle = 'Verify dialog E2E card'
    const ensureSeededCard = async () => {
      await clickButtonByText('kanban')
      await waitFor('kanban board', () => !!document.querySelector('.kanban-board'))
      const hasCard = await browser.execute(() =>
        Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
          (c.textContent || '').includes('Verify dialog E2E'),
        ),
      )
      if (hasCard) return
      await invoke('kanban_create_task', { title: seedTitle, description: null })
      await browser.pause(500)
      // KanbanBoard loads tasks only on mount — remount via panel switch.
      await clickButtonByText('workspace')
      await browser.pause(700)
      await clickButtonByText('kanban')
      await waitFor('kanban columns', () =>
        document.querySelectorAll('.kanban-column').length === 4,
      )
      await waitFor('seeded card rendered', () =>
        Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
          (c.textContent || '').includes('Verify dialog E2E'),
        ),
      )
      await browser.pause(300)
    }

    // Capture page-level errors so the harness can tell a render panic
    // (app reload) from a silent no-op click.
    await browser.execute(() => {
      window.__verifyErrs = []
      window.addEventListener('error', (e) => window.__verifyErrs.push(String(e.message || e)))
      window.addEventListener('unhandledrejection', (e) => window.__verifyErrs.push('reject: ' + String(e.reason)))
    })

    // Click the card's delete button, then poll for the confirm dialog.
    // tauri-wd session drops reset the in-memory kanban store, so on a dead
    // session we reseed and try again (up to 3 attempts).
    let opened = null
    let clicked = false
    for (let attempt = 0; attempt < 3; attempt += 1) {
      await ensureSeededCard()
      clicked = await browser.execute(() => {
        const card = Array.from(document.querySelectorAll('.kanban-card')).find((c) =>
          (c.textContent || '').includes('Verify dialog E2E'),
        )
        const btn = card && card.querySelector('button[title="Delete"]')
        if (!btn) return false
        btn.click()
        return true
      })
      console.log('[VERIFY] click attempt', attempt, 'buttonFound', clicked)
      const deadline = Date.now() + 8000
      while (Date.now() < deadline) {
        opened = await browser.execute(() => ({
          dialogTextFound: document.body.textContent.includes('This cannot be undone'),
          modals: document.querySelectorAll('.modal-overlay').length,
          dioxusMounted: !!document.querySelector('[data-dioxus-id]'),
          verifyErrs: window.__verifyErrs || [],
          cardBack: Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
            (c.textContent || '').includes('Verify dialog E2E'),
          ),
        }))
        if (opened.dialogTextFound) break
        await browser.pause(400)
      }
      console.log('[VERIFY] poll result', attempt, JSON.stringify(opened))
      if (opened.dialogTextFound) break
      if (opened.dioxusMounted && opened.cardBack) {
        // App is healthy and the card exists but no dialog: assume the click
        // was dropped by the flaky session and retry after a pause.
        await browser.pause(2000)
      }
    }
    expect(clicked).toBe(true)
    expect(opened.dialogTextFound).toBe(true)
    const stillThere = await browser.execute(() =>
      Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
        (c.textContent || '').includes('Verify dialog E2E'),
      ),
    )
    expect(stillThere).toBe(true)
    await capturePage('01-kanban-confirm-open.png')

    // Cancel must keep the card.
    const cancelled = await browser.execute(() => {
      const btns = document.querySelectorAll('.modal-footer button')
      if (btns.length < 2) return false
      btns[0].click()
      return btns[0].textContent.trim()
    })
    expect(cancelled).toBe('Cancel')
    await waitFor('dialog dismissed', () =>
      !document.body.textContent.includes('This cannot be undone'),
    )
    const afterCancel = await browser.execute(() =>
      Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
        (c.textContent || '').includes('Verify dialog E2E'),
      ),
    )
    expect(afterCancel).toBe(true)

    // Confirming must delete.
    await browser.execute(() => {
      const card = Array.from(document.querySelectorAll('.kanban-card')).find((c) =>
        (c.textContent || '').includes('Verify dialog E2E'),
      )
      const btn = card && card.querySelector('button[title="Delete"]')
      if (btn) btn.click()
    })
    await waitFor('dialog re-opened', () =>
      document.body.textContent.includes('This cannot be undone'),
    )
    const confirmed = await browser.execute(() => {
      const btns = document.querySelectorAll('.modal-footer button')
      if (btns.length < 2) return false
      btns[1].click()
      return btns[1].textContent.trim()
    })
    expect(confirmed).toBe('Delete')
    await waitFor(
      'card deleted',
      () =>
        !Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
          (c.textContent || '').includes('Verify dialog E2E'),
        ),
      10000,
    )
    await capturePage('02-kanban-after-delete.png')

    // ---------- New-space modal: unified agent color + label ----------
    await clickBySelector('.workspace-tabs button[aria-label="New Workspace"]')
    await waitFor('new workspace modal', () =>
      Array.from(document.querySelectorAll('button')).some((b) =>
        (b.textContent || '').includes('Terminal Workspace'),
      ),
    )
    await clickButtonByText('Terminal Workspace', { partial: true })
    await clickButtonByText('Next >')
    await waitFor('agent config step', () => !!document.getElementById('add-claude'))

    // Bump Claude's count so its row dot turns from dim to the agent color.
    await browser.execute(() => {
      const btn = document.getElementById('add-claude')
      if (btn) btn.click()
    })
    await browser.pause(300)

    const paletteFacts = await browser.execute(() => {
      const btn = document.getElementById('add-claude')
      if (!btn) return { ok: false }
      // Row layout: row > [left(dot, label), right(-, count, +)].
      const row = btn.parentElement.parentElement
      const dot = row.children[0].children[0]
      return {
        ok: true,
        dotBg: getComputedStyle(dot).backgroundColor,
        label: row.children[0].children[1].textContent.trim(),
      }
    })
    expect(paletteFacts.ok).toBe(true)
    // #f97316 (Claude orange) — the old divergent palette used #d97706.
    expect(paletteFacts.dotBg).toBe('rgb(249, 115, 22)')
    // Short canonical label — the old palette rendered "Claude Code".
    expect(paletteFacts.label).toBe('Claude')
    await capturePage('03-new-space-unified-palette.png')

    // Close the modal so the app is left in a clean state.
    await browser.execute(() => {
      const x = document.querySelector('.modal-header button[aria-label="Close dialog"]')
      if (x) x.click()
    })
  })
})