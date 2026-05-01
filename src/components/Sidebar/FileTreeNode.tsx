import { useState } from 'react'
import { ChevronRight, ChevronDown } from 'lucide-react'
import type { FileNode } from '../../types/editor'
import { getFileIcon } from '../../utils/fileIcons'

interface FileTreeNodeProps {
  node: FileNode
  depth: number
  onFileSelect: (path: string) => void
}

export function FileTreeNode({ node, depth, onFileSelect }: FileTreeNodeProps) {
  const [expanded, setExpanded] = useState(depth < 1)

  return (
    <div>
      <button
        onClick={() => {
          if (node.isDirectory) {
            setExpanded(!expanded)
          } else {
            onFileSelect(node.path)
          }
        }}
        className="w-full flex items-center gap-1 py-0.5 px-1 text-left hover:bg-white/5 transition-colors rounded-sm"
        style={{ paddingLeft: depth * 12 + 4 }}
      >
        {node.isDirectory ? (
          expanded ? (
            <ChevronDown size={12} style={{ color: 'var(--textDim)' }} />
          ) : (
            <ChevronRight size={12} style={{ color: 'var(--textDim)' }} />
          )
        ) : (
          <span className="w-3 text-center text-[10px]">{getFileIcon(node.name)}</span>
        )}
        <span
          className="text-[11px] truncate"
          style={{ color: node.isDirectory ? 'var(--textMuted)' : 'var(--text)' }}
        >
          {node.name}
        </span>
      </button>
      {node.isDirectory &&
        expanded &&
        node.children?.map((child) => (
          <FileTreeNode
            key={child.path}
            node={child}
            depth={depth + 1}
            onFileSelect={onFileSelect}
          />
        ))}
    </div>
  )
}
