#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

// Resolve repo root from this script's location (like the sibling checkers)
// so the gate behaves identically no matter which cwd it is launched from.
const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const baselinePath = resolve(root, process.env.CLIPPY_WARNING_BASELINE ?? 'scripts/clippy-warning-baseline.txt')
const logPath = resolve(root, process.env.CLIPPY_BASELINE_PATH ?? 'clippy-baseline.log')

// Warm cargo caches omit diagnostics for cached crates, which made a dirty
// tree pass locally while cold CI flagged new warnings. Compile in a
// dedicated target dir that is wiped first so every run sees the full
// warning set, exactly like a cold CI machine. Set CLIPPY_SKIP_CLEAN=1 for
// quick local iteration when you know the risk.
const targetDir = resolve(root, process.env.CLIPPY_TARGET_DIR ?? 'target/clippy-baseline')
if (process.env.CLIPPY_SKIP_CLEAN !== '1') {
  rmSync(targetDir, { recursive: true, force: true })
}
// --all-targets so test/bench code participates: test-code warnings used to
// escape the baseline gate entirely (P2).
const result = spawnSync('cargo', ['clippy', '--workspace', '--all-targets', '--locked', '--message-format=json'], {
  cwd: root,
  encoding: 'utf8',
  stdio: ['ignore', 'pipe', 'pipe'],
  env: { ...process.env, CARGO_TARGET_DIR: targetDir },
})

const output = `${result.stdout ?? ''}${result.stderr ?? ''}`
writeFileSync(logPath, output)

if (result.error) {
  console.error(`Clippy could not start: ${result.error.message}`)
  process.exit(1)
}
if (result.status !== 0) {
  console.error(output)
  console.error(`Clippy failed with exit code ${result.status}`)
  process.exit(result.status ?? 1)
}

// Key format: `code|file` where file is the primary (or first) span's file.
// The baseline Set alone would collapse repeats: a *second* instance of an
// already-baselined code+file pair produced the identical key and slipped
// through the gate. We therefore compare per-key occurrence counts — each
// baseline line allows exactly one instance, so the same code+file requires
// one duplicated line per reviewed instance.
const warningCounts = new Map()
for (const line of result.stdout.split('\n')) {
  try {
    const event = JSON.parse(line)
    if (event.reason !== 'compiler-message' || event.message?.level !== 'warning') continue
    const code = event.message.code?.code
    const file = event.message.spans?.find(span => span.is_primary)?.file_name
      ?? event.message.spans?.[0]?.file_name
    if (code?.startsWith('clippy::') && file) {
      const key = `${code}|${file}`
      warningCounts.set(key, (warningCounts.get(key) ?? 0) + 1)
    }
  } catch {
    // Cargo's JSON stream may contain non-diagnostic lines; ignore those.
  }
}

if (!existsSync(baselinePath)) {
  console.error(`Clippy warning baseline is missing: ${baselinePath}`)
  process.exit(1)
}
const baselineCounts = new Map()
for (const line of readFileSync(baselinePath, 'utf8').split('\n')) {
  const key = line.trim()
  if (!key || key.startsWith('#')) continue
  baselineCounts.set(key, (baselineCounts.get(key) ?? 0) + 1)
}
const newWarnings = [...warningCounts.entries()]
  .filter(([key, count]) => count > (baselineCounts.get(key) ?? 0))
  .map(([key, count]) => `${key} (${count} instance(s), baseline allows ${baselineCounts.get(key) ?? 0})`)
  .sort()
if (newWarnings.length > 0) {
  console.error(`New Clippy warning instance(s):\n${newWarnings.join('\n')}`)
  console.error(`Update ${baselinePath} only after reviewing each new warning.`)
  process.exit(1)
}

const totalWarnings = [...warningCounts.values()].reduce((sum, count) => sum + count, 0)
console.log(
  `Clippy completed with ${totalWarnings} warning instance(s); baseline is current (${baselinePath}).`,
)
