#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { readFileSync } from 'node:fs'

const root = path.dirname(path.dirname(fileURLToPath(import.meta.url)))
const run = (command, args) => {
  execFileSync(command, args, {
    cwd: root,
    stdio: 'inherit',
  })
}

run('npx', [
  'eslint',
  'plugins',
  '--ext',
  '.ts,.tsx',
])
run(process.execPath, ['--check', 'bin/mcp-proxy.js'])

// Keep the public trust boundary explicit in both the release policy and the
// runtime crate. This is a policy assertion, not a claim that arbitrary plugin
// code is sandboxed.
const policy = readFileSync(path.join(root, 'docs/release/PLUGIN_TRUST_POLICY.md'), 'utf8').toLowerCase()
const pluginRuntime = readFileSync(path.join(root, 'crates/athena-plugins/src/lib.rs'), 'utf8')
if (!policy.includes('trusted_developer_integrations')
  || !policy.includes('sandbox')
  || !policy.includes('marketplace')
  || !policy.includes('not included')
  || !pluginRuntime.includes('PUBLIC_PLUGIN_TRUST_POLICY: &str = "trusted_developer_integrations"')) {
  throw new Error('Public plugin trust policy is missing or has drifted.')
}

console.log('Plugin integration and public trust-policy checks passed.')
