// Shared trusted-root helper for the e2e suite.
//
// Why this exists: `workspace_add_trusted_root` used to be called with
// `dir: '/tmp'` in 15 specs. The backend REJECTS /tmp — `is_forbidden_root`
// (src-tauri/src/commands/workspace.rs:31) lists `/tmp` in its forbidden
// prefixes — and macOS `$TMPDIR` resolves to `/private/var/folders/...`,
// which is forbidden too (both `/private` and `/var` are listed). The gate is
// a deliberate sandbox control and must NOT be relaxed; the test side has to
// ask for a directory that is actually permitted.
//
// A directory under the repo root is permitted for two independent reasons:
//   1. It is not any forbidden prefix.
//   2. The app's own project root is always implicitly trusted
//      (`effective_roots` in src-tauri/src/commands/mod.rs:352), and any
//      descendant of a root passes `is_within_any_root`.
// so PTY spawns and sandboxed fs commands inside the sandbox dir succeed
// without the app needing a persisted user gesture.
//
// `window.__athenaE2E` is a FRONTEND-ONLY bypass
// (new_space_modal.rs:93 / xterm_mount.rs:1377): it skips the modal's empty
// directory validation and it exposes `__athenaTermMap`. It does NOT bypass
// the backend trust gate — never write a comment claiming it does.
//
// Two caveats every spec that seeds a root must respect:
//   1. `workspace_add_trusted_root` for a NOT-already-persisted dir opens a
//      blocking native dialog (workspace.rs:79-100) with no window to click,
//      so the invoke never returns and the spec hangs. Seeding the persisted
//      list below makes it an early-returning no-op that skips the dialog.
//   2. `browser.execute` does NOT await a Promise returned by its function
//      (this repo proved it in athena-chat-stub / kanban-persistence), so
//      every invoke here is bridged through a window ticket and polled —
//      otherwise the seed could still be in flight when the next command
//      runs, which is exactly the race that re-opens the dialog.

import { mkdirSync, realpathSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))

// test/helpers/ -> test/ -> e2e-tests/ -> repo root.
const REPO_ROOT = join(__dirname, '..', '..', '..')

// Store key the backend reads trusted roots from
// (TRUSTED_ROOTS_KEY, src-tauri/src/commands/mod.rs:236).
const TRUSTED_ROOTS_KEY = 'workspace.trusted_roots'

const POLL = { timeout: 30000, interval: 200 }

let ticketSeq = 0

// Fire a Tauri invoke in the page, park its settlement on `window`, and poll
// for it synchronously so the caller really knows it finished.
async function invokeSettled(browser, cmd, args = {}) {
  const ticket = `__e2eTrust_${Date.now()}_${ticketSeq++}`
  await browser.execute((t, c, a) => {
    window[t] = { status: 'pending' }
    window.__TAURI__.core.invoke(c, a).then(
      (v) => { window[t] = { status: 'ok', value: v === undefined ? null : v } },
      (e) => { window[t] = { status: 'err', error: String(e) } },
    )
  }, ticket, cmd, args)
  await browser.waitUntil(
    async () => browser.execute(
      (t) => {
        const entry = window[t]
        return entry && entry.status !== 'pending'
      },
      ticket,
    ),
    { ...POLL, timeoutMsg: `invoke(${cmd}) never settled` },
  )
  const entry = await browser.execute((t) => {
    const e = window[t]
    delete window[t]
    return e
  }, ticket)
  return entry
}

async function readPersistedRoots(browser) {
  const entry = await invokeSettled(browser, 'store_get', { key: TRUSTED_ROOTS_KEY })
  if (!entry || entry.status === 'err') return []
  try {
    const parsed = JSON.parse(entry.value)
    return Array.isArray(parsed) ? parsed.filter((d) => typeof d === 'string') : []
  } catch {
    return []
  }
}

// Per-label sandbox directories, unique per spec so concurrent specs can
// never share (or delete) each other's directory.
export function e2eTrustRoot(label) {
  const dir = join(REPO_ROOT, '.e2e-sandbox', label)
  mkdirSync(dir, { recursive: true })
  // Canonicalize so what the spec hands to the backend matches what the
  // backend would store (`canonicalize()` in workspace.rs:71). macOS realpaths
  // away any symlink in the path.
  try {
    return realpathSync(dir)
  } catch {
    return dir
  }
}

// Persist roots into the store purely to record intent; specs no longer call
// `workspace_add_trusted_root` for them, because that command opens a blocking
// native dialog (workspace.rs:79-100) whenever the dir is not in the persisted
// list — even when it is already covered by the implicitly trusted project
// root (the membership check runs before the dialog; there is no
// effective-root early-return there). A spec cannot click a native dialog, so
// every invoke used to leak one window per spec, which is what the 2026-10-17
// screenshot pile-up was. The gate is a deliberate sandbox control and must NOT
// be relaxed; instead specs rely on the project root alone and simply do not
// invoke the gesture.
//
// NOTE (proven 2026-10-17): `store_set` CANNOT actually seed the backend key.
// Its Rust signature is `value: String` (store.rs:174), so the page may only
// hand it a string; `set_deferred` stores that string verbatim as a JSON
// string, while `load_trusted_roots` reads the key back as `Vec<String>`
// (mod.rs:246). A JSON-text array round-trips through `store_get`
// (`readPersistedRoots` parses it happily) but fails the backend's
// `Vec<String>` decode. Harmless, since nothing reads it back as roots:
// `.e2e-sandbox/<label>` is already authorized by the implicit project root
// (mod.rs:352). Kept for intent only; `fillSpaceDir` is what keeps spawns
// inside the trusted root.
export async function seedTrustedRoots(browser, dirs) {
  const merged = [...new Set([...await readPersistedRoots(browser), ...dirs])]
  await invokeSettled(browser, 'store_set', {
    key: TRUSTED_ROOTS_KEY,
    value: JSON.stringify(merged),
  })
  return merged
}

// Drop roots again so a spec leaves the user's persisted list as it found it.
// Same limitation as seedTrustedRoots: the write is only visible to the
// frontend's own reads, never to the backend's `Vec<String>` decode.
export async function unseedTrustedRoots(browser, dirs) {
  const kept = (await readPersistedRoots(browser)).filter((d) => !dirs.includes(d))
  await invokeSettled(browser, 'store_set', {
    key: TRUSTED_ROOTS_KEY,
    value: JSON.stringify(kept),
  })
  return kept
}

// Record trust-established for the sandbox dir without calling the add command.
//
// The dir is authorized: it lives under the repo root, and the app's own project
// root is always implicitly trusted (`effective_roots_from`, mod.rs:352), so
// `pty_spawn`'s cwd validation accepts it with no persisted entry. Calling
// `workspace_add_trusted_root` instead would open the backend's blocking native
// dialog (its only early return is membership in the persisted list), which a
// spec cannot service — the dialog leaked one OS window per spec. Never relax
// the gate to make this work: the correct harness change is to not ask.
//
// Sets the per-spec flag synchronously so `waitUntil(() => flag === 'ok')`
// gates stay true without awaiting an IPC.
export async function trustViaProjectRoot(browser) {
  await browser.execute(() => {
    for (const key of [
      '__athenaTrust',
      '__fsTrustRoot',
      '__paneSwapTrustRoot',
      '__paneScalingTrustRoot',
    ]) {
      window[key] = 'ok'
    }
    return true
  })
}

// Frontend-only bypass: skips the New Space modal's empty-directory validation
// and exposes `__athenaTermMap`. Never a substitute for a permitted dir.
export async function setE2E() {
  await browser.execute(() => {
    window.__athenaE2E = true
    return true
  })
}

// Fill the New Space modal's Working Directory field.
//
// Why this is required: `pty_spawn` validates the pane cwd against the trusted
// roots (validate_cwd -> is_within_any_root) and rejects it otherwise. Under
// `__athenaE2E` the modal treats a blank field as "/tmp" (new_space_modal.rs:175),
// which `is_forbidden_root` (workspace.rs:31) rejects — so every pane spawns
// with a doomed cwd and the spec's ".xterm mounted" wait times out. Passing the
// spec's own `e2eTrustRoot(label)` dir keeps the launch inside the app root and
// inside the list seeded by `seedTrustedRoots`.
//
// The field is a controlled Dioxus input (`value: "{space_dir}"`), but a
// direct `.value` assignment plus `input`/`change` events is the path already
// proven against it (workspace-modal-gating.e2e.mjs:99-112). Setting the value
// also opens the Step-1 "Next >" gate for non-e2e launches.
export async function fillSpaceDir(browser, dir) {
  // Not `async` inside the callback: browser.execute does NOT await a returned
  // Promise (caveat 2 above, proven in athena-chat-stub / kanban-persistence),
  // so anything async here would be silently dropped. Mirrors the proven path
  // in workspace-modal-gating.e2e.mjs: find the field, assign, dispatch.
  return browser.execute((d) => {
    const input = document.querySelector('[data-athena-browse-target="directory"]')
    if (!input) return { ok: false, reason: 'dir field not found' }
    input.value = d
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('change', { bubbles: true }))
    return { ok: input.value === d, value: input.value }
  }, dir)
}
