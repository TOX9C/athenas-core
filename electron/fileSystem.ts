import { readdir, stat, readFile, writeFile } from 'fs/promises'
import { join, extname } from 'path'

const SKIP_DIRS = new Set(['node_modules', '.git', '.next', 'dist', 'build', '.ade', '.DS_Store'])
const MAX_DEPTH = 6

export interface FileNode {
  name: string
  path: string
  isDirectory: boolean
  children?: FileNode[]
}

export async function readTree(dir: string, depth = 0): Promise<FileNode[]> {
  if (depth >= MAX_DEPTH) return []

  try {
    const entries = await readdir(dir, { withFileTypes: true })
    const nodes: FileNode[] = []

    const sorted = entries
      .filter((e) => !SKIP_DIRS.has(e.name) && !e.name.startsWith('.'))
      .sort((a, b) => {
        if (a.isDirectory() && !b.isDirectory()) return -1
        if (!a.isDirectory() && b.isDirectory()) return 1
        return a.name.localeCompare(b.name)
      })

    for (const entry of sorted) {
      const fullPath = join(dir, entry.name)
      if (entry.isDirectory()) {
        const children = await readTree(fullPath, depth + 1)
        nodes.push({ name: entry.name, path: fullPath, isDirectory: true, children })
      } else {
        nodes.push({ name: entry.name, path: fullPath, isDirectory: false })
      }
    }

    return nodes
  } catch (err: any) {
    throw new Error(`Failed to read directory: ${err.message}`)
  }
}

export async function readFileContent(path: string): Promise<string> {
  try {
    return await readFile(path, 'utf-8')
  } catch (err: any) {
    throw new Error(`Failed to read file: ${err.message}`)
  }
}

export async function writeFileContent(path: string, content: string): Promise<void> {
  try {
    await writeFile(path, content, 'utf-8')
  } catch (err: any) {
    throw new Error(`Failed to write file: ${err.message}`)
  }
}

export async function getDirectories(dirPath: string): Promise<string[]> {
  try {
    const entries = await readdir(dirPath, { withFileTypes: true })
    return entries.filter((entry) => entry.isDirectory()).map((entry) => entry.name)
  } catch (err: unknown) {
    throw new Error(
      `Failed to read directories: ${err instanceof Error ? err.message : String(err)}`,
    )
  }
}
