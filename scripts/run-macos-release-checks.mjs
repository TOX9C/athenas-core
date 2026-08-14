#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { existsSync } from 'node:fs'
import process from 'node:process'

const args = process.argv.slice(2)
const artifactIndex = args.indexOf('--artifact')
const artifact = artifactIndex >= 0 ? args[artifactIndex + 1] : undefined
if (artifactIndex >= 0 && !artifact) {
  console.error('error: --artifact requires a path')
  process.exit(2)
}

const checks = [
  ['release identity', ['npm', 'run', 'check:release-identity']],
  ['Tauri command registry', ['npm', 'run', 'check:tauri-commands']],
  ['Tauri permissions', ['npm', 'run', 'check:tauri-permissions']],
  ['Tauri security', ['npm', 'run', 'check:tauri-security']],
  ['release privacy', ['npm', 'run', 'check:release-privacy']],
  ['plugin integration', ['npm', 'run', 'check:plugin-integration']],
  ['release script tests', ['npm', 'run', 'test:release-scripts']],
]

let failed = false
for (const [label, command] of checks) {
  console.log(`\n== ${label} ==`)
  const result = spawnSync(command[0], command.slice(1), { stdio: 'inherit' })
  if (result.error || result.status !== 0) {
    failed = true
    console.error(`FAILED: ${label}`)
  } else {
    console.log(`PASSED: ${label}`)
  }
}

if (artifact) {
  console.log('\n== macOS artifact structure/integrity ==')
  if (!existsSync(artifact)) {
    failed = true
    console.error(`FAILED: artifact does not exist: ${artifact}`)
  } else {
    // This intentionally performs only the checks available without release
    // credentials. Signing/notarization remain explicit opt-in verifier flags.
    const result = spawnSync(
      'node',
      ['scripts/verify-macos-artifact.mjs', '--artifact', artifact, '--require-app', '--require-arm64'],
      { stdio: 'inherit' },
    )
    if (result.error || result.status !== 0) {
      failed = true
      console.error('FAILED: macOS artifact structure/integrity')
    } else {
      console.log('PASSED: macOS artifact structure/integrity')
    }
  }
} else {
  console.log('\nSKIPPED: macOS artifact verification (pass --artifact <path>)')
}

console.log('\nRelease gate status:')
console.log(failed ? 'LOCAL CHECKS FAILED' : 'LOCAL CHECKS PASSED')
console.log('Signing, notarization, clean-machine validation, and packaged soak evidence remain manual release gates.')
process.exit(failed ? 1 : 0)
