/**
 * Deterministic store snapshot for e2e runs.
 *
 * The suite runs with maxInstances: 1, so every spec shares ONE app process
 * and the persisted store at ~/Library/Application Support/athena-core/
 * (crates/athena-store/src/store.rs:159 -> `dirs::data_dir()/athena-core`).
 *
 * Workspaces persist across launches, so specs that assume an empty state
 * fail depending on what earlier specs left behind. To make a run
 * reproducible, clear the store OUTSIDE the suite process and start from a
 * known state.
 *
 * This module deliberately does NOT delete or rewrite anything by itself.
 * It only reads the store path and writes a timestamped, never-overwritten
 * backup, and refuses to touch a live app process.
 *
 * Usage:
 *   node test/reset-store.mjs backup    # snapshot store.json -> <path>.e2e-backup.<ts>
 *   node test/reset-store.mjs clear     # move store.json aside (keeps the backup)
 *   node test/reset-store.mjs restore   # put back the newest backup
 */
import { access, cp, mkdir, readdir, rm } from 'node:fs/promises'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { execSync } from 'node:child_process'

const dataDir = join(homedir(), 'Library', 'Application Support', 'athena-core')
const storePath = join(dataDir, 'store.json')

async function exists(path) {
  try {
    await access(path)
    return true
  } catch {
    return false
  }
}

function ensureAppNotRunning() {
  try {
    const out = execSync('pgrep -x athenas-core', { stdio: ['ignore', 'pipe', 'ignore'] })
      .toString()
      .trim()
    if (out) {
      throw new Error(
        'athenas-core is running (pid ' +
          out.replace(/\n/g, ', ') +
          '). It holds the store open and re-persists on shutdown, which would resurrect anything you change. Quit the app first.',
      )
    }
  } catch (err) {
    if (err.status === 1) return // pgrep found nothing -> safe
    throw err
  }
}

async function newestBackup() {
  const entries = await readdir(dataDir)
  const backups = entries
    .filter((name) => name.startsWith('store.json.e2e-backup.'))
    .sort()
    .reverse()
  return backups.length ? join(dataDir, backups[0]) : null
}

const cmd = process.argv[2] ?? 'help'

if (cmd === 'backup') {
  if (!(await exists(storePath))) {
    console.log(`reset-store: nothing to back up (no ${storePath})`)
    process.exit(0)
  }
  ensureAppNotRunning()
  const target = `${storePath}.e2e-backup.${Date.now()}`
  await cp(storePath, target) // copy-clobber semantics; name is timestamped, never overwrites
  console.log(`reset-store: backed up -> ${target}`)
} else if (cmd === 'clear') {
  ensureAppNotRunning()
  await mkdir(dataDir, { recursive: true })
  if (await exists(storePath)) {
    const target = `${storePath}.e2e-backup.${Date.now()}`
    await cp(storePath, target)
    await rm(storePath)
    console.log(`reset-store: moved store aside -> ${target}`)
    console.log(`reset-store: restore with: node test/reset-store.mjs restore`)
  } else {
    console.log('reset-store: no store to clear (already empty)')
  }
} else if (cmd === 'restore') {
  ensureAppNotRunning()
  const backup = await newestBackup()
  if (!backup) {
    console.error('reset-store: no backup found')
    process.exit(1)
  }
  await cp(backup, storePath)
  console.log(`reset-store: restored from ${backup}`)
} else {
  console.log('usage: node test/reset-store.mjs backup|clear|restore')
}
