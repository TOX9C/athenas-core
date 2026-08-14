#!/usr/bin/env node

import { spawnSync } from 'node:child_process'
import { strict as assert } from 'node:assert'
import { fileURLToPath } from 'node:url'
import { chmodSync, mkdirSync, mkdtempSync, writeFileSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'

const script = fileURLToPath(new URL('./verify-macos-artifact.mjs', import.meta.url))
const fixtureDir = mkdtempSync(join(tmpdir(), 'athena-verifier-test-'))

function run(args, env = process.env) {
  return spawnSync(process.execPath, [script, ...args], { encoding: 'utf8', env })
}

const missing = run(['--artifact', '/definitely/missing/Athena.dmg'])
assert.equal(missing.status, 2)
assert.match(missing.stderr, /artifact does not exist/)

const unsupportedPath = join(fixtureDir, 'Athena.zip')
writeFileSync(unsupportedPath, 'not a disk image')
const unsupported = run(['--artifact', unsupportedPath])
assert.equal(unsupported.status, 1)
assert.match(unsupported.stderr, /expected a \.dmg artifact/)

const help = run(['--help'])
assert.equal(help.status, 0)
assert.match(help.stdout, /--require-app/)
assert.match(help.stdout, /--require-signing/)
assert.match(help.stdout, /--require-arm64/)

// Exercise the architecture failure path without requiring a real DMG. The
// verifier still runs its normal command flow, while these tiny fixtures
// provide deterministic stand-ins for macOS's hdiutil/find/file commands.
const fakeBin = join(fixtureDir, 'bin')
mkdirSync(fakeBin)
const fakeHdiutil = join(fakeBin, 'hdiutil')
const fakeFind = join(fakeBin, 'find')
const fakePlutil = join(fakeBin, 'plutil')
const fakeFile = join(fakeBin, 'file')
writeFileSync(fakeHdiutil, '#!/bin/sh\nif [ "$1" = "attach" ]; then\n  mount=""\n  previous=""\n  for arg in "$@"; do\n    if [ "$previous" = "-mountpoint" ]; then mount="$arg"; fi\n    previous="$arg"\n  done\n  mkdir -p "$mount/app.app/Contents/MacOS"\n  : > "$mount/app.app/Contents/Info.plist"\n  : > "$mount/app.app/Contents/MacOS/Athena"\nfi\nexit 0\n')
writeFileSync(fakeFind, '#!/bin/sh\nprintf "%s\\0" "$1/app.app"\n')
writeFileSync(fakePlutil, '#!/bin/sh\nplist=""\nfor arg in "$@"; do plist="$arg"; done\n[ -f "$plist" ] || exit 1\nprintf "%s\\n" "Athena"\n')
writeFileSync(fakeFile, '#!/bin/sh\n[ -f "$1" ] || exit 1\nprintf "%s\\n" "Mach-O 64-bit executable x86_64"\n')
for (const file of [fakeHdiutil, fakeFind, fakePlutil, fakeFile]) chmodSync(file, 0o755)
const dmg = join(fixtureDir, 'Athena.dmg')
writeFileSync(dmg, 'fake dmg')
const fakeEnv = { ...process.env, PATH: `${fakeBin}:${process.env.PATH}` }
const nonArm64 = run(['--artifact', dmg, '--require-app', '--require-arm64'], fakeEnv)
assert.equal(nonArm64.status, 1)
assert.match(nonArm64.stderr, /not arm64-capable/)

writeFileSync(fakeFile, '#!/bin/sh\nprintf "%s\\n" "Mach-O 64-bit executable arm64"\n')
const arm64 = run(['--artifact', dmg, '--require-app', '--require-arm64'], fakeEnv)
assert.equal(arm64.status, 0)
assert.match(arm64.stdout, /architecture: arm64/)

rmSync(fixtureDir, { recursive: true, force: true })
console.log('macOS artifact verifier tests passed')
