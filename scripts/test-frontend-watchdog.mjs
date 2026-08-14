#!/usr/bin/env node

import { strict as assert } from 'node:assert'
import { readFileSync } from 'node:fs'

const html = readFileSync(new URL('../frontend/index.html', import.meta.url), 'utf8')
const appSource = readFileSync(new URL('../frontend/src/lib.rs', import.meta.url), 'utf8')
const mainSource = readFileSync(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8')

assert.match(html, /__athenaWasmHeartbeat/)
assert.match(html, /no WASM heartbeat for/)
assert.match(html, /document\.visibilityState === 'hidden'/)
assert.match(html, /document\.visibilityState === 'visible'/)
assert.match(html, /athena-wasm-watchdog-stabilizing/)
assert.match(html, /sessionStorage\.setItem\(reloadKey, '1'\)/)
assert.match(html, /sessionStorage\.removeItem\(stabilizationKey\)/)

// The watchdog must not depend on recent clicks/user input to detect a freeze.
assert.doesNotMatch(html, /now - lastUserActivity < silenceLimitMs/)

// Keep recoverable boundaries around both desktop and mobile high-risk UI.
assert.match(appSource, /fallback_message: "The mobile interface could not be rendered\./)
assert.match(appSource, /fallback_message: "The editor could not be rendered\./)
assert.match(appSource, /fallback_message: "The browser panel could not be rendered\./)
assert.match(appSource, /fallback_message: "A background interface component could not be rendered\./)

// The experimental plaintext LAN relay must not reopen from persisted state
// during a normal public launch.
assert.match(mainSource, /ATHENA_RELAY_AUTOSTART/)
assert.match(mainSource, /#\[cfg\(debug_assertions\)\][\s\S]*ATHENA_RELAY_AUTOSTART/)
assert.match(mainSource, /#\[cfg\(not\(debug_assertions\)\)\][\s\S]*fn relay_autostart_requested\(\) -> bool \{\s*false/)
assert.match(mainSource, /persisted_enabled && explicit_autostart/)
assert.match(mainSource, /explicit ATHENA_RELAY_AUTOSTART=1 is required/)

console.log('frontend watchdog and recovery source checks passed')
