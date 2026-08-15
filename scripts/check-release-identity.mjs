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
if (process.env.CI && !suppliedVersion) failures.push('CI release identity requires RELEASE_VERSION or GITHUB_REF_NAME')
const packageJson = json('package.json')
const tauri = json('src-tauri/tauri.conf.json')
const frontendCargo = read('frontend/Cargo.toml')
const tauriCargo = read('src-tauri/Cargo.toml')
const readme = read('README.md')
const scope = read('docs/release/RELEASE_SCOPE.md')
const privacy = read('docs/release/PRIVACY_NOTICE.md')
const entitlements = read('src-tauri/entitlements.plist')

const cargoVersion = (source) => source.match(/^version\s*=\s*"([^"]+)"/m)?.[1]
const metadata = [
  ['package.json', packageJson.version],
  ['src-tauri/tauri.conf.json', tauri.version],
  ['frontend/Cargo.toml', cargoVersion(frontendCargo)],
  ['src-tauri/Cargo.toml', cargoVersion(tauriCargo)],
]
for (const [name, version] of metadata) {
  if (version !== tag) failures.push(`${name} version ${version ?? '<missing>'} != ${tag}`)
}

if (tauri.productName !== "Athena's Core") failures.push('product name is not final')
if (tauri.identifier !== 'com.athena.core') failures.push('bundle identifier is not final')
if (tauri.bundle?.macOS?.minimumSystemVersion !== '13.0') failures.push('minimum macOS version must be 13.0')
if (tauri.bundle?.macOS?.entitlements !== 'entitlements.plist') failures.push('production entitlements path is not configured')
if (!/<dict\s*\/>/.test(entitlements)) failures.push('production entitlements must remain empty until a capability is justified')
if (tauri.bundle?.targets !== 'dmg' && !(Array.isArray(tauri.bundle?.targets) && tauri.bundle.targets.length === 1 && tauri.bundle.targets[0] === 'dmg')) {
  failures.push('public macOS scope must package only the DMG target')
}
if (!/macOS on Apple Silicon/.test(readme) || !/Apple Silicon macOS first/.test(scope)) {
  failures.push('Apple Silicon macOS scope is not documented consistently')
}
if (!/Mobile Mirror.*experimental.*disabled by default/i.test(scope) || !/Mobile Mirror.*experimental.*plaintext/i.test(privacy)) {
  failures.push('Mobile Mirror exclusion/trust warning is missing')
}
if (!/No in-app updater is shipped/i.test(scope)) {
  failures.push('no-updater scope is not recorded')
}
if (/Windows\/Linux are out of scope/.test(scope) && /- \*\*Linux\*\*|\*\*Windows\*\*/.test(readme)) {
  // Development prerequisites may mention these platforms, but they must be
  // explicitly labeled as non-release platforms in the README.
  if (!/not release artifacts/i.test(readme)) failures.push('README advertises non-release platforms without a scope disclaimer')
}

if (failures.length) {
  console.error(`Release identity checks failed: ${failures.join('; ')}`)
  process.exit(1)
}
console.log(`Release identity checks passed for Athena's Core ${tag} (macOS Apple Silicon DMG scope).`)
