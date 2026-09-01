#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync, readFileSync, rmSync, writeFileSync } from 'node:fs'

const baselinePath = process.env.CLIPPY_WARNING_BASELINE ?? 'scripts/clippy-warning-baseline.txt'
const logPath = process.env.CLIPPY_BASELINE_PATH ?? 'clippy-baseline.log'

// Warm cargo caches omit diagnostics for cached crates, which made a dirty
// tree pass locally while cold CI flagged new warnings. Compile in a
// dedicated target dir that is wiped first so every run sees the full
// warning set, exactly like a cold CI machine. Set CLIPPY_SKIP_CLEAN=1 for
// quick local iteration when you know the risk.
const targetDir = process.env.CLIPPY_TARGET_DIR ?? 'target/clippy-baseline'
if (process.env.CLIPPY_SKIP_CLEAN !== '1') {
  rmSync(targetDir, { recursive: true, force: true })
}
// --all-targets so test/bench code participates: test-code warnings used to
// escape the baseline gate entirely (P2).
const result = spawnSync('cargo', ['clippy', '--workspace', '--all-targets', '--locked', '--message-format=json'], {
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

const warningKeys = new Set()
for (const line of result.stdout.split('\n')) {
  try {
    const event = JSON.parse(line)
    if (event.reason !== 'compiler-message' || event.message?.level !== 'warning') continue
    const code = event.message.code?.code
    const file = event.message.spans?.find(span => span.is_primary)?.file_name
      ?? event.message.spans?.[0]?.file_name
    if (code?.startsWith('clippy::') && file) warningKeys.add(`${code}|${file}`)
  } catch {
    // Cargo's JSON stream may contain non-diagnostic lines; ignore those.
  }
}

if (!existsSync(baselinePath)) {
  console.error(`Clippy warning baseline is missing: ${baselinePath}`)
  process.exit(1)
}
const baseline = new Set(
  readFileSync(baselinePath, 'utf8')
    .split('\n')
    .map(line => line.trim())
    .filter(line => line && !line.startsWith('#')),
)
const newWarnings = [...warningKeys].filter(key => !baseline.has(key)).sort()
if (newWarnings.length > 0) {
  console.error(`New Clippy warning instance(s):\n${newWarnings.join('\n')}`)
  console.error(`Update ${baselinePath} only after reviewing each new warning.`)
  process.exit(1)
}

console.log(
  `Clippy completed with ${warningKeys.size} warning instance(s); baseline is current (${baselinePath}).`,
)
