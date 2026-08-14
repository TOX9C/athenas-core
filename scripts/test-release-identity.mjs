#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { strict as assert } from 'node:assert'
import { fileURLToPath } from 'node:url'

const script = fileURLToPath(new URL('./check-release-identity.mjs', import.meta.url))
const result = spawnSync(process.execPath, [script], { encoding: 'utf8' })
assert.equal(result.status, 0, result.stderr)
assert.match(result.stdout, /Release identity checks passed/)
console.log('release identity checks passed')
