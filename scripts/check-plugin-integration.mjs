#!/usr/bin/env node
import { execFileSync } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'

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

console.log('Plugin integration checks passed.')
