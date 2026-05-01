import { Zap, Plus, FolderOpen, Bot, ChevronLeft, Layers, Puzzle, Eye } from 'lucide-react'
import { useUIStore } from '../../store/uiStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { useAgentOutputStore } from '../../store/agentOutputStore'
import { WorkspaceList } from './WorkspaceList'
import { FileTree } from './FileTree'
import { AgentStatusList } from '../Plugin/AgentStatusList'
import { PluginDashboard } from '../Plugin/PluginDashboard'

interface SidebarProps {
  onNewSpace: () => void
  onFileSelect?: (path: string) => void
}

export function Sidebar({ onNewSpace, onFileSelect }: SidebarProps) {
  const { sidebarWidth, toggleSidebar, activeSidebarSection, setSidebarSection } = useUIStore()
  const { spaces, activeSpaceId, setActiveSpace } = useWorkspaceStore()
  const activeSpace = spaces.find((s) => s.id === activeSpaceId)

  const getSectionTitle = () => {
    switch (activeSidebarSection) {
      case 'spaces':
        return 'Spaces'
      case 'files':
        return 'Files'
      case 'agents':
        return 'Agents'
      case 'plugins':
        return 'Plugins'
    }
  }

  const getSectionIcon = () => {
    switch (activeSidebarSection) {
      case 'spaces':
        return <Layers size={14} style={{ color: 'var(--accent)' }} />
      case 'files':
        return <FolderOpen size={14} style={{ color: 'var(--accent)' }} />
      case 'agents':
        return <Bot size={14} style={{ color: 'var(--accent)' }} />
      case 'plugins':
        return <Puzzle size={14} style={{ color: 'var(--accent)' }} />
    }
  }

  return (
    <div
      className="shrink-0 flex flex-col border-r h-full"
      style={{
        width: sidebarWidth,
        minWidth: 180,
        maxWidth: 400,
        borderColor: 'var(--border)',
        background: 'var(--bgSecondary)',
      }}
    >
      <div
        className="flex items-center justify-between px-3 py-2 border-b"
        style={{ borderColor: 'var(--border)' }}
      >
        <div className="flex items-center gap-2">
          {getSectionIcon()}
          <span
            className="text-xs font-semibold tracking-wider uppercase"
            style={{ color: 'var(--textMuted)' }}
          >
            {getSectionTitle()}
          </span>
        </div>
        <div className="flex items-center gap-1">
          {activeSidebarSection === 'spaces' && (
            <button
              onClick={onNewSpace}
              className="p-1 rounded hover:bg-white/10 transition-colors"
              title="New workspace"
            >
              <Plus size={14} style={{ color: 'var(--textMuted)' }} />
            </button>
          )}
          <button
            onClick={toggleSidebar}
            className="p-1 rounded hover:bg-white/10 transition-colors"
            title="Collapse sidebar"
          >
            <ChevronLeft size={14} style={{ color: 'var(--textMuted)' }} />
          </button>
        </div>
      </div>

      <div className="flex-1 overflow-y-auto py-1">
        {activeSidebarSection === 'spaces' && (
          <WorkspaceList spaces={spaces} activeSpaceId={activeSpaceId} onSelect={setActiveSpace} />
        )}
        {activeSidebarSection === 'files' && (
          <div className="p-4 text-center text-xs" style={{ color: 'var(--textDim)' }}>
            File explorer coming soon
          </div>
        )}
        {activeSidebarSection === 'agents' && (
          <>
            <AgentStatusList />
            <div className="px-3 pt-2 pb-1 border-t" style={{ borderColor: 'var(--border)' }}>
              <button
                onClick={() => useAgentOutputStore.getState().setInspectorOpen(true)}
                className="w-full flex items-center justify-center gap-1.5 px-2 py-1.5 rounded text-[10px] font-medium transition-colors hover:bg-white/5"
                style={{ color: 'var(--textMuted)' }}
              >
                <Eye size={11} />
                Open Output Inspector
              </button>
            </div>
          </>
        )}
        {activeSidebarSection === 'plugins' && <PluginDashboard />}
      </div>

      {activeSidebarSection === 'spaces' && (
        <div className="border-t p-2" style={{ borderColor: 'var(--border)' }}>
          <button
            onClick={onNewSpace}
            className="w-full flex items-center gap-2 px-3 py-2 rounded-md text-xs font-medium transition-colors hover:bg-white/5"
            style={{ color: 'var(--textMuted)' }}
          >
            <Plus size={14} />
            New Workspace
          </button>
        </div>
      )}

      {/* Sidebar Section Tabs at bottom */}
      <div
        className="flex items-center justify-around p-1 border-t shrink-0"
        style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
      >
        <button
          onClick={() => setSidebarSection('spaces')}
          className={`p-1.5 rounded-md transition-colors ${activeSidebarSection === 'spaces' ? 'bg-white/10' : 'hover:bg-white/5'}`}
          title="Spaces"
        >
          <Layers
            size={14}
            style={{ color: activeSidebarSection === 'spaces' ? 'var(--text)' : 'var(--textDim)' }}
          />
        </button>
        <button
          onClick={() => setSidebarSection('files')}
          className={`p-1.5 rounded-md transition-colors ${activeSidebarSection === 'files' ? 'bg-white/10' : 'hover:bg-white/5'}`}
          title="Files"
        >
          <FolderOpen
            size={14}
            style={{ color: activeSidebarSection === 'files' ? 'var(--text)' : 'var(--textDim)' }}
          />
        </button>
        <button
          onClick={() => setSidebarSection('agents')}
          className={`p-1.5 rounded-md transition-colors ${activeSidebarSection === 'agents' ? 'bg-white/10' : 'hover:bg-white/5'}`}
          title="Agents"
        >
          <Bot
            size={14}
            style={{ color: activeSidebarSection === 'agents' ? 'var(--text)' : 'var(--textDim)' }}
          />
        </button>
        <button
          onClick={() => setSidebarSection('plugins')}
          className={`p-1.5 rounded-md transition-colors ${activeSidebarSection === 'plugins' ? 'bg-white/10' : 'hover:bg-white/5'}`}
          title="Plugins"
        >
          <Puzzle
            size={14}
            style={{ color: activeSidebarSection === 'plugins' ? 'var(--text)' : 'var(--textDim)' }}
          />
        </button>
      </div>
    </div>
  )
}
