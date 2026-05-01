import { Plus, X } from 'lucide-react'
import { useWorkspaceStore } from '../../store/workspaceStore'

interface WorkspaceTabsProps {
  onNewTab: () => void
}

export function WorkspaceTabs({ onNewTab }: WorkspaceTabsProps) {
  const { spaces, activeSpaceId, setActiveSpace, removeSpace } = useWorkspaceStore()

  return (
    <div className="flex items-center gap-0.5 no-drag min-w-0 overflow-x-auto">
      {spaces.map((space) => (
        <div
          key={space.id}
          onClick={() => setActiveSpace(space.id)}
          className="group flex items-center gap-1.5 px-3 py-1 rounded-md cursor-pointer transition-all shrink-0 max-w-[160px]"
          style={{
            background: space.id === activeSpaceId ? 'var(--bgTertiary)' : 'transparent',
          }}
          onMouseEnter={(e) => {
            if (space.id !== activeSpaceId) e.currentTarget.style.background = 'var(--bg)'
          }}
          onMouseLeave={(e) => {
            if (space.id !== activeSpaceId) e.currentTarget.style.background = 'transparent'
          }}
        >
          <div className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: space.color }} />
          <span
            className="text-[11px] truncate"
            style={{ color: space.id === activeSpaceId ? 'var(--text)' : 'var(--textMuted)' }}
          >
            {space.name}
          </span>
          <button
            onClick={(e) => {
              e.stopPropagation()
              removeSpace(space.id)
            }}
            className="p-0.5 rounded opacity-0 group-hover:opacity-100 hover:bg-white/10 transition-all shrink-0"
          >
            <X size={10} style={{ color: 'var(--textDim)' }} />
          </button>
        </div>
      ))}
      <button
        onClick={onNewTab}
        className="p-1 rounded hover:bg-white/10 transition-colors shrink-0"
        title="New workspace"
      >
        <Plus size={13} style={{ color: 'var(--textDim)' }} />
      </button>
    </div>
  )
}
