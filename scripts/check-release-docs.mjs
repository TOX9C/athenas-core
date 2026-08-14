#!/usr/bin/env node

import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const read = (relative) => readFileSync(resolve(root, relative), 'utf8')
const json = (relative) => JSON.parse(read(relative))

const readme = read('README.md')
const architecture = read('docs/ARCHITECTURE.md')
const migration = read('docs/MIGRATION_GUIDE.md')
const roadmap = read('ROADMAP.md')
const workflow = read('.github/workflows/release-macos.yml')
const tauri = json('src-tauri/tauri.conf.json')
const failures = []

const buildCommandsSource = read('src-tauri/build.rs')
const commandStart = buildCommandsSource.indexOf('const COMMANDS:')
const commandEnd = commandStart >= 0 ? buildCommandsSource.indexOf('];', commandStart) : -1
const commandManifest = commandStart >= 0 && commandEnd >= 0
  ? buildCommandsSource.slice(commandStart, commandEnd)
  : ''
const registeredCommandCount = (commandManifest.match(/"[a-zA-Z0-9_]+"/g) ?? []).length

const toolSchemaSource = read('crates/athena-core/src/tool_schema.rs')
const canonicalToolCount = (toolSchemaSource.match(/name: "[^"]+"\.to_string\(\)/g) ?? []).length
const protocolSource = read('crates/athena-core/src/mcp_protocol.rs')
const aliasStart = protocolSource.indexOf('filter(|tool|')
const aliasEnd = aliasStart >= 0 ? protocolSource.indexOf('})', aliasStart) : -1
const legacyAliasBlock = aliasStart >= 0 && aliasEnd >= 0
  ? protocolSource.slice(aliasStart, aliasEnd)
  : ''
const legacyMcpAliases = new Set(
  [...legacyAliasBlock.matchAll(/"([^"]+)"/g)].map((match) => match[1]),
).size
const expectedToolCount = canonicalToolCount + legacyMcpAliases

if (registeredCommandCount !== 133) {
  failures.push(`expected 133 registered commands, found ${registeredCommandCount}`)
}
if (!new RegExp(`${registeredCommandCount} IPC commands`).test(readme)) {
  failures.push('README command count is stale')
}
if (!new RegExp(`${registeredCommandCount} invoke_handlers`).test(architecture)) {
  failures.push('architecture diagram command count is stale')
}
if (!architecture.includes(`- \`commands/mod.rs\` — ${registeredCommandCount} \`#[tauri::command]\` functions`)) {
  failures.push('architecture command description count is stale')
}
if (!new RegExp(`exposes ${expectedToolCount} .* tools`).test(architecture)) {
  failures.push(`architecture MCP tool count must be ${expectedToolCount}`)
}
if (!migration.includes('clients should use `tools/list` for the live set')) {
  failures.push('migration guide does not explain that MCP clients should discover the live tool set')
}

const actualCsp = tauri.app?.security?.csp ?? ''
const cspDirectives = Object.fromEntries(
  actualCsp.split(';').map((directive) => {
    const tokens = directive.trim().split(/\s+/)
    return [tokens.shift(), new Set(tokens)]
  }),
)
const hasCspToken = (directive, token) => cspDirectives[directive]?.has(token) ?? false
if (!hasCspToken('script-src', "'self'") || !hasCspToken('script-src', "'wasm-unsafe-eval'") || hasCspToken('script-src', "'unsafe-eval'")) {
  failures.push('script-src CSP policy is inconsistent with the documented baseline')
}
if (!hasCspToken('connect-src', "'self'") || !hasCspToken('connect-src', 'ipc:') || [...(cspDirectives['connect-src'] ?? [])].some((token) => /^(https?:|ws:|wss:)/.test(token))) {
  failures.push('connect-src CSP policy is inconsistent with the documented baseline')
}
if (!roadmap.includes('inline styles and `data:`/`blob:` assets remain explicitly required')) {
  failures.push('ROADMAP does not document the intentional CSP allowances')
}
if (/removed `unsafe-inline`|no inline styles|removed `data:` and `blob:`/i.test(roadmap)) {
  failures.push('ROADMAP still claims CSP allowances were removed when they remain configured')
}

if (!workflow.includes('expected="Athena\'s Core_${RELEASE_VERSION}_aarch64.dmg"')) {
  failures.push('release workflow does not derive artifact naming from RELEASE_VERSION')
}
if (/expected="Athena's Core_\$\{GITHUB_REF_NAME#v\}_aarch64\.dmg"/.test(workflow)) {
  failures.push('release workflow still derives artifact naming directly from GITHUB_REF_NAME')
}
if (!workflow.includes('RELEASE_VERSION="$expected_version" node scripts/check-release-identity.mjs')) {
  failures.push('release workflow does not validate the selected dispatch/tag version')
}

if (failures.length) {
  console.error(`Release documentation checks failed: ${failures.join('; ')}`)
  process.exit(1)
}

console.log(`Release documentation checks passed (${registeredCommandCount} commands, ${expectedToolCount} discovered MCP tools).`)
