# File Explorer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement a recursive file tree in the left sidebar that displays the exact directory structure of the user's active workspace utilizing `window.athena.fs.readTree`.

**Architecture:** We will create a `FileTree` and a linked recursive component `FileNodeItem` within `src/components/Sidebar/FileTree.tsx`. The main `FileTree` will hydrate a `useState` whenever the active `dir` property changes by invoking the existing IPC backend. The `Sidebar` component will pass `activeSpace.dir` to this new feature.

**Tech Stack:** React, Tailwind CSS, lucide-react

---

### Task 1: Create the FileTree Components

**Files:**

- Create: `src/components/Sidebar/FileTree.tsx`

- [ ] **Step 1: Write the generic types and FileNodeItem component logic**

Create the new file `src/components/Sidebar/FileTree.tsx`. Import lucide-react icons and react hooks, then define the base types and recursive component block.

```tsx
import React, { useState, useEffect } from 'react'
import { ChevronRight, ChevronDown, Folder, FolderOpen, File } from 'lucide-react'

// Match the electron/fileSystem.ts return signature
export interface FileNode {
  name: string
  path: string
  isDirectory: boolean
  children?: FileNode[]
}

const FileNodeItem = ({ node, level = 0 }: { node: FileNode; level?: number }) => {
  const [isOpen, setIsOpen] = useState(false)

  const toggleOpen = () => {
    if (node.isDirectory) setIsOpen(!isOpen)
  }

  const paddingLeft = `${level * 1rem}rem`

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
            <FileNodeItem key={child.path} node={child} level={level + 1} />
          ))}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Write the generic root FileTree component**

Append this code to `src/components/Sidebar/FileTree.tsx` right below `FileNodeItem`.

```tsx
export const FileTree = ({ dir }: { dir?: string }) => {
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
    <div className="py-2 overflow-y-auto">
      {nodes.map((node) => (
        <FileNodeItem key={node.path} node={node} />
      ))}
    </div>
  )
}
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Sidebar/FileTree.tsx
git commit -m "feat(sidebar): implement recursive recursive file tree view components"
```

---

### Task 2: Mount FileTree in Sidebar

**Files:**

- Modify: `src/components/Sidebar/Sidebar.tsx`

- [ ] **Step 1: Import the new component**

In `src/components/Sidebar/Sidebar.tsx`, add the import near the top:

```tsx
import { FileTree } from './FileTree'
```

- [ ] **Step 2: Read activeSpace and replace placeholder UI**

In `Sidebar.tsx`, the active space is already fetched via `const { spaces, activeSpaceId } = useWorkspaceStore()`.
Right below that, declare:

```tsx
const activeSpace = spaces.find((s) => s.id === activeSpaceId)
```

Find the explicit string `File explorer coming soon`. Replace the `div` containing it with the `<FileTree />`.

Change:

```tsx
<div className="flex-1 overflow-y-auto">
  {/* File explorer will go here */}
  <div className="p-4 text-sm text-neutral-500 text-center">File explorer coming soon</div>
</div>
```

To:

```tsx
<div className="flex-1 overflow-y-auto">
  <FileTree dir={activeSpace?.dir} />
</div>
```

- [ ] **Step 3: Commit**

```bash
git add src/components/Sidebar/Sidebar.tsx
git commit -m "feat(sidebar): mount file tree driven by active workspace directory"
```
