import { useState, useEffect } from 'react'
import { FolderOpen, RefreshCw } from 'lucide-react'
import type { FileNode } from '../../types/editor'
import { FileTreeNode } from './FileTreeNode'
import { useWorkspaceStore } from '../../store/workspaceStore'

interface FileExplorerProps {
  onFileSelect: (path: string) => void
}

export function FileExplorer({ onFileSelect }: FileExplorerProps) {
  const [tree, setTree] = useState<FileNode[]>([])
  const [loading, setLoading] = useState(false)
  const activeSpaceId = useWorkspaceStore((s) => s.activeSpaceId)
  const spaces = useWorkspaceStore((s) => s.spaces)
  const activeSpace = spaces.find((s) => s.id === activeSpaceId)

  const loadTree = async () => {
    if (!activeSpace) return
    setLoading(true)
    try {
      const nodes = await window.athena.fs.readTree(activeSpace.dir)
      if (Array.isArray(nodes)) setTree(nodes)
    } catch {
      // silently fail
    }
    setLoading(false)
  }

  useEffect(() => {
    loadTree()
  }, [activeSpace?.dir])

  if (!activeSpace) {
    return (
      <div className="px-3 py-4 text-[11px]" style={{ color: 'var(--textDim)' }}>
        Select a workspace to explore files
      </div>
    )
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-1.5 border-b" style={{ borderColor: 'var(--border)' }}>
        <div className="flex items-center gap-1.5">
          <FolderOpen size={12} style={{ color: 'var(--textDim)' }} />
          <span className="text-[11px] font-medium" style={{ color: 'var(--textMuted)' }}>
            Files
          </span>
        </div>
        <button
          onClick={loadTree}
          className="p-0.5 rounded hover:bg-white/10 transition-colors"
          disabled={loading}
        >
          <RefreshCw size={11} style={{ color: 'var(--textDim)' }} className={loading ? 'animate-spin' : ''} />
        </button>
      </div>
      <div className="flex-1 overflow-y-auto py-1">
        {tree.map((node) => (
          <FileTreeNode key={node.path} node={node} depth={0} onFileSelect={onFileSelect} />
        ))}
      </div>
    </div>
  )
}
