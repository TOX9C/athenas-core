import { useState, useEffect, useRef, useCallback } from 'react'
import { Search } from 'lucide-react'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { useEditorStore } from '../../store/editorStore'
import { fuzzySearch } from '../../utils/fuzzySearch'

interface QuickOpenProps {
  onClose: () => void
}

async function collectFiles(dir: string): Promise<string[]> {
  const tree = await window.athena.fs.readTree(dir)
  const paths: string[] = []
  function walk(nodes: any[]) {
    for (const n of nodes) {
      if (n.isDirectory && n.children) {
        walk(n.children)
      } else if (!n.isDirectory) {
        paths.push(n.path)
      }
    }
  }
  if (Array.isArray(tree)) walk(tree)
  return paths
}

export function QuickOpen({ onClose }: QuickOpenProps) {
  const [query, setQuery] = useState('')
  const [allFiles, setAllFiles] = useState<string[]>([])
  const [selectedIdx, setSelectedIdx] = useState(0)
  const inputRef = useRef<HTMLInputElement>(null)
  const activeSpace = useWorkspaceStore((s) => {
    const sp = s.spaces.find((sp) => sp.id === s.activeSpaceId)
    return sp
  })
  const { openFile } = useEditorStore()

  useEffect(() => {
    inputRef.current?.focus()
    if (activeSpace) {
      collectFiles(activeSpace.dir).then(setAllFiles)
    }
  }, [activeSpace?.dir])

  const filtered = fuzzySearch(query, allFiles).slice(0, 20)

  useEffect(() => {
    setSelectedIdx(0)
  }, [query])

  const handleSelect = useCallback(
    async (path: string) => {
      try {
        const content = await window.athena.fs.readFile(path)
        const lang = path.split('.').pop() ?? 'plaintext'
        const langMap: Record<string, string> = {
          ts: 'typescript',
          tsx: 'typescript',
          js: 'javascript',
          jsx: 'javascript',
          json: 'json',
          md: 'markdown',
          css: 'css',
          html: 'html',
          py: 'python',
        }
        openFile({
          path,
          content: typeof content === 'string' ? content : '',
          language: langMap[lang] ?? 'plaintext',
          isDirty: false,
          cursorPosition: { line: 1, column: 1 },
        })
        onClose()
      } catch {
        // file read failed
      }
    },
    [openFile, onClose],
  )

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setSelectedIdx((i) => Math.min(i + 1, filtered.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setSelectedIdx((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter' && filtered[selectedIdx]) {
      handleSelect(filtered[selectedIdx])
    } else if (e.key === 'Escape') {
      onClose()
    }
  }

  const stripDir = activeSpace?.dir ?? ''

  return (
    <div
      className="fixed inset-0 z-50 flex justify-center pt-[15vh]"
      style={{ background: 'rgba(0,0,0,0.4)' }}
      onClick={(e) => {
        if (e.target === e.currentTarget) onClose()
      }}
    >
      <div
        className="rounded-xl shadow-2xl overflow-hidden flex flex-col"
        style={{
          width: 520,
          maxHeight: 400,
          background: 'var(--bgSecondary)',
          border: '1px solid var(--border)',
        }}
      >
        <div
          className="flex items-center gap-2 px-3 py-2 border-b"
          style={{ borderColor: 'var(--border)' }}
        >
          <Search size={14} style={{ color: 'var(--textDim)' }} />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search files..."
            className="flex-1 bg-transparent text-sm outline-none"
            style={{ color: 'var(--text)' }}
          />
        </div>
        <div className="flex-1 overflow-y-auto">
          {filtered.length === 0 ? (
            <div className="px-3 py-6 text-center text-xs" style={{ color: 'var(--textDim)' }}>
              {query ? 'No files found' : 'Type to search'}
            </div>
          ) : (
            filtered.map((path, idx) => (
              <button
                key={path}
                onClick={() => handleSelect(path)}
                className="w-full flex items-center px-3 py-1.5 text-left transition-colors"
                style={{
                  background: idx === selectedIdx ? 'var(--bgTertiary)' : 'transparent',
                }}
                onMouseEnter={() => setSelectedIdx(idx)}
              >
                <span className="text-[11px] font-mono truncate" style={{ color: 'var(--text)' }}>
                  {path.replace(stripDir + '/', '')}
                </span>
              </button>
            ))
          )}
        </div>
      </div>
    </div>
  )
}
