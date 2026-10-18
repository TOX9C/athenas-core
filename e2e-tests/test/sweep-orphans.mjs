/**
 * Kill PTY orphans left behind by e2e runs (and crashed dev instances).
 *
 * The wdio runner starts the BARE `target/debug/athenas-core` binary and
 * tears it down after the run. When the app is killed hard (or crashes —
 * see the UNUserNotificationCenter abort guard in state.rs) instead of
 * quitting through RunEvent::ExitRequested, its PTY shells survive,
 * reparented to launchd with the exact argv `/bin/zsh -d -i`
 * (crates/athena-terminal/src/session.rs spawn args). Whatever ran inside
 * them survives too: omp deliberately outlives the death of its tty — that
 * is what `omp --resume` recovers from. A day of test runs piled up 77 omp
 * agents (~3.4 GB RAM) plus ~400 orphan shells (2026-10-17).
 *
 * Runs from wdio.conf.mjs onPrepare (cleans leftovers of a crashed or
 * aborted previous run before a new one starts) and onComplete (immediately
 * after teardown). Also usable standalone after a `cargo tauri dev` crash:
 *
 *   node test/sweep-orphans.mjs
 *
 * Targets — only processes attributable to Athena e2e/dev runs, i.e. only
 * processes whose parent is DEAD (PPID 1, reparented to launchd):
 *   1. a wedged app instance at target/debug/athenas-core — SIGTERM first
 *      so the app's own graceful shutdown reaps its panes; SIGKILL only if
 *      it refuses to die (those panes are then caught by (2)/(3) anyway);
 *   2. orphaned Athena shells: `/bin/zsh -d -i` with PPID 1. That exact
 *      argv is the PTY spawn signature; the user's own shells (Terminal.app,
 *      tmux) never run with `-d -i` and always keep a live parent;
 *   3. orphaned omp agents: argv `omp` with PPID 1;
 *   4. orphaned omp headless Chrome: `--user-data-dir` inside
 *      ~/.omp/browser-profiles with PPID 1.
 *
 * It never matches anything with a live parent — the user's own interactive
 * shells, a foreground agent in their terminal, or a user-spawned omp
 * browser. Best-effort by design: it logs everything it kills and always
 * exits 0 so a sweep hiccup can never block a test run.
 */
import { execFileSync } from 'node:child_process'
import { homedir } from 'node:os'
import { join } from 'node:path'

const E2E_BINARY = join('target', 'debug', 'athenas-core')
const OMP_BROWSER_DIR = join(homedir(), '.omp', 'browser-profiles')

/** One `ps` row: { pid, ppid, command }. */
function snapshot() {
  // Explicit 16 MB buffer: a full ps listing is ~100 KB for ~700 processes,
  // but execFileSync's 1 MB default would throw ENOBUFS at ~10x that count.
  const out = execFileSync('ps', ['-axo', 'pid=,ppid=,command='], {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  })
  const rows = []
  for (const line of out.split('\n')) {
    // Parse the two leading numeric columns positionally; the command column
    // must keep its exact bytes (argv equality matters for the zsh match).
    const match = line.trim().match(/^(\d+)\s+(\d+)\s+(.*)$/)
    if (!match) continue
    rows.push({ pid: Number(match[1]), ppid: Number(match[2]), command: match[3] })
  }
  return rows
}

/** Liveness probe that never throws (signal 0 = permission check only). */
function alive(pid) {
  try {
    process.kill(pid, 0)
    return true
  } catch {
    return false
  }
}

function sleep(ms) {
  execFileSync('/bin/sleep', [String(ms / 1000)])
}

/**
 * SIGTERM the given pids, poll until they are gone or the grace window
 * expires, then SIGKILL the survivors. Never throws.
 */
function terminate(procs, label, graceMs) {
  if (procs.length === 0) return
  const pids = procs.map((p) => p.pid)
  console.log(`[sweep] terminating ${pids.length} ${label}: pid ${pids.join(', ')}`)
  for (const pid of pids) {
    try {
      process.kill(pid, 'SIGTERM')
    } catch {}
  }
  const deadline = Date.now() + graceMs
  while (Date.now() < deadline && pids.some(alive)) sleep(250)
  for (const pid of pids.filter(alive)) {
    try {
      process.kill(pid, 'SIGKILL')
    } catch {}
  }
}

function killOrphans() {
  // Phase 1 — wedged app instances. SIGTERM lets the app's own
  // shutdown_all reap its panes properly; 10 s covers the graceful
  // shutdown's timeouts. A survivor gets SIGKILL, which orphans its
  // panes — phases 2-4 below then catch those. PPID 1 (reparented to
  // launchd) is required: a live `cargo tauri dev` app keeps cargo as
  // its parent and must never be touched by a sweep. Matched on argv[0]
  // only — an orphaned rustc linking the binary (`rustc -o
  // …/target/debug/athenas-core …`) carries the path in a later arg and
  // must not be killed.
  terminate(
    snapshot().filter(
      (p) => p.ppid === 1 && p.command.split(' ')[0].includes(E2E_BINARY),
    ),
    'wedged e2e app instance(s)',
    10_000,
  )

  // Phase 2 — orphaned Athena pane shells (the `-d -i` argv is the PTY
  // spawn signature; only Athena-spawned zsh ever runs with it orphaned).
  const before = snapshot()
  const shells = before.filter((p) => p.command === '/bin/zsh -d -i' && p.ppid === 1)
  terminate(shells, 'orphaned athena shell(s) (zsh -d -i)', 3_000)

  // Phase 3 — orphaned omp agents. After phase 2, agents whose shell just
  // died are reparented to launchd, so PPID 1 catches both them and any
  // earlier direct orphans. Matched on the first argv token only: omp
  // rewrites its process title, so argv can be `omp`, a full path, or carry
  // trailing flags.
  const mid = snapshot()
  const omp = mid.filter((p) => {
    if (p.ppid !== 1) return false
    const first = p.command.split(' ')[0]
    return first === 'omp' || first.endsWith('/omp')
  })
  terminate(omp, 'orphaned omp agent(s)', 3_000)

  // Phase 4 — omp's headless Chrome, orphaned once its omp parent died.
  // Matched by the omp-only browser profile dir AND PPID 1, so a live
  // user-spawned omp browser is never touched.
  const after = snapshot()
  const chrome = after.filter(
    (p) => p.ppid === 1 && p.command.includes(`--user-data-dir=${OMP_BROWSER_DIR}`),
  )
  terminate(chrome, 'orphaned omp headless Chrome helper(s)', 3_000)

  if (shells.length + omp.length + chrome.length === 0) {
    console.log('[sweep] no PTY orphans found')
  }
}

// Best-effort by design: a sweep hiccup (ps unavailable, output overflow,
// …) logs and exits 0 so it can never fail a wdio run (see header).
try {
  killOrphans()
} catch (err) {
  console.warn(`[sweep] failed (best-effort, ignored): ${err?.stack ?? err}`)
}
