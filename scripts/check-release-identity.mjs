#!/usr/bin/env node

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const read = (relative) => readFileSync(resolve(root, relative), 'utf8')
const json = (relative) => JSON.parse(read(relative))
const suppliedVersion =
  process.env.RELEASE_VERSION ??
  (process.env.GITHUB_REF_TYPE === 'tag'
    ? process.env.GITHUB_REF_NAME?.replace(/^v/, '')
    : undefined)
const tag = suppliedVersion ?? '0.3.0'
const failures = []
const fail = (name, detail) => failures.push(`${name}: ${detail}`)

if (process.env.CI && !suppliedVersion) {
  fail('version source', 'CI release identity requires RELEASE_VERSION or GITHUB_REF_NAME')
}

const packageJson = json('package.json')
const tauri = json('src-tauri/tauri.conf.json')
const frontendCargo = read('frontend/Cargo.toml')
const tauriCargo = read('src-tauri/Cargo.toml')
const readme = read('README.md')
const entitlements = read('src-tauri/entitlements.plist')

const cargoVersion = (source) => source.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
const metadata = [
  ['package.json', packageJson.version],
  ['src-tauri/tauri.conf.json', tauri.version],
  ['frontend/Cargo.toml', cargoVersion(frontendCargo)],
  ['src-tauri/Cargo.toml', cargoVersion(tauriCargo)],
]
for (const [name, version] of metadata) {
  if (version !== tag) fail('version', `${name} version ${version ?? '<missing>'} != ${tag}`)
}

if (tauri.productName !== "Athena's Core") fail('product identity', 'product name is not final')
if (tauri.identifier !== 'com.athena.core') fail('product identity', 'bundle identifier is not final')
if (tauri.bundle?.macOS?.minimumSystemVersion !== '13.0') fail('macOS target', 'minimum macOS version must be 13.0')
if (tauri.bundle?.macOS?.entitlements !== 'entitlements.plist') fail('entitlements', 'production entitlements path is not configured')
if (!/<dict\s*\/>/.test(entitlements)) fail('entitlements', 'production entitlements must remain empty until a capability is justified')
if (tauri.bundle?.targets !== 'dmg' && !(Array.isArray(tauri.bundle?.targets) && tauri.bundle.targets.length === 1 && tauri.bundle.targets[0] === 'dmg')) {
  fail('artifact target', 'public macOS scope must package only the DMG target')
}
if (!/macOS(?: \d+(?:\.\d+)?\+)? on Apple Silicon/.test(readme)) {
  fail('README platform scope', 'README must document macOS 13+ on Apple Silicon')
}
if (failures.length) {
  console.error('Release identity checks failed:')
  for (const failure of failures) console.error(`- ${failure}`)
  process.exit(1)
}
console.log(`Release identity checks passed for Athena's Core ${tag} (macOS Apple Silicon DMG scope).`)
