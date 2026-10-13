#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { readFileSync } from 'node:fs'

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)))
const run = (command, args) => {
  // On Windows, npx is a `npx.cmd` shim — execFileSync cannot resolve
  // .cmd files without shell:true. Use a string command so the shell
  // resolves it on every platform.
  execFileSync(process.platform === 'win32' ? `${command} ${args.join(' ')}` : command, process.platform === 'win32' ? [] : args, {
    cwd: root,
    stdio: 'inherit',
    ...(process.platform === 'win32' ? { shell: true } : {}),
  })
}

run('npx', [
  'eslint',
  'plugins',
  '--ext',
  '.ts,.tsx',
])
run(process.execPath, ['--check', 'bin/mcp-proxy.js'])

// Keep the runtime trust boundary explicit. This is a policy assertion, not
// a claim that arbitrary plugin code is sandboxed.
const pluginRuntime = readFileSync(path.join(root, 'crates/athena-plugins/src/lib.rs'), 'utf8')
if (!pluginRuntime.includes('PUBLIC_PLUGIN_TRUST_POLICY: &str = "trusted_developer_integrations"')) {
  throw new Error('Plugin trust policy has drifted.')
}

console.log('Plugin integration and public trust-policy checks passed.')
