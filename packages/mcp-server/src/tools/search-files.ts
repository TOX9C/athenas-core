import { z } from 'zod'
import { spawn } from 'child_process'

export const searchFilesSchema = z.object({
  pattern: z.string().min(1).describe('The search pattern (supports regex). Required.'),
  path: z.string().min(1).describe('The directory to search in. Required.'),
  glob: z.string().optional().describe('File glob filter (e.g., "*.ts", "*.{js,jsx}"). Optional.'),
  type: z.string().optional().describe('File type filter (e.g., "ts", "py", "rust"). Optional.'),
  case_sensitive: z
    .boolean()
    .default(false)
    .describe('Whether the search should be case sensitive. Defaults to false.'),
  max_results: z
    .number()
    .int()
    .min(1)
    .max(500)
    .default(100)
    .describe('Maximum number of results to return. Defaults to 100, hard cap 500.'),
  context_lines: z
    .number()
    .int()
    .min(0)
    .max(10)
    .default(2)
    .describe('Number of context lines around each match. Defaults to 2.'),
})

import path from 'path'
import { realpath } from 'fs/promises'

const WORKSPACE_ROOT = process.cwd()

export type SearchFilesInput = z.infer<typeof searchFilesSchema>

interface MatchEntry {
  filePath: string
  lineNumber: number
  column: number
  lineText: string
  matchText: string
  contextBefore: string[]
  contextAfter: string[]
}

async function findRgBinary(): Promise<string | null> {
  try {
    const mod = await import('@vscode/ripgrep')
    if (mod.rgPath) return mod.rgPath
  } catch {
    // not installed — fall through
  }

  const { access, constants } = await import('fs/promises')
  const candidates =
    process.platform === 'win32'
      ? ['rg.exe', 'C:\\ProgramData\\chocolatey\\bin\\rg.exe']
      : ['rg', '/usr/local/bin/rg', '/opt/homebrew/bin/rg', '/usr/bin/rg']

  for (const candidate of candidates) {
    try {
      await access(candidate, constants.X_OK)
      return candidate
    } catch {
      continue
    }
  }
  return null
}

/** Hard cap on a single search run — belt-and-braces on top of the schema's
 * `.max()`, plus a per-file cap passed to ripgrep (`--max-count`). */
const MAX_SEARCH_RESULTS = 500
/** Kill ripgrep after this long; prior (unbounded) runs could hang the tool
 * forever and buffer unbounded output in memory. */
const SEARCH_TIMEOUT_MS = 30_000

async function executeSearch(input: SearchFilesInput) {
  const errorResult = (text: string) => ({
    isError: true as const,
    content: [{ type: 'text' as const, text }],
  })

  // Pre-validate BEFORE spawning: resolve symlinks on both sides so a
  // workspace symlink pointing outside cannot be searched (F11).
  let realTarget: string
  try {
    const realRoot = await realpath(path.resolve(WORKSPACE_ROOT))
    const resolved = path.resolve(WORKSPACE_ROOT, input.path)
    if (resolved.length > 4096) {
      return errorResult('Path too long')
    }
    realTarget = await realpath(resolved)
    const relative = path.relative(realRoot, realTarget)
    if (relative.startsWith('..') || path.isAbsolute(relative)) {
      return errorResult(`Path traversal detected: ${input.path} is outside the workspace`)
    }
  } catch (err) {
    return errorResult(
      `Cannot access path ${input.path}: ${err instanceof Error ? err.message : String(err)}`,
    )
  }

  const rgBin = await findRgBinary()
  if (!rgBin) {
    return errorResult(
      'ripgrep binary not found. Install rg via your package manager (e.g. brew install ripgrep, apt install ripgrep, choco install ripgrep) or ensure @vscode/ripgrep is installed.',
    )
  }

  // Note: direct callers (outside the SDK schema parse) may omit optional
  // fields; fall back to the schema defaults here.
  const maxResults = Math.min(input.max_results ?? 100, MAX_SEARCH_RESULTS)
  const contextLines = input.context_lines ?? 2

  const args: string[] = [
    '--json',
    '--with-filename',
    '--line-number',
    '--column',
    '--color=never',
    '--binary',
    '--max-columns=500',
    '--max-columns-preview',
    // Per-file match cap so a hot file cannot dominate the whole budget.
    `--max-count=${maxResults}`,
  ]

  if (input.case_sensitive) {
    args.push('--case-sensitive')
  } else {
    args.push('--ignore-case')
  }

  if (contextLines > 0) {
    args.push('--context', String(contextLines))
  }

  if (input.glob) {
    args.push('--glob', input.glob)
  }

  if (input.type) {
    args.push('--type', input.type)
  }

  // Search the resolved absolute path; do NOT also chdir into it (relative
  // `path` values previously searched `<path>/<path>` and matched nothing).
  args.push('--', input.pattern, realTarget)

  return new Promise<{ isError?: boolean; content: Array<{ type: 'text'; text: string }> }>(
    (resolve) => {
      const proc = spawn(rgBin, args, {
        env: { ...process.env, LC_ALL: 'en_US.UTF-8' },
      })

      const matches: MatchEntry[] = []
      const filesMatched = new Set<string>()
      let truncated = false
      let timedOut = false
      let finished = false
      let stderr = ''

      // Streaming parse state. rg emits begin/match/context/end per file; we
      // consume line-by-line so memory stays bounded regardless of tree size.
      let lineBuf = ''
      let currentFile = ''
      // Context lines awaiting a following match (candidates for contextBefore).
      let pendingBefore: Array<{ lineNum: number; text: string }> = []
      // Recent matches still eligible to receive contextAfter lines.
      const openMatches: MatchEntry[] = []
      let lastMatchLine = 0

      const killProc = () => {
        if (!proc.killed) proc.kill('SIGKILL')
      }

      const timeout = setTimeout(() => {
        timedOut = true
        truncated = truncated || matches.length > 0
        killProc()
      }, SEARCH_TIMEOUT_MS)

      const finish = (code: number | null) => {
        if (finished) return
        finished = true
        clearTimeout(timeout)

        if (!timedOut && code !== 0 && code !== 1) {
          resolve({
            isError: true,
            content: [{ type: 'text', text: `ripgrep exited with code ${code}: ${stderr.trim()}` }],
          })
          return
        }

        if (matches.length === 0) {
          resolve({
            isError: timedOut || undefined,
            content: [
              {
                type: 'text',
                text: timedOut
                  ? `Search timed out after ${SEARCH_TIMEOUT_MS}ms with no results: ${stderr.trim() || 'ripgrep produced no output'}`
                  : `No matches found for pattern "${input.pattern}" in ${input.path}.`,
              },
            ],
          })
          return
        }

        const formatted = matches
          .map((m) => {
            let output = `${m.filePath}:${m.lineNumber}:${m.column}: ${m.lineText}`
            if (m.contextBefore.length > 0) {
              const before = m.contextBefore
                .map((l, i) => `  ${m.lineNumber - m.contextBefore.length + i}: ${l}`)
                .join('\n')
              output = before + '\n' + output
            }
            if (m.contextAfter.length > 0) {
              output +=
                '\n' + m.contextAfter.map((l, i) => `  ${m.lineNumber + 1 + i}: ${l}`).join('\n')
            }
            return output
          })
          .join('\n\n')

        const reasons = [
          truncated ? 'hit max_results' : '',
          timedOut ? `timed out after ${SEARCH_TIMEOUT_MS}ms` : '',
        ]
          .filter(Boolean)
          .join(', ')
        const header = `Found ${matches.length} matches in ${filesMatched.size} files${reasons ? ` (truncated: ${reasons} — refine the pattern for more)` : ''}:\n\n`

        resolve({ content: [{ type: 'text', text: header + formatted }] })
      }

      proc.stdout.on('data', (data: Buffer) => {
        lineBuf += data.toString()
        const lines = lineBuf.split('\n')
        lineBuf = lines.pop() ?? ''

        for (const line of lines) {
          if (!line.trim()) continue
          let parsed: {
            type: string
            data?: {
              path?: { text?: string }
              line_number?: number
              lines?: { text?: string }
              submatches?: Array<{ start: number; match?: { text?: string } }>
            }
          }
          try {
            parsed = JSON.parse(line)
          } catch {
            continue
          }

          if (parsed.type === 'begin') {
            currentFile = parsed.data?.path?.text ?? ''
            pendingBefore = []
            continue
          }

          if (parsed.type === 'end') {
            // No more context can follow for this file; stop once capped.
            if (truncated) killProc()
            continue
          }

          if (parsed.type === 'context') {
            const data = parsed.data
            if (!data) continue
            const lineNum = data.line_number ?? 0
            const filePath = data.path?.text ?? currentFile
            const text = (data.lines?.text ?? '').trimEnd()
            if (filePath !== currentFile) continue

            // Attach as contextAfter to any matching open match…
            for (const m of openMatches) {
              if (lineNum > m.lineNumber && lineNum <= m.lineNumber + contextLines) {
                m.contextAfter.push(text)
              }
            }
            // …and keep it as a contextBefore candidate for the next match.
            if (pendingBefore.length === contextLines) pendingBefore.shift()
            pendingBefore.push({ lineNum, text })

            // Once capped, only in-range trailing context is worth reading.
            if (truncated && lineNum > lastMatchLine + contextLines) killProc()
            continue
          }

          if (parsed.type !== 'match') continue

          if (truncated) {
            killProc()
            continue
          }

          const data = parsed.data
          if (!data) continue
          const filePath = data.path?.text ?? currentFile
          const lineNum = data.line_number ?? 0
          const col = data.submatches?.[0]?.start ?? 1
          const lineText = (data.lines?.text ?? '').trimEnd()
          const matchText = data.submatches?.[0]?.match?.text ?? ''

          filesMatched.add(filePath)

          const match: MatchEntry = {
            filePath,
            lineNumber: lineNum,
            column: col,
            lineText,
            matchText,
            // Context queued before this match within range is contextBefore.
            contextBefore: pendingBefore
              .filter((c) => c.lineNum < lineNum && c.lineNum >= lineNum - contextLines)
              .map((c) => c.text),
            contextAfter: [],
          }
          matches.push(match)
          // Only the latest match per file can still receive contextAfter
          // (lines arrive in order), so drop fully-closed predecessors.
          for (let i = openMatches.length - 1; i >= 0; i--) {
            if (lineNum > openMatches[i]!.lineNumber + contextLines) openMatches.splice(i, 1)
          }
          openMatches.push(match)
          lastMatchLine = lineNum

          if (matches.length >= maxResults) {
            truncated = true
            // Keep reading briefly to collect this match's contextAfter.
            if (contextLines === 0) killProc()
          }
        }
      })

      proc.stderr.on('data', (data: Buffer) => {
        if (stderr.length < 64 * 1024) stderr += data.toString()
      })

      proc.on('close', finish)
      proc.on('error', (err) => {
        if (finished) return
        finished = true
        clearTimeout(timeout)
        resolve({
          isError: true,
          content: [{ type: 'text', text: `Failed to spawn ripgrep: ${err.message}` }],
        })
      })
    },
  )
}

export async function searchFiles(_bridge: unknown, input: SearchFilesInput) {
  return executeSearch(input)
}
