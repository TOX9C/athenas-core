import React, { useState, useEffect } from 'react'
import { ChevronRight, ChevronDown, Folder, FolderOpen, File } from 'lucide-react'

// Match the electron/fileSystem.ts return signature
export interface FileNode {
  name: string
  path: string
  isDirectory: boolean
  children?: FileNode[]
}

const FileNodeItem = ({ node, level = 0, onFileSelect }: { node: FileNode; level?: number; onFileSelect?: (path: string) => void }) => {
  const [isOpen, setIsOpen] = useState(false)

  const toggleOpen = () => {
    if (node.isDirectory) {
      setIsOpen(!isOpen)
    } else if (onFileSelect) {
      onFileSelect(node.path)
    }
  }

  const paddingLeft = `${level}rem`

  return (
    <div>
      <div
        className="flex items-center py-1 px-2 hover:bg-neutral-800 cursor-pointer text-sm text-neutral-300 transition-colors"
        style={{ paddingLeft }}
        onClick={toggleOpen}
      >
        <div className="mr-1.5 flex-shrink-0 w-4 h-4 flex items-center justify-center">
          {node.isDirectory ? (
            isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />
          ) : null}
        </div>

        <div className="mr-2 text-neutral-400">
          {node.isDirectory ? (
            isOpen ? <FolderOpen size={14} /> : <Folder size={14} />
          ) : (
            <File size={14} />
          )}
        </div>
        <span className="truncate Select-none">{node.name}</span>
      </div>

      {isOpen && node.children && (
        <div>
          {node.children.map((child) => (
            <FileNodeItem key={child.path} node={child} level={level + 1} onFileSelect={onFileSelect} />
          ))}
        </div>
      )}
    </div>
  )
}

export const FileTree = ({ dir, onFileSelect }: { dir?: string; onFileSelect?: (path: string) => void }) => {
  const [nodes, setNodes] = useState<FileNode[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!dir) {
      setNodes([])
      return
    }

    let mounted = true

    const fetchTree = async () => {
      setLoading(true)
      setError(null)
      try {
        // @ts-ignore - athena is exposed via Preload script
        const result = await window.athena.fs.readTree(dir)
        if (mounted) setNodes(result)
      } catch (err: any) {
        if (mounted) setError(err.message)
      } finally {
        if (mounted) setLoading(false)
      }
    }

    fetchTree()

    return () => {
      mounted = false
    }
  }, [dir])

  if (!dir) {
    return <div className="p-4 text-sm text-neutral-500 text-center">No workspace selected</div>
  }

  if (loading) {
    return <div className="p-4 text-sm text-neutral-500 text-center">Loading files...</div>
  }

  if (error) {
    return <div className="p-4 text-sm text-red-500 text-center break-words">{error}</div>
  }

  return (
    <div className="py-2">
      {nodes.map((node) => (
        <FileNodeItem key={node.path} node={node} onFileSelect={onFileSelect} />
      ))}
    </div>
  )
}
