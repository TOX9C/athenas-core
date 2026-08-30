import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { mkdirSync, writeFileSync, rmSync } from 'node:fs'
import { tmpdir } from 'node:os'
import path from 'node:path'
import { searchFiles, searchFilesSchema } from '../src/tools/search-files.js'

// Contract under defense: searchFiles runs ripgrep inside the workspace,
// formats matches as file:line:column with optional context, honors glob
// filters and case-insensitive defaults, reports "No matches" for clean
// inputs, caps max_results at the 500 hard limit, and rejects paths outside
// the workspace (traversal guard).

// search-files resolves WORKSPACE_ROOT at import time from process.cwd()
// (packages/mcp-server when vitest runs). The traversal-guard fixture must
// live inside that root to prove the guard rejects a path *outside* it while
// the fixture itself is addressable.
const fixtureDir = path.join(process.cwd(), 'test-fixture-search-files')

beforeAll(() => {
  rmSync(fixtureDir, { recursive: true, force: true })
  mkdirSync(path.join(fixtureDir, 'sub'), { recursive: true })
  writeFileSync(
    path.join(fixtureDir, 'alpha.ts'),
    ['const one = 1', 'const two = 2', 'const three = 3'].join('\n'),
  )
  writeFileSync(path.join(fixtureDir, 'sub', 'beta.ts'), 'const needle = "beta"\n')
  writeFileSync(path.join(fixtureDir, 'notes.txt'), 'const not_ts = true\n')
})

afterAll(() => {
  rmSync(fixtureDir, { recursive: true, force: true })
})

describe('searchFiles', () => {
  it('finds matches with file:line:column formatting', async () => {
    const result = await searchFiles(undefined, {
      pattern: 'const two',
      path: fixtureDir,
      context_lines: 0,
    })
    const text = result.content[0].text
    expect(text).toContain('Found 1 matches')
    expect(text).toMatch(/alpha\.ts:2:\d+: const two = 2/)
  })
  it('includes context lines before a match', async () => {
    const result = await searchFiles(undefined, {
      pattern: 'const two',
      path: fixtureDir,
      context_lines: 1,
    })
    const text = result.content[0].text
    expect(text).toContain('1: const one = 1')
  })
  it('honors glob filters', async () => {
    const result = await searchFiles(undefined, {
      pattern: 'needle',
      path: fixtureDir,
      glob: '*.txt',
      context_lines: 0,
    })
    expect(result.content[0].text).toContain('No matches found')
  })

  it('is case-insensitive by default', async () => {
    const result = await searchFiles(undefined, {
      pattern: 'NEEDLE',
      path: fixtureDir,
      context_lines: 0,
    })
    expect(result.content[0].text).toContain('Found 1 matches')
  })

  it('reports no matches for a clean tree', async () => {
    const result = await searchFiles(undefined, {
      pattern: 'zzz_no_such_token_zzz',
      path: fixtureDir,
      context_lines: 0,
    })
    expect(result.content[0].text).toContain('No matches found for pattern')
  })
  it('rejects max_results above the 500 hard limit via schema', () => {
    expect(() => searchFilesSchema.parse({ pattern: 'x', path: '.', max_results: 5001 })).toThrow()
    expect(searchFilesSchema.parse({ pattern: 'x', path: '.', max_results: 500 }).max_results).toBe(
      500,
    )
  })

  it('rejects out-of-range context_lines via schema', () => {
    expect(() => searchFilesSchema.parse({ pattern: 'x', path: '.', context_lines: 11 })).toThrow()
    expect(() => searchFilesSchema.parse({ pattern: 'x', path: '.', context_lines: -1 })).toThrow()
    expect(
      searchFilesSchema.parse({ pattern: 'x', path: '.', context_lines: 0 }).context_lines,
    ).toBe(0)
  })
})

describe('searchFiles traversal guard', () => {
  it('rejects a path outside the workspace instead of searching it', async () => {
    const outside = path.join(tmpdir(), 'athena-search-outside-probe')
    rmSync(outside, { recursive: true, force: true })
    mkdirSync(outside, { recursive: true })
    writeFileSync(path.join(outside, 'secret.txt'), 'const needle = "outside"\n')

    // assertInsideWorkspace throws synchronously inside the executor before
    // spawn; searchFiles must surface that rejection, never search results.
    await expect(
      searchFiles(undefined, { pattern: 'needle', path: outside, context_lines: 0 }),
    ).rejects.toThrow(/outside the workspace/)

    rmSync(outside, { recursive: true, force: true })
  })
})
