#!/usr/bin/env node

import { createHash } from 'node:crypto'
import { existsSync, readFileSync, mkdtempSync, rmSync } from 'node:fs'
import { basename, join } from 'node:path'
import { execFileSync, spawnSync } from 'node:child_process'
import { tmpdir } from 'node:os'

function usage() {
  console.log(`Usage: node scripts/verify-macos-artifact.mjs --artifact <path> [options]

Options:
  --artifact <path>          DMG to verify (required)
  --sha256 <path>            Compare against a file containing an expected SHA-256
  --expected-name <name>     Require the artifact filename to match exactly
  --require-app              Require exactly one .app bundle in the DMG
  --require-arm64            Require the app executable to contain an arm64 slice
  --require-signing          Verify the mounted app with codesign and spctl
  --require-ad-hoc           Require a valid ad-hoc signature (code signed with '-'):
                             hard-fails on any codesign integrity problem; the
                             expected Gatekeeper rejection for unsigned-by-Apple
                             builds is logged but not fatal
  --require-notarization     Verify a stapled notarization ticket
  --help                     Show this help
`)
}

function argumentValue(args, name) {
  const index = args.indexOf(name)
  return index === -1 ? undefined : args[index + 1]
}

function run(command, args) {
  return execFileSync(command, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).trim()
}

function runCaptureAll(command, args) {
  const result = spawnSync(command, args, { encoding: 'utf8' })
  return { status: result.status, output: `${result.stdout ?? ''}${result.stderr ?? ''}` }
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex')
}

const args = process.argv.slice(2)
if (args.includes('--help')) {
  usage()
  process.exit(0)
}

const artifact = argumentValue(args, '--artifact')
const expectedShaPath = argumentValue(args, '--sha256')
const expectedName = argumentValue(args, '--expected-name')
const requireApp = args.includes('--require-app')
const requireArm64 = args.includes('--require-arm64')
const requireSigning = args.includes('--require-signing')
const requireAdHoc = args.includes('--require-ad-hoc')
const requireNotarization = args.includes('--require-notarization')

if (!artifact) {
  console.error('error: --artifact is required')
  usage()
  process.exit(2)
}
if (!existsSync(artifact)) {
  console.error(`error: artifact does not exist: ${artifact}`)
  process.exit(2)
}
if (expectedName && basename(artifact) !== expectedName) {
  console.error(`error: expected artifact name ${expectedName}, got ${basename(artifact)}`)
  process.exit(1)
}
if (!artifact.toLowerCase().endsWith('.dmg')) {
  console.error(`error: expected a .dmg artifact, got ${artifact}`)
  process.exit(1)
}

const digest = sha256(artifact)
console.log(`artifact: ${artifact}`)
console.log(`sha256: ${digest}`)

if (expectedShaPath) {
  if (!existsSync(expectedShaPath)) {
    console.error(`error: checksum file does not exist: ${expectedShaPath}`)
    process.exit(2)
  }
  const expected = readFileSync(expectedShaPath, 'utf8').match(/[a-f0-9]{64}/i)?.[0]?.toLowerCase()
  if (!expected) {
    console.error(`error: no SHA-256 digest found in ${expectedShaPath}`)
    process.exit(1)
  }
  if (expected !== digest) {
    console.error(`error: checksum mismatch (expected ${expected})`)
    process.exit(1)
  }
  console.log('checksum: match')
}

try {
  run('hdiutil', ['verify', artifact])
  console.log('dmg: hdiutil verify passed')
} catch (error) {
  console.error('error: hdiutil verify failed')
  console.error(error.stderr?.toString() ?? error.message)
  process.exit(1)
}

if (requireApp || requireArm64 || requireSigning || requireNotarization || requireAdHoc) {
  const mountPoint = mkdtempSync(join(tmpdir(), 'athena-dmg-'))
  let attached = false
  try {
    run('hdiutil', ['attach', '-nobrowse', '-readonly', '-mountpoint', mountPoint, artifact])
    attached = true
    // Use a NUL-delimited find result so a malformed bundle name containing
    // a newline cannot be miscounted or alter the verifier's path parsing.
    const apps = run('find', [mountPoint, '-maxdepth', '2', '-name', '*.app', '-type', 'd', '-print0'])
      .split('\0')
      .filter(Boolean)
    if (apps.length !== 1) {
      throw new Error(`expected exactly one .app in DMG, found ${apps.length}`)
    }
    const app = apps[0]
    console.log(`app: ${app}`)

    if (requireArm64) {
      const plist = join(app, 'Contents', 'Info.plist')
      const executableName = run('plutil', [
        '-extract',
        'CFBundleExecutable',
        'raw',
        '-o',
        '-',
        plist,
      ])
      if (!executableName || executableName.includes('/') || executableName.includes('\0')) {
        throw new Error('app bundle has an invalid CFBundleExecutable value')
      }
      if (
        !executableName
        || executableName !== basename(executableName)
        || /[\u0000\r\n]/.test(executableName)
      ) {
        throw new Error('app bundle has an invalid CFBundleExecutable value')
      }
      const executable = join(app, 'Contents', 'MacOS', executableName)
      if (!existsSync(executable)) {
        throw new Error(`app executable does not exist: ${executable}`)
      }
      const executableInfo = run('file', [executable])
      if (!/\barm64(?:e)?\b/i.test(executableInfo)) {
        throw new Error(`app executable is not arm64-capable: ${executableInfo}`)
      }
      console.log(`architecture: arm64 (${executable})`)
    }

    if (requireSigning) {
      run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', app])
      run('spctl', ['--assess', '--type', 'execute', '--verbose', app])
      console.log('signing: codesign and spctl passed')
    }
    if (requireAdHoc) {
      // Hard gate: the bundle signature must be complete and valid. The broken
      // v3.3.0 DMG (Mach-O linkedit signature with no sealed resources and no
      // Contents/_CodeSignature) fails right here — that is the "damaged, move
      // to Bin" class of failure and must never ship again.
      run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', app])
      // Hard gate: this branch must be ad-hoc signed (identity "-"), never
      // misrouted into the Developer-ID branch or left unsigned.
      const details = runCaptureAll('codesign', ['-dvv', app])
      if (details.status !== 0 || !/\bSignature=adhoc\b/.test(details.output)) {
        throw new Error(
          `expected an ad-hoc signature (Signature=adhoc) on the app, got: ${details.output.trim()}`,
        )
      }
      console.log('signing: codesign --verify --deep --strict passed (Signature=adhoc)')
      // Informational: Gatekeeper assessment. An ad-hoc signed app is NEVER
      // accepted by spctl (no Developer ID, no notarization ticket), so a
      // rejection here is expected and NOT fatal — users launch the app via
      // right-click -> Open once the signature itself is valid.
      const assess = runCaptureAll('spctl', ['--assess', '--type', 'execute', '--verbose', app])
      console.log(
        `gatekeeper: spctl exited ${assess.status} (expected non-zero for ad-hoc builds; right-click -> Open bypass applies once the signature is valid)`,
      )
    }
    if (requireNotarization) {
      run('xcrun', ['stapler', 'validate', app])
      console.log('notarization: stapler validate passed')
    }
  } catch (error) {
    console.error(`error: ${error.message}`)
    if (error.stderr) console.error(error.stderr.toString())
    process.exitCode = 1
  } finally {
    if (attached) {
      try { run('hdiutil', ['detach', mountPoint, '-quiet']) } catch { /* best effort cleanup */ }
    }
    rmSync(mountPoint, { recursive: true, force: true })
  }
}
