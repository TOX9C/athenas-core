#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { strict as assert } from 'node:assert'
import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const script = fileURLToPath(new URL('./check-release-identity.mjs', import.meta.url))
const readme = readFileSync(fileURLToPath(new URL('../README.md', import.meta.url)), 'utf8')
const pkgVersion = JSON.parse(readFileSync(fileURLToPath(new URL('../package.json', import.meta.url)), 'utf8')).version
const run = (env = {}) => spawnSync(process.execPath, [script], {
  encoding: 'utf8',
  env: { ...process.env, ...env },
})

const result = run()
assert.equal(result.status, 0, result.stderr)
assert.match(result.stdout, /Release identity checks passed/)
assert.match(readme, /macOS 13\+ on Apple Silicon/)

const mismatched = run({ RELEASE_VERSION: '9.9.9' })
assert.equal(mismatched.status, 1)
assert.match(mismatched.stderr, /Release identity checks failed:/)
assert.match(mismatched.stderr, new RegExp(`version: package\\.json version ${pkgVersion.replace(/\\./g, '\\.')} != 9\\.9\\.9`))
assert.match(mismatched.stderr, new RegExp(`version: src-tauri/tauri\\.conf\\.json version ${pkgVersion.replace(/\./g, '\\.')} != 9\\.9\\.9`))
assert.match(mismatched.stderr, new RegExp(`version: frontend/Cargo\\.toml version ${pkgVersion.replace(/\./g, '\\.')} != 9\\.9\\.9`))
assert.match(mismatched.stderr, new RegExp(`version: src-tauri/Cargo\\.toml version ${pkgVersion.replace(/\./g, '\\.')} != 9\\.9\\.9`))

console.log('release identity checks passed')
