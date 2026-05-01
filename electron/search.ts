import { spawn } from 'child_process'

let rgPath: string | null = null

async function getRgBinaryPath(): Promise<string> {
  if (rgPath) return rgPath
  try {
    const mod = await import('@vscode/ripgrep')
    rgPath = mod.rgPath
    return rgPath
  } catch {
    throw new Error('ripgrep binary not found. Ensure @vscode/ripgrep is installed.')
  }
}

export interface SearchOptions {
  pattern: string
  path: string
  glob?: string
  type?: string
  caseSensitive?: boolean
  maxResults?: number
  contextLines?: number
}

export interface SearchMatch {
  filePath: string
  lineNumber: number
  column: number
  lineText: string
  matchText: string
  contextBefore: string[]
  contextAfter: string[]
}

export interface SearchResult {
  matches: SearchMatch[]
  truncated: boolean
  stats: {
    filesMatched: number
    totalMatches: number
  }
}

export async function searchCode(options: SearchOptions): Promise<SearchResult> {
  const rgBin = await getRgBinaryPath()

  const args: string[] = ['--json', '--with-filename', '--line-number', '--column', '--color=never']

  if (options.caseSensitive) {
    args.push('--case-sensitive')
  } else {
    args.push('--ignore-case')
  }

  if (options.maxResults) {
    args.push('--max-count', String(options.maxResults))
  }

  if (options.contextLines && options.contextLines > 0) {
    args.push('--context', String(options.contextLines))
  }

  if (options.glob) {
    args.push('--glob', options.glob)
  }

  if (options.type) {
    args.push('--type', options.type)
  }

  args.push('--', options.pattern, options.path)

  return new Promise<SearchResult>((resolve, reject) => {
    const proc = spawn(rgBin, args, {
      cwd: options.path,
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
        reject(new Error(`ripgrep exited with code ${code}: ${stderr}`))
        return
      }

      const matches: SearchMatch[] = []
      const filesMatched = new Set<string>()
      let truncated = false

      const lines = stdout.split('\n').filter((l) => l.trim())
      for (const line of lines) {
        try {
          const parsed = JSON.parse(line)
          if (parsed.type === 'match') {
            const data = parsed.data
            const filePath = data.path.text
            filesMatched.add(filePath)
            matches.push({
              filePath,
              lineNumber: data.line_number,
              column: data.submatches?.[0]?.start ?? 1,
              lineText: data.lines.text.trimEnd(),
              matchText: data.submatches?.[0]?.match?.text ?? '',
              contextBefore: [],
              contextAfter: [],
            })
            _matchIndex++
            if (options.maxResults && matches.length >= options.maxResults) {
              truncated = true
              break
            }
          } else if (parsed.type === 'context') {
            const data = parsed.data
            if (matches.length > 0) {
              const lastMatch = matches[matches.length - 1]
              if (data.line_number > lastMatch.lineNumber) {
                lastMatch.contextAfter.push(data.lines.text.trimEnd())
              }
            }
          } else if (parsed.type === 'summary') {
            if (parsed.data?.stats?.searched) {
              // Use summary stats if available
            }
          }
        } catch {
          // Skip malformed JSON lines
        }
      }

      resolve({
        matches,
        truncated,
        stats: {
          filesMatched: filesMatched.size,
          totalMatches: matches.length,
        },
      })
    })

    proc.on('error', (err) => {
      reject(new Error(`Failed to spawn ripgrep: ${err.message}`))
    })
  })
}

export async function searchFiles(
  directory: string,
  pattern: string,
  options?: { glob?: string; type?: string; maxResults?: number },
): Promise<string[]> {
  const rgBin = await getRgBinaryPath()

  const args: string[] = ['--files', '--color=never']

  if (options?.glob) {
    args.push('--glob', options.glob)
  }

  if (options?.type) {
    args.push('--type', options.type)
  }

  if (pattern) {
    args.push('--glob', `*${pattern}*`)
  }

  args.push(directory)

  return new Promise<string[]>((resolve, reject) => {
    const proc = spawn(rgBin, args, {
      cwd: directory,
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
        reject(new Error(`ripgrep exited with code ${code}: ${stderr}`))
        return
      }

      const results = stdout
        .split('\n')
        .filter((l) => l.trim())
        .slice(0, options?.maxResults ?? 500)

      resolve(results)
    })

    proc.on('error', (err) => {
      reject(new Error(`Failed to spawn ripgrep: ${err.message}`))
    })
  })
}

export interface RipgrepOptions {
  pattern: string
  path: string
  glob?: string
  type?: string
  caseSensitive?: boolean
  maxResults?: number
  contextLines?: number
}

export interface RipgrepMatch {
  filePath: string
  lineNumber: number
  column: number
  lineText: string
  matchText: string
  contextBefore: string[]
  contextAfter: string[]
}

export interface RipgrepResult {
  matches: RipgrepMatch[]
  truncated: boolean
  stats: {
    filesMatched: number
    totalMatches: number
  }
  error?: string
}

const DEFAULT_MAX_RESULTS = 100
const HARD_LIMIT = 500

async function findRgBinary(): Promise<string | null> {
  if (rgPath) return rgPath
  try {
    const mod = await import('@vscode/ripgrep')
    rgPath = mod.rgPath
    return rgPath
  } catch {
    // fallback: check common system paths
    const { access, constants } = await import('fs/promises')
    const candidates =
      process.platform === 'win32'
        ? ['rg.exe', 'C:\\ProgramData\\chocolatey\\bin\\rg.exe']
        : ['rg', '/usr/local/bin/rg', '/opt/homebrew/bin/rg', '/usr/bin/rg']
    for (const candidate of candidates) {
      try {
        await access(candidate, constants.X_OK)
        rgPath = candidate
        return rgPath
      } catch {
        continue
      }
    }
    return null
  }
}

export async function searchRipgrep(options: RipgrepOptions): Promise<RipgrepResult> {
  const rgBin = await findRgBinary()

  if (!rgBin) {
    return {
      matches: [],
      truncated: false,
      stats: { filesMatched: 0, totalMatches: 0 },
      error:
        'ripgrep binary not found. Install rg via your package manager (e.g. brew install ripgrep) or ensure @vscode/ripgrep is installed.',
    }
  }

  const maxResults = Math.min(options.maxResults || DEFAULT_MAX_RESULTS, HARD_LIMIT)
  const contextLines = options.contextLines ?? 2

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

  if (options.caseSensitive) {
    args.push('--case-sensitive')
  } else {
    args.push('--ignore-case')
  }

  if (contextLines > 0) {
    args.push('--context', String(contextLines))
  }

  if (options.glob) {
    args.push('--glob', options.glob)
  }

  if (options.type) {
    args.push('--type', options.type)
  }

  args.push('--', options.pattern, options.path)

  return new Promise<RipgrepResult>((resolve) => {
    const proc = spawn(rgBin, args, {
      cwd: options.path,
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
          matches: [],
          truncated: false,
          stats: { filesMatched: 0, totalMatches: 0 },
          error: `ripgrep exited with code ${code}: ${stderr.trim()}`,
        })
        return
      }

      const matches: RipgrepMatch[] = []
      const filesMatched = new Set<string>()
      let truncated = false
      const pendingContext = new Map<number, { before: string[]; after: string[] }>()

      const lines = stdout.split('\n').filter((l) => l.trim())
      for (const line of lines) {
        if (truncated) break
        try {
          const parsed = JSON.parse(line)

          if (parsed.type === 'context') {
            const data = parsed.data
            const filePath = data.path?.text ?? ''
            const lineNum = data.line_number
            const text = (data.lines?.text ?? '').trimEnd()
            const key = filePath.length > 0 ? filePath.length * 100000 + lineNum : lineNum
            if (!pendingContext.has(key)) {
              pendingContext.set(key, { before: [], after: [] })
            }
            // context lines will be assigned to nearest match below
            const entry = pendingContext.get(key)!
            // we don't know yet if before or after; store for post-processing
            entry.before.push(text)
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

          // collect context lines that belong before this match
          const contextBefore: string[] = []
          const contextAfter: string[] = []

          // context lines with line numbers before this match belong to "before"
          for (const [key, entry] of pendingContext.entries()) {
            const ctxLineNum = key % 100000
            const ctxFilePathLen = Math.floor(key / 100000)
            if (
              ctxFilePathLen === filePath.length &&
              ctxLineNum < lineNum &&
              ctxLineNum >= lineNum - contextLines
            ) {
              contextBefore.push(...entry.before)
              pendingContext.delete(key)
            }
          }

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

      // assign remaining context lines to after the last match
      if (matches.length > 0 && pendingContext.size > 0) {
        const lastMatch = matches[matches.length - 1]
        const remaining: string[] = []
        for (const [key, entry] of pendingContext.entries()) {
          const ctxLineNum = key % 100000
          if (
            ctxLineNum > lastMatch.lineNumber &&
            ctxLineNum <= lastMatch.lineNumber + contextLines
          ) {
            remaining.push(...entry.before)
          }
        }
        lastMatch.contextAfter = remaining
      }

      resolve({
        matches,
        truncated,
        stats: {
          filesMatched: filesMatched.size,
          totalMatches: matches.length,
        },
      })
    })

    proc.on('error', (err) => {
      resolve({
        matches: [],
        truncated: false,
        stats: { filesMatched: 0, totalMatches: 0 },
        error: `Failed to spawn ripgrep: ${err.message}`,
      })
    })
  })
}
