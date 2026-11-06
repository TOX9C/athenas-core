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
// Specs never seed the persisted trusted-root list and never call
// `workspace_add_trusted_root`. Seeding it was a no-op by construction:
// `store_set` takes a `String` (store.rs:174) while the backend reads the key
// back as `Vec<String>` (mod.rs:246), so the value never reached the decoder.
// The add command opens a blocking native dialog (workspace.rs:79-100) that
// no spec can service. The `.e2e-sandbox/<label>` dirs handed to `pty_spawn`
// are already authorized by the implicit project root, so specs rely on that
// alone and simply do not invoke the gesture.

import { e2eSandboxDir } from './repo.mjs'

// Per-label sandbox directories, unique per spec so concurrent specs can
// never share (or delete) each other's directory.
//
// Alias of `e2eSandboxDir` (same behaviour); kept under this name so the ~20
// existing `e2eTrustRoot(label)` import sites keep working.
export const e2eTrustRoot = e2eSandboxDir

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
// spec's own `e2eTrustRoot(label)` dir keeps the launch inside the app root.
//
// The field is a controlled Dioxus input (`value: "{space_dir}"`), but a
// direct `.value` assignment plus `input`/`change` events is the path already
// proven against it (workspace-modal-gating.e2e.mjs:99-112). Setting the value
// also opens the Step-1 "Next >" gate for non-e2e launches.
export async function fillSpaceDir(browser, dir) {
  // Not `async` inside the callback: browser.execute does NOT await a returned
  // Promise (proven in athena-chat-stub / kanban-persistence), so anything
  // async here would be silently dropped. Mirrors the proven path in
  // workspace-modal-gating.e2e.mjs: find the field, assign, dispatch.
  return browser.execute((d) => {
    const input = document.querySelector('[data-athena-browse-target="directory"]')
    if (!input) return { ok: false, reason: 'dir field not found' }
    input.value = d
    input.dispatchEvent(new Event('input', { bubbles: true }))
    input.dispatchEvent(new Event('change', { bubbles: true }))
    return { ok: input.value === d, value: input.value }
  }, dir)
}
