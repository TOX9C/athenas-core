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
const scriptSrc = csp.split(';').find((directive) => directive.trim().startsWith('script-src'))?.trim() ?? ''
const connectSrc = csp.split(';').find((directive) => directive.trim().startsWith('connect-src'))?.trim() ?? ''
const scriptTokens = scriptSrc.split(/\s+/).slice(1)
const connectTokens = connectSrc.split(/\s+/).slice(1)

const checks = [
  [
    'WebDriver is debug-only',
    /#\[cfg\(debug_assertions\)\][\s\S]*tauri_plugin_webdriver_automation::init\(\)/.test(main),
  ],
  [
    'relay autostart is compiled out of release builds',
    /#\[cfg\(not\(debug_assertions\)\)\][\s\S]*fn relay_autostart_requested\(\) -> bool \{\s*false/.test(main),
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
