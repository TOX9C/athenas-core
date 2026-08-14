import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'
import { mkdirSync, writeFileSync } from 'node:fs'

const __dirname = dirname(fileURLToPath(import.meta.url))
const shotDir = join(__dirname, '..', 'screenshots', 'ad')
mkdirSync(shotDir, { recursive: true })

const REPO_DIR = '/Users/apollo/Documents/athenas-core'

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

async function setField(selector, value) {
  return browser.execute(
    ({ selector, value }) => {
      const field = document.querySelector(selector)
      if (!field) return false
      const setter = Object.getOwnPropertyDescriptor(
        field.constructor.prototype,
        'value',
      )?.set
      setter?.call(field, value)
      field.dispatchEvent(new Event('input', { bubbles: true }))
      field.dispatchEvent(new Event('change', { bubbles: true }))
      return true
    },
    { selector, value },
  )
}

// Page capture that works around the tauri-wd /screenshot pipeline's
// limitations. The plugin serializes the DOM into an SVG <foreignObject>
// loaded from a data: URL, which (a) cannot load external stylesheets such as
// styles.css, so class-based styles fall back to UA defaults (icon buttons
// render as white Aqua boxes), (b) never serializes <canvas> bitmaps, so the
// xterm panes render blank, and (c) does not serialize property-set form
// values. This helper re-renders the same page through the same SVG pipeline
// but with all stylesheet rules inlined into the clone, every <canvas>
// replaced by a data-URL <img> of its live bitmap, and input/textarea/select
// values copied into the clone as attributes. Promises are not reliably
// awaited over tauri-wd, so we poll a window flag like the invoke() helper.
async function capturePage(name) {
  await browser.execute(() => {
    window.__captureState = { pending: true }
    try {
      const liveDoc = document
      const clone = liveDoc.documentElement.cloneNode(true)

      // 1. Inline every stylesheet rule so class-based styling survives the
      // data-URL SVG image render (external <link> sheets cannot load there).
      // Only the `animation*` declarations are stripped — dropping whole rules
      // would also remove layout properties (e.g. .panel-enter's display:flex),
      // and in SVG-as-image mode animations freeze at their 0% keyframe, so
      // entrance animations (.reveal-2 etc.) would render the app invisible.
      const css = []
      const ruleText = (rule) => {
        // Style rules first: this WebKit exposes a truthy .cssRules on
        // CSSStyleRule too, so type must be checked before .cssRules.
        if (rule.type === 1) {
          const props = []
          for (const p of rule.style) {
            // WebKit iterates longhands only (animation-name, -duration, ...)
            if (/^animation/.test(p)) continue
            props.push(`${p}: ${rule.style.getPropertyValue(p)};`)
          }
          return `${rule.selectorText} { ${props.join(' ')} }`
        }
        if (rule.cssRules) {
          // @media / container blocks: rebuild from children.
          const inner = []
          for (const child of rule.cssRules) inner.push(ruleText(child))
          return rule.constructor && rule.constructor.name === 'CSSMediaRule'
            ? `@media ${rule.conditionText} { ${inner.join(' ')} }`
            : rule.cssText
        }
        return rule.cssText // non-style rules (@font-face, @keyframes, ...)
      }
      for (const sheet of liveDoc.styleSheets) {
        try {
          for (const rule of sheet.cssRules) css.push(ruleText(rule))
        } catch (e) {
          // Cross-origin / restricted sheet — skip (mirrors live rendering).
        }
      }
      const styleEl = liveDoc.createElement('style')
      styleEl.textContent = css.join('\n')
      clone.insertBefore(styleEl, clone.firstChild)

      // 2. Copy form element values into the clone (property-set values are
      // not serialized by XMLSerializer).
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
          // 3. Overlay live <canvas> bitmaps (xterm terminals) directly — the
          // serialized SVG cannot carry canvas pixels, and nested data-URL
          // images are blocked in SVG-as-image mode.
          for (const cv of liveDoc.querySelectorAll('canvas')) {
            try {
              if (!cv.width || !cv.height) continue
              const r = cv.getBoundingClientRect()
              if (r.width < 1 || r.height < 1) continue
              ctx.drawImage(cv, r.left, r.top, r.width, r.height)
            } catch (e) {}
          }
          // 4. Same for loaded <img> elements (their relative src cannot
          // resolve inside the data-URL SVG).
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

// Fire a Tauri invoke from the page's own JS context and capture the result
// on a window flag. Promises returned through `browser.execute` are not
// reliably awaited over tauri-wd, so we poll the flag instead (the same
// pattern agent-badges.e2e.mjs uses for pty writes).
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

// Focus pane i's xterm hidden textarea so keystrokes land in that shell.
async function focusPane(i) {
  return browser.execute((idx) => {
    const mounts = document.querySelectorAll('.xterm-mount')
    const mount = mounts[idx]
    if (!mount) return { ok: false, count: mounts.length }
    const textarea = mount.querySelector('textarea')
    if (textarea) textarea.focus()
    const xterm = mount.querySelector('.xterm')
    if (xterm) xterm.click()
    return { ok: true, count: mounts.length }
  }, i)
}

async function typeInPane(i, text) {
  await focusPane(i)
  await browser.pause(300)
  await browser.keys(text)
  await browser.keys(['\uE007']) // Enter
}

async function addKanbanTasks(columnIndex, titles) {
  for (const title of titles) {
    const ok = await browser.execute(
      ({ col, title }) => {
        const colEl = document.querySelectorAll('.kanban-column')[col]
        if (!colEl) return { ok: false, why: 'no column' }
        // Target the column's own add-task row — a generic 'input'/'icon-btn'
        // query picks the first card's edit field/button once cards exist.
        const input = colEl.querySelector('input[placeholder="Add task..."]')
        if (!input) return { ok: false, why: 'no add input' }
        const setter = Object.getOwnPropertyDescriptor(
          input.constructor.prototype,
          'value',
        )?.set
        setter?.call(input, title)
        input.dispatchEvent(new Event('input', { bubbles: true }))
        const addBtn = colEl.querySelector('button[title="Add task"]')
        if (!addBtn) return { ok: false, why: 'no add button' }
        addBtn.click()
        return { ok: true }
      },
      { col: columnIndex, title },
    )
    expect(ok.ok).toBe(true)
    // Wait until the card actually appears in this column before the next
    // add — the column input resets when the previous task's reload lands,
    // so racing adds get dropped.
    await browser.waitUntil(
      async () =>
        browser.execute(
          ({ col, title }) => {
            const colEl = document.querySelectorAll('.kanban-column')[col]
            if (!colEl) return false
            return Array.from(colEl.querySelectorAll('.kanban-card')).some((c) =>
              (c.textContent || '').includes(title),
            )
          },
          { col: columnIndex, title },
        ),
      {
        timeout: 8000,
        interval: 250,
        timeoutMsg: `Kanban card "${title}" never appeared in column ${columnIndex}`,
      },
    )
    await browser.pause(250)
  }
}

describe('Ad capture — richer real app state', () => {
  it('captures a 5-pane workspace, live swarm mission and populated kanban', async function () {
    this.timeout(300000)

    // E2E flag bypasses the modal's required-directory validation.
    await browser.execute(() => {
      window.__athenaE2E = true
    })
    await browser.pause(2500)

    // 01 — Welcome / empty state.
    await capturePage('01-welcome.png')

    // ---------- Terminal workspace ----------
    const openResult = await clickButtonByText('New Workspace', { partial: true })
    expect(openResult.ok).toBe(true)
    await waitFor('new space modal', () =>
      Array.from(document.querySelectorAll('button')).some((b) =>
        (b.textContent || '').includes('Terminal Workspace'),
      ),
    )
    await clickButtonByText('Terminal Workspace', { partial: true })

    await waitFor('directory input', () => !!document.querySelector('input[placeholder="/path/to/project"]'))
    const filled = await browser.execute(
      ({ name, dir }) => {
        const nameInput = document.querySelector('input[placeholder="my-project"]')
        const dirInput = document.querySelector('input[placeholder="/path/to/project"]')
        if (!nameInput || !dirInput) return { ok: false }
        const setVal = (el, value) => {
          const proto = Object.getPrototypeOf(el)
          const setter = Object.getOwnPropertyDescriptor(proto, 'value').set
          setter.call(el, value)
          el.dispatchEvent(new Event('input', { bubbles: true }))
        }
        setVal(nameInput, name)
        setVal(dirInput, dir)
        return { ok: true }
      },
      { name: 'athenas-core', dir: REPO_DIR },
    )
    expect(filled.ok).toBe(true)
    await browser.pause(400)

    // 02 — Workspace setup dialog (name + working directory).
    await capturePage('02-new-space-dialog.png')

    await clickButtonByText('Next >')
    await waitFor('add-shell button', () => !!document.getElementById('add-shell'))

    // Queue 3 shell panes in the launch config.
    await browser.execute(() => {
      const btn = document.getElementById('add-shell')
      for (let i = 0; i < 3; i += 1) btn.click()
      return { ok: true }
    })
    await browser.pause(600)

    // 03 — Launch configuration with 3 shells queued.
    await capturePage('03-launch-config.png')

    await clickButtonByText('Launch Space')
    await waitFor(
      '3 mounted xterm panes',
      () => {
        const mounts = Array.from(document.querySelectorAll('.xterm-mount'))
        return mounts.length >= 3 && mounts.every((m) => !!m.querySelector('.xterm'))
      },
      25000,
    )

    // Add two more shell panes from the titlebar for a 5-pane layout.
    for (let i = 0; i < 2; i += 1) {
      expect((await clickBySelector('button[title*="Add Shell"]')).ok).toBe(true)
      const target = 4 + i
      await browser.waitUntil(
        async () =>
          browser.execute((t) => document.querySelectorAll('.xterm-mount').length >= t, target),
        { timeout: 20000, interval: 250, timeoutMsg: `Timeout waiting for ${target} panes` },
      )
      await browser.pause(1200)
    }

    // Let every shell print its prompt.
    await browser.pause(3500)

    // Run real commands so the panes show genuine work.
    await typeInPane(0, 'git status -sb')
    await browser.pause(2600)
    await typeInPane(1, 'ls -la | head -24')
    await browser.pause(2600)
    await typeInPane(2, 'cargo --version && rustc --version')
    await browser.pause(2600)
    await typeInPane(3, 'git log --oneline -10')
    await browser.pause(2600)
    await typeInPane(4, 'date +"%H:%M:%S ready"')
    await browser.pause(2200)

    // 04 — Live workspace: five real zsh panes with real output.
    await capturePage('04-workspace.png')

    // ---------- Athena panel ----------
    const athena = await browser.execute(() => {
      const btn = document.querySelector('[data-athena-toggle]')
      if (btn) {
        btn.click()
        return { ok: true, via: 'toggle' }
      }
      return { ok: false }
    })
    if (!athena.ok) {
      await browser.keys(['Meta', 'j'])
    }
    await browser.pause(2500)

    // 05 — Workspace with the Athena panel open.
    const athenaFacts = await browser.execute(() => {
      const chat = document.querySelector('[data-panel="athena"], .athena-panel, .chat-panel')
      const cands = Array.from(document.querySelectorAll('*')).filter((el) => {
        const r = el.getBoundingClientRect()
        return r.width > 250 && r.height > 500 && el.className &&
          /(chat|athena|panel)/i.test(el.className.toString())
      })
      const pick = chat || cands.slice(-1)[0]
      if (!pick) return { found: false, cands: cands.length }
      const r = pick.getBoundingClientRect()
      return {
        found: true,
        cls: (pick.className || '').toString().slice(0, 60),
        rect: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)],
        msgs: pick.querySelectorAll('.msg, .message, .chat-msg').length,
      }
    })
    console.log('[CAPTURE] athena panel:', JSON.stringify(athenaFacts))
    await capturePage('05-athena.png')

    // ---------- Kanban: populate every column with real tasks ----------
    await clickButtonByText('kanban')
    await waitFor('kanban board', () => !!document.querySelector('.kanban-board'))

    // The board renders an empty state (no column inputs) until the first task
    // exists — seed one via the backend, then remount the board so it reloads
    // (KanbanBoard loads tasks on mount only).
    const seeded = await invoke('kanban_create_task', {
      title: 'Browser e2e for new-workspace modal',
      description: null,
    })
    console.log('[CAPTURE] seeded kanban task:', typeof seeded === 'string' ? seeded.slice(0, 80) : seeded)
    await browser.pause(500)
    await clickButtonByText('workspace')
    await browser.pause(700)
    await clickButtonByText('kanban')
    await waitFor('kanban columns', () => document.querySelectorAll('.kanban-column').length === 4)
    // Wait for the board's mount-time task reload to settle — its (slow)
    // kanban_get_tasks invoke can return AFTER the first add's reload and
    // clobber the fresh task store. The seeded task rendering is the signal.
    await waitFor(
      'seeded kanban card rendered',
      () =>
        Array.from(document.querySelectorAll('.kanban-card')).some((c) =>
          (c.textContent || '').includes('Browser e2e'),
        ),
    )
    await browser.pause(300)

    // Column order: To Do, In Progress, Review, Done.
    await addKanbanTasks(0, ['Theme tokens for light canvas', 'Plugin trust-policy docs'])
    await addKanbanTasks(1, ['Agent activity feed UI polish', 'Swarm task status colors'])
    await addKanbanTasks(2, ['Swarm lifecycle persistence test'])
    await addKanbanTasks(3, ['Terminal split panes', 'Notification center', 'Command palette'])
    await browser.pause(1200)

    // 07 — Kanban: four columns with real cards.
    await capturePage('07-kanban.png')

    // ---------- Swarm Mission workspace ----------
    await clickBySelector('.workspace-tabs button[aria-label="New Workspace"]')
    await waitFor('swarm mission option', () =>
      Array.from(document.querySelectorAll('button')).some((b) =>
        (b.textContent || '').includes('Swarm Mission'),
      ),
    )
    await clickButtonByText('Swarm Mission', { partial: true })

    await waitFor('swarm dir input', () => !!document.querySelector('input[placeholder="/path/to/project"]'))
    // Name the mission space so its workspace tab is findable (distinct from
    // the panel labels 'workspace'/'kanban'/'swarm').
    expect(await setField('input[placeholder="my-project"]', 'mission')).toBe(true)
    expect(
      await setField('input[placeholder="/path/to/project"]', REPO_DIR),
    ).toBe(true)
    expect(
      await setField(
        'textarea[placeholder="Describe what the swarm should accomplish..."]',
        'Refactor the tool dispatch loop into a registry pattern, add coverage for the agent activity feed, and update the release notes.',
      ),
    ).toBe(true)
    await clickButtonByText('Next >')
    await waitFor('swarm team step', () => document.body.textContent.includes('Team (3 agents)'))
    await clickButtonByText('Launch Swarm')

    // Launching a swarm mission does not switch the active panel — the board
    // only mounts on the Swarm panel.
    await clickButtonByText('swarm')
    try {
      await waitFor(
        'swarm board with 3 agents',
        () => document.querySelectorAll('.agent-card').length === 3,
        8000,
      )
    } catch {
      // The active space may still be the terminal one — switch to the mission tab.
      await browser.execute(() => {
        const tabs = Array.from(document.querySelectorAll('.workspace-tabs *'))
        const tab = tabs.find((el) => (el.textContent || '').trim() === 'mission')
        if (tab) tab.click()
        return { ok: !!tab }
      })
      await browser.pause(1500)
      await waitFor(
        'swarm board with 3 agents after tab switch',
        () => document.querySelectorAll('.agent-card').length === 3,
        20000,
      )
    }
    await browser.pause(2500)

    // Add real mission tasks through the board UI.
    const addSwarmTask = async (title) => {
      await setField('input[placeholder="New task"]', title)
      await browser.pause(300)
      expect((await clickButtonByText('Add task')).ok).toBe(true)
      await browser.pause(900)
    }
    await addSwarmTask('Refactor tool dispatch registry')
    await addSwarmTask('Agent activity feed coverage')
    await addSwarmTask('Release notes for v0.3.0')

    // Read the live swarm state to get real agent + task ids.
    // tauri-wd can deliver the JSON string as a parsed object; handle both.
    const stateRaw = await invoke('swarm_read_state', { dir: REPO_DIR })
    const state = typeof stateRaw === 'string' ? JSON.parse(stateRaw) : stateRaw
    console.log('[CAPTURE] swarm state:', JSON.stringify(state).slice(0, 160))
    const agents = state.agents || []
    const coordinator = agents.find((a) => a.role === 'coordinator')
    const builders = agents.filter((a) => a.role === 'builder')
    expect(coordinator).toBeTruthy()
    expect(builders.length).toBeGreaterThanOrEqual(2)

    const dir = REPO_DIR
    // Bring the swarm to life: statuses, last actions, current tasks.
    // NOTE: Tauri v2 command args use camelCase wire keys, so multi-word
    // backend params are `agentId` / `lastAction` / `currentTask` / `taskId`.
    if (coordinator) {
      await invoke('swarm_update_agent', {
        dir,
        agentId: coordinator.id,
        status: 'thinking',
        lastAction: 'Delegating tasks across the team',
        currentTask: 'Coordinate the dispatch refactor',
      })
    }
    if (builders[0]) {
      await invoke('swarm_update_agent', {
        dir,
        agentId: builders[0].id,
        status: 'writing',
        lastAction: 'Refactoring the tool dispatch loop',
        currentTask: 'Registry pattern extraction',
      })
    }
    if (builders[1]) {
      await invoke('swarm_update_agent', {
        dir,
        agentId: builders[1].id,
        status: 'waiting',
        lastAction: 'Awaiting review on activity-feed tests',
      })
    }

    // Give one task a live status. Backend accepts: queued/building/review/done/blocked/stalled.
    const tasks = state.tasks || []
    if (tasks[0]) {
      await invoke('swarm_update_task', { dir, taskId: tasks[0].id, status: 'building' })
    }
    if (tasks[1]) {
      await invoke('swarm_update_task', { dir, taskId: tasks[1].id, status: 'done' })
    }

    // Populate the activity feed with inter-agent messages.
    if (coordinator && builders[0]) {
      await invoke('swarm_send_message', {
        dir,
        from: coordinator.id,
        to: builders[0].id,
        content: 'Tackle the registry pattern first — keep the dispatch contract stable.',
      })
    }
    if (builders[0] && builders[1]) {
      await invoke('swarm_send_message', {
        dir,
        from: builders[0].id,
        to: builders[1].id,
        content: 'Draft pushed — can you review the activity-feed diff?',
      })
    }
    await browser.pause(3000)

    // 06 — Swarm: live agents with statuses, mission tasks, activity feed.
    const swarmFacts = await browser.execute(() => {
      const cards = Array.from(document.querySelectorAll('.agent-card')).map((c) => {
        const r = c.getBoundingClientRect()
        return {
          text: (c.textContent || '').trim().slice(0, 30),
          rect: [Math.round(r.x), Math.round(r.y), Math.round(r.width), Math.round(r.height)],
        }
      })
      const feed = Array.from(document.querySelectorAll('.activity-feed *, .feed *, [class*=activity] *'))
        .filter((el) => el.children.length === 0 && el.textContent.trim().length > 4)
        .map((el) => el.textContent.trim().slice(0, 40))
        .slice(0, 6)
      return { cards, feed }
    })
    console.log('[CAPTURE] swarm facts:', JSON.stringify(swarmFacts))
    await capturePage('06-swarm.png')

    // ---------- Back to the terminal workspace for the final wide shot ----------
    await browser.execute(() => {
      const tabs = Array.from(document.querySelectorAll('.workspace-tabs *'))
      const tab = tabs.find((el) => (el.textContent || '').trim() === 'athenas-core')
      if (tab) tab.click()
      return { ok: !!tab }
    })
    await clickButtonByText('workspace')
    await waitFor(
      '5 panes visible again',
      () => document.querySelectorAll('.xterm-mount').length >= 5,
    )
    await browser.pause(2000)

    // 08 — Final wide shot of the full workspace.
    await capturePage('08-workspace-final.png')

    console.log('[CAPTURE] screenshots written to', shotDir)
  })
})
