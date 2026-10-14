#!/usr/bin/env node
// Replay the ATAHENA_PTY_TEE capture files (one `<pane>.bin` per pane) through
// @xterm/headless and snapshot the final buffers so renderer bugs become
// permanent, repeatable tests. This is the corpus counterpart to `pty_debug_tee`.
//
// Usage:
//   node scripts/replay-pty-tee.mjs <tee-dir-or-file>

import { readFileSync, readdirSync, statSync, writeFileSync } from 'node:fs'
import { basename, join } from 'node:path'
import { Terminal } from '@xterm/headless'

const [,, targetArg] = process.argv
if (!targetArg) {
  console.error('usage: node scripts/replay-pty-tee.mjs <tee-dir-or-file>')
  process.exit(2)
}

const stat = statSync(targetArg)
const files = stat.isDirectory()
  ? readdirSync(targetArg).filter((f) => f.endsWith('.bin')).map((f) => join(targetArg, f))
  : [targetArg]

const failures = []

for (const path of files) {
  const bytes = readFileSync(path)
  const term = new Terminal({ cols: 160, rows: 48, scrollback: 10_000, allowProposedApi: true })
  const writePromise = new Promise((resolve) => term.write(bytes, resolve))
  await writePromise

  const active = term.buffer.active
  const lines = []
  for (let i = 0; i < active.length; i += 1) {
    lines.push(active.getLine(i)?.translateToString(true) ?? '')
  }

  const state = {
    file: basename(path),
    cursorX: active.cursorX,
    cursorY: active.cursorY,
    baseY: active.baseY,
    length: active.length,
    // Any U+FFFD leak means the feed sequence broke a parser invariant.
    replacementGlyphs: lines.join('\n').split('\u{fffd}').length - 1,
  }
  writeFileSync(`${path}.snapshot.json`, JSON.stringify(state, null, 2))
  if (state.replacementGlyphs > 0) failures.push(state.file)
  console.log(`${state.file}: ${state.replacementGlyphs} replacement glyphs`)
}

if (failures.length) {
  console.error(`corrupt streams: ${failures.join(', ')}`)
  process.exit(1)
}
