#!/usr/bin/env node

import { existsSync, readFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = resolve(fileURLToPath(new URL('..', import.meta.url)))
const read = (relative) => readFileSync(resolve(root, relative), 'utf8')
const failures = []

// Extract the argument text of every `log::level!(...)` macro call.
// The previous `\([\s\S]*?\);` match over-captured when a log call is nested
// inside a closure or `map_err`/`inspect_err` — e.g. `.inspect_err(|_|
// log::warn!("...")?;` — because the macro's closing `)` is followed by `?;`
// (or `,;`/`:;`) rather than `);`. The lazy match then swallowed following
// statements (such as `validate_shell(&shell)`), producing false positives.
// Track parenthesis depth and skip string literals so each capture ends at
// the macro's own closing paren.
function extractLogCalls(source) {
  const calls = []
  const re = /log::(?:trace|debug|info|warn|error)!/g
  let match
  while ((match = re.exec(source)) !== null) {
    let i = match.index + match[0].length
    while (i < source.length && /\s/.test(source[i])) i += 1
    if (source[i] !== '(') continue
    const start = i + 1
    let depth = 1
    let j = start
    while (j < source.length && depth > 0) {
      const ch = source[j]
      if (ch === '"') {
        j += 1
        while (j < source.length) {
          if (source[j] === '\\') {
            j += 2
            continue
          }
          if (source[j] === '"') {
            j += 1
            break
          }
          j += 1
        }
        continue
      }
      if (ch === "'") {
        // Rust char literal: '\\', '\'', or a single character.
        if (source[j + 1] === '\\') j += 4 // e.g. '\n', '\\'
        else j += 3
        continue
      }
      if (ch === 'r') {
        // Rust raw string: r"..." / r#"..."# etc. Parens inside must not
        // perturb the depth accounting.
        let hashes = 0
        while (source[j + 1 + hashes] === '#') hashes += 1
        if (source[j + 1 + hashes] === '"') {
          const terminator = '"' + '#'.repeat(hashes)
          const end = source.indexOf(terminator, j + 2 + hashes)
          j = end === -1 ? source.length : end + terminator.length
          continue
        }
      }
      if (ch === '(') depth += 1
      else if (ch === ')') depth -= 1
      j += 1
    }
    calls.push(source.slice(start, j - 1))
  }
  return calls
}

// Remove double-quoted string literals (honoring backslash escapes) so the
// remaining text contains only argument expressions. This lets the positional
// check find `cwd`/`shell` values without tripping over prose inside strings.
function stripStringLiterals(text) {
  let out = ''
  let i = 0
  while (i < text.length) {
    if (text[i] === '"') {
      i += 1
      while (i < text.length) {
        if (text[i] === '\\') {
          i += 2
          continue
        }
        if (text[i] === '"') {
          i += 1
          break
        }
        i += 1
      }
      out += ' '
      continue
    }
    if (text[i] === "'") {
      // Rust char literal ('\\', '\'', or one character).
      if (text[i + 1] === '\\') i += 4
      else i += 3
      out += ' '
      continue
    }
    if (text[i] === 'r') {
      let hashes = 0
      while (text[i + 1 + hashes] === '#') hashes += 1
      if (text[i + 1 + hashes] === '"') {
        const terminator = '"' + '#'.repeat(hashes)
        const end = text.indexOf(terminator, i + 2 + hashes)
        i = end === -1 ? text.length : end + terminator.length
        out += ' '
        continue
      }
    }
    out += text[i]
    i += 1
  }
  return out
}

// Returns true if a captured log call serializes any of `fields` — either via
// inline named interpolation (`{field}` / `{field:?}`) or as a positional
// argument in any position. String literals are stripped for the positional
// check so prose inside the format string is ignored, and `.len()` metadata
// accesses are allowed.
function serializesField(call, fields) {
  const alternatives = fields.join('|')
  const inline = new RegExp(`\\{\\s*(?:${alternatives})\\b`, 'i').test(call)
  const positional = new RegExp(`\\b(?:${alternatives})\\b(?!\\.len\\()`, 'i').test(stripStringLiterals(call))
  return inline || positional
}

const agentComms = read('crates/athena-core/src/agent_comms_connection.rs')
// assistant_logger.rs is unreferenced dead code slated for removal; keep the
// invariant enforced only while the file exists.
const ASSISTANT_LOGGER = 'frontend/src/utils/assistant_logger.rs'
const assistantLogger = existsSync(resolve(root, ASSISTANT_LOGGER)) ? read(ASSISTANT_LOGGER) : null
const fileTree = read('frontend/src/components/sidebar_dir/file_tree.rs')
const pty = read('src-tauri/src/commands/pty.rs')

const required = [
  [agentComms, /event emitted on channel \{channel\}/, 'Agent Comms fallback log is metadata-only'],
  [agentComms, /received request_id_bytes=\{\} agent_id_bytes=\{\}/, 'Agent input-request log excludes title/prompt'],
  ...(assistantLogger !== null
    ? [[assistantLogger, /\[assistant\] action=\{\} level=\{\}/, 'assistant native log is metadata-only']]
    : []),
  [fileTree, /Failed to read file from the active workspace/, 'file-tree read failure is metadata-only'],
  [pty, /pty_spawn requested: session=\{\} cols=\{\} rows=\{\}/, 'PTY spawn request log excludes cwd/shell'],
]
for (const [source, pattern, description] of required) {
  if (!pattern.test(source)) failures.push(description)
}

// Check the exact native-log call sites rather than scanning arbitrary nearby
// comments or structured event construction. This keeps the guard stable as
// the implementation evolves without mistaking in-memory payload fields for
// serialized native diagnostics.
const agentLogCalls = extractLogCalls(agentComms)
const userPayloadFields = ['title', 'prompt', 'message', 'data', 'notif', 'payload']
const identifierFields = ['request_id', 'agent_id', 'plugin_id', 'session\\.id', 'session\\.agent_id', 'session\\.plugin_id', 'msg\\.method', 'level']
for (const call of agentLogCalls) {
  if (serializesField(call, userPayloadFields)) {
    failures.push('Agent Comms log serializes user payload')
  }
  if (serializesField(call, identifierFields)) {
    failures.push('Agent Comms log serializes client-controlled identifiers')
  }
}

const assistantLogCalls = assistantLogger !== null ? extractLogCalls(assistantLogger) : []
if (assistantLogCalls.some((call) => /\bmessage\b(?!\.len\(\))/i.test(call))) {
  failures.push('assistant logger forwards message to native logs')
}

if (extractLogCalls(fileTree).some((call) => /\bfile_path\b/.test(call))) {
  failures.push('file-tree log serializes an absolute path')
}

const ptyLogCalls = extractLogCalls(pty)
if (ptyLogCalls.some((call) => serializesField(call, ['cwd', 'shell']))) {
  failures.push('PTY log serializes cwd or shell')
}

if (failures.length) {
  console.error(`Release privacy checks failed: ${failures.join('; ')}`)
  process.exit(1)
}

console.log(`Release privacy checks passed (${required.length + agentLogCalls.length + assistantLogCalls.length + 2} invariants).`)
