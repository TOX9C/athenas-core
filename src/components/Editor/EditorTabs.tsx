import { useState, useEffect, useRef, useCallback } from 'react'
import { X, Circle } from 'lucide-react'
import { useEditorStore } from '../../store/editorStore'

interface EditorTabsProps {
  onClose: (path: string) => void
}

export function EditorTabs({ onClose }: EditorTabsProps) {
  const { openFiles, activeFilePath, setActiveFile } = useEditorStore()

  if (openFiles.length === 0) return null

  return (
    <div
      className="flex items-center gap-0.5 overflow-x-auto shrink-0 border-b px-1"
      style={{ height: 32, borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
    >
      {openFiles.map((file) => {
        const filename = file.path.split('/').pop() ?? file.path
        const isActive = file.path === activeFilePath
        return (
          <div
            key={file.path}
            onClick={() => setActiveFile(file.path)}
            className="group flex items-center gap-1.5 px-2.5 py-1 rounded-t-md cursor-pointer transition-colors shrink-0"
            style={{
              background: isActive ? 'var(--bg)' : 'transparent',
              borderBottom: isActive ? '2px solid var(--accent)' : '2px solid transparent',
            }}
          >
            {file.isDirty && (
              <Circle size={6} fill="var(--warning)" style={{ color: 'var(--warning)' }} className="shrink-0" />
            )}
            <span
              className="text-[11px] truncate max-w-[120px]"
              style={{ color: isActive ? 'var(--text)' : 'var(--textMuted)' }}
            >
              {filename}
            </span>
            <button
              onClick={(e) => { e.stopPropagation(); onClose(file.path) }}
              className="p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-white/10 transition-all shrink-0"
            >
              <X size={10} style={{ color: 'var(--textDim)' }} />
            </button>
          </div>
        )
      })}
    </div>
  )
}
