#!/usr/bin/env node

import { readFileSync } from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)))
const read = (relativePath) => readFileSync(path.join(root, relativePath), 'utf8')

const main = read('src-tauri/src/main.rs')
const config = JSON.parse(read('src-tauri/tauri.conf.json'))
const capability = JSON.parse(read('src-tauri/capabilities/default.json'))
const csp = config.app?.security?.csp ?? ''
// Match the exact directive name: a bare startsWith('script-src') would also
// hit script-src-elem / script-src-attr and silently check the wrong
// directive if either is ever added.
const findDirective = (name) =>
  csp
    .split(';')
    .find((directive) => directive.trim().split(/\s+/)[0] === name)
    ?.trim() ?? ''
const scriptSrc = findDirective('script-src')
const connectSrc = findDirective('connect-src')
const scriptTokens = scriptSrc.split(/\s+/).slice(1)
const connectTokens = connectSrc.split(/\s+/).slice(1)

const checks = [
  [
    'WebDriver is debug-only',
    // Accept `#[cfg(debug_assertions)]` or the stricter
    // `#[cfg(all(debug_assertions, feature = "..."))]` before init.
    /#\[cfg\((?:debug_assertions|all\(debug_assertions[^)]*\))\)\][\s\S]*tauri_plugin_webdriver_automation::init\(\)/.test(main),
  ],
  [
    // The autostart path is valid if `relay::start` sits under a cfg gate
    // containing debug_assertions (plain or inside `all(...)`), or the
    // stub-fn shape `#[cfg(not(debug_assertions))] fn relay_autostart_requested() -> bool { false }`.
    'relay autostart is compiled out of release builds',
    /#\[cfg\(not\(debug_assertions\)\)\][\s\S]*fn relay_autostart_requested\(\) -> bool \{\s*false/.test(main)
      || /#\[cfg\((?:debug_assertions|all\(debug_assertions[^)]*\))\)\][\s\S]*relay::start\(/.test(main),
  ],
  [
    'default capability has no broad shell execute permission',
    !capability.permissions.some((permission) => /shell:.*execute|shell-.*execute/.test(permission)),
  ],
  [
    'CSP has a self-only script baseline with wasm support',
    scriptTokens.includes("'self'")
      && scriptTokens.includes("'wasm-unsafe-eval'")
      && !scriptTokens.includes("'unsafe-eval'"),
  ],
  [
    'CSP connect sources are restricted to app IPC',
    connectTokens.includes("'self'")
      && connectTokens.includes('ipc:')
      && !connectTokens.some((token) => /^(https?:|ws:|wss:)/.test(token)),
  ],
]

const failures = checks.filter(([, passed]) => !passed).map(([name]) => name)
if (failures.length) {
  console.error(`Tauri security config checks failed: ${failures.join('; ')}`)
  process.exit(1)
}

console.log(`Tauri security config checks passed (${checks.length} invariants).`)
