import { FolderOpen, Trash2 } from 'lucide-react'
import type { Space } from '../../types/workspace'
import { useWorkspaceStore } from '../../store/workspaceStore'

interface WorkspaceListProps {
  spaces: Space[]
  activeSpaceId: string | null
  onSelect: (id: string) => void
}

export function WorkspaceList({ spaces, activeSpaceId, onSelect }: WorkspaceListProps) {
  const { removeSpace } = useWorkspaceStore()

  if (spaces.length === 0) {
    return (
      <div className="px-3 py-6 text-center">
        <p className="text-xs" style={{ color: 'var(--textDim)' }}>
          No workspaces yet
        </p>
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-0.5 px-1">
      {spaces.map((space) => (
        <div
          key={space.id}
          onClick={() => onSelect(space.id)}
          className="group flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors w-full cursor-pointer"
          style={{
            background: space.id === activeSpaceId ? 'var(--bgTertiary)' : 'transparent',
          }}
          onMouseEnter={(e) => {
            if (space.id !== activeSpaceId) e.currentTarget.style.background = 'var(--bgTertiary)'
          }}
          onMouseLeave={(e) => {
            if (space.id !== activeSpaceId) e.currentTarget.style.background = 'transparent'
          }}
        >
          <div className="w-2 h-2 rounded-full shrink-0" style={{ background: space.color }} />
          <FolderOpen size={13} style={{ color: 'var(--textDim)' }} className="shrink-0" />
          <span className="text-xs truncate flex-1" style={{ color: 'var(--text)' }}>
            {space.name}
          </span>
          <span className="text-[10px] shrink-0" style={{ color: 'var(--textDim)' }}>
            {space.grid}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation()
              removeSpace(space.id)
            }}
            className="p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-white/10 transition-all"
          >
            <Trash2 size={11} style={{ color: 'var(--error)' }} />
          </button>
        </div>
      ))}
    </div>
  )
}
