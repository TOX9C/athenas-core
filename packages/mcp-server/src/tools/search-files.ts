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

const HARD_LIMIT = 500

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

async function executeSearch(input: SearchFilesInput) {
  const rgBin = await findRgBinary()
  if (!rgBin) {
    return {
      isError: true as const,
      content: [
        {
          type: 'text' as const,
          text: 'ripgrep binary not found. Install rg via your package manager (e.g. brew install ripgrep, apt install ripgrep, choco install ripgrep) or ensure @vscode/ripgrep is installed.',
        },
      ],
    }
  }

  const maxResults = Math.min(input.max_results, HARD_LIMIT)
  const contextLines = input.context_lines

  const args: string[] = [
    '--json',
    '--with-filename',
    '--line-number',
    '--column',
    '--color=never',
    '--binary',
    '--max-columns=500',
    '--max-columns-preview',
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

  args.push('--', input.pattern, input.path)

  return new Promise<{ isError?: boolean; content: Array<{ type: 'text'; text: string }> }>(
    (resolve) => {
      const proc = spawn(rgBin, args, {
        cwd: input.path,
        env: { ...process.env, LC_ALL: 'en_US.UTF-8' },
      })

      let stdout = ''
      let stderr = ''

      proc.stdout.on('data', (data: Buffer) => {
        stdout += data.toString()
      })

      proc.stderr.on('data', (data: Buffer) => {
        stderr += data.toString()
      })

      proc.on('close', (code) => {
        if (code !== 0 && code !== 1) {
          resolve({
            isError: true,
            content: [{ type: 'text', text: `ripgrep exited with code ${code}: ${stderr.trim()}` }],
          })
          return
        }

        const matches: MatchEntry[] = []
        const filesMatched = new Set<string>()
        let truncated = false

        const lines = stdout.split('\n').filter((l) => l.trim())
        const pendingContext: Array<{ lineNum: number; filePath: string; text: string }> = []

        for (const line of lines) {
          if (truncated) break
          try {
            const parsed = JSON.parse(line)

            if (parsed.type === 'context') {
              const data = parsed.data
              pendingContext.push({
                lineNum: data.line_number,
                filePath: data.path?.text ?? '',
                text: (data.lines?.text ?? '').trimEnd(),
              })
              continue
            }

            if (parsed.type !== 'match') continue

            const data = parsed.data
            const filePath = data.path?.text ?? ''
            const lineNum = data.line_number
            const col = data.submatches?.[0]?.start ?? 1
            const lineText = (data.lines?.text ?? '').trimEnd()
            const matchText = data.submatches?.[0]?.match?.text ?? ''

            filesMatched.add(filePath)

            const contextBefore = pendingContext
              .filter(
                (c) =>
                  c.filePath === filePath &&
                  c.lineNum < lineNum &&
                  c.lineNum >= lineNum - contextLines,
              )
              .map((c) => c.text)
            const contextAfter = pendingContext
              .filter(
                (c) =>
                  c.filePath === filePath &&
                  c.lineNum > lineNum &&
                  c.lineNum <= lineNum + contextLines,
              )
              .map((c) => c.text)

            pendingContext.length = 0

            matches.push({
              filePath,
              lineNumber: lineNum,
              column: col,
              lineText,
              matchText,
              contextBefore,
              contextAfter,
            })

            if (matches.length >= maxResults) {
              truncated = true
            }
          } catch {
            // skip malformed JSON lines
          }
        }

        if (matches.length === 0) {
          resolve({
            content: [
              {
                type: 'text',
                text: `No matches found for pattern "${input.pattern}" in ${input.path}.`,
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

        const header = `Found ${matches.length} matches in ${filesMatched.size} files${truncated ? ' (truncated — increase max_results for more)' : ''}:\n\n`

        resolve({ content: [{ type: 'text', text: header + formatted }] })
      })

      proc.on('error', (err) => {
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
