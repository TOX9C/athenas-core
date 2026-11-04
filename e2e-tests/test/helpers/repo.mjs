// Portable repo paths for the e2e suite.
//
// Every spec used to hardcode an absolute, owner-specific checkout path, which
// made the suite runnable from exactly one checkout. These helpers derive the
// checkout root from this module's own location instead, so specs work from
// the main checkout, a `git worktree`, or a CI runner.

import { mkdirSync, realpathSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const __dirname = dirname(fileURLToPath(import.meta.url))

/** Absolute, symlink-resolved root of the checkout this harness lives in. */
export function repoRoot() {
  return realpathSync(join(__dirname, '..', '..', '..'))
}

/** Per-label sandbox dir under the repo root (always implicitly trusted). */
export function e2eSandboxDir(label) {
  const dir = join(repoRoot(), '.e2e-sandbox', label)
  mkdirSync(dir, { recursive: true })
  // Canonicalize so what the spec hands the backend matches what the backend
  // would store (`canonicalize()` in workspace.rs:71). macOS realpaths away
  // any symlink in the path.
  return realpathSync(dir)
}
