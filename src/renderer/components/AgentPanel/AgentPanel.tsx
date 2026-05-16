import { useRef, useEffect } from 'react'
import { GripVertical, Maximize2, LayoutGrid, Minus, X } from 'lucide-react'

export interface AgentPanelProps {
  paneId: string
  agentName: string
  projectName?: string
  cwd: string
  modelName?: string
  isSelected: boolean
  onSelect: () => void
  onFullscreen: () => void
  onSplit: () => void
  onMinimize: () => void
  onClose: () => void
  children: React.ReactNode
}

export default function AgentPanel({
  agentName,
  projectName,
  cwd,
  modelName,
  isSelected,
  onSelect,
  onFullscreen,
  onSplit,
  onMinimize,
  onClose,
  children,
}: AgentPanelProps) {
  const panelRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    // On mount, read the initial --panel-bg CSS variable and dispatch
    // a theme-change event so the child xterm can pick up the correct background.
    const root = document.documentElement
    const bg = getComputedStyle(root).getPropertyValue('--panel-bg').trim()
    if (bg) {
      const event = new CustomEvent('theme-change', {
        detail: { background: bg },
      })
      document.dispatchEvent(event)
    }
  }, [])

  return (
    <div
      ref={panelRef}
      className={`agent-panel ${isSelected ? 'agent-panel--selected' : ''}`}
      style={{
        background: 'var(--panel-bg)',
        border: isSelected ? '2px solid var(--color-accent)' : 'none',
        boxShadow: isSelected ? '0 0 0 1px var(--color-accent)' : 'none',
      }}
      onClick={(e) => {
        // Only select if clicking the panel itself (not interactive children)
        if (e.currentTarget === e.target) {
          onSelect()
        }
      }}
    >
      {/* Title bar */}
      <div className="agent-panel-titlebar">
        <div className="agent-panel-titlebar-left">
          <GripVertical size={16} style={{ cursor: 'grab', color: 'var(--text-muted)' }} />
          <span className="agent-name">{agentName}</span>
          <span className="project-name">{projectName || ''}</span>
        </div>
        <div className="agent-panel-titlebar-right">
          <button onClick={onFullscreen} title="Fullscreen">
            <Maximize2 size={14} />
          </button>
          <button onClick={onSplit} title="Split">
            <LayoutGrid size={14} />
          </button>
          <button onClick={onMinimize} title="Minimize">
            <Minus size={14} />
          </button>
          <button onClick={onClose} className="agent-panel-close-btn" title="Close">
            <X size={14} />
          </button>
        </div>
      </div>

      {/* Terminal area */}
      <div className="agent-panel-terminal">{children}</div>

      {/* Status bar */}
      <div className="agent-panel-statusbar">
        <span className="agent-panel-statusbar-model">{modelName || 'N/A'}</span>
        <span className="agent-panel-statusbar-cwd">{cwd}</span>
      </div>
    </div>
  )
}
