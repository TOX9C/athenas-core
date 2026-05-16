import { useCallback, useState } from 'react'
import type { ReactNode } from 'react'
import type { PaneConfig } from '../../types/workspace'
import AgentPanel from '../AgentPanel/AgentPanel'

export interface AgentGridProps {
  panes: PaneConfig[]
  cwd: string
  onSelectPane: (paneId: string) => void
  onFullscreenPane: (paneId: string) => void
  onSplitPane: (paneId: string) => void
  onMinimizePane: (paneId: string) => void
  onClosePane: (paneId: string) => void
  children: (pane: PaneConfig) => ReactNode
}

export default function AgentGrid({
  panes,
  cwd,
  onSelectPane,
  onFullscreenPane,
  onSplitPane,
  onMinimizePane,
  onClosePane,
  children,
}: AgentGridProps) {
  const [selectedPaneId, setSelectedPaneId] = useState<string | null>(null)

  const handleSelect = useCallback(
    (paneId: string) => {
      setSelectedPaneId(paneId)
      onSelectPane(paneId)
    },
    [onSelectPane]
  )

  return (
    <div
      className="agent-grid"
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(3, 1fr)',
        gap: '1rem',
      }}
    >
      {panes.map((pane) => (
        <AgentPanel
          key={pane.id}
          paneId={pane.id}
          agentName={pane.agentType}
          projectName={pane.projectName ?? ''}
          cwd={cwd}
          modelName={pane.modelName ?? ''}
          isSelected={selectedPaneId === pane.id}
          onSelect={() => handleSelect(pane.id)}
          onFullscreen={() => onFullscreenPane(pane.id)}
          onSplit={() => onSplitPane(pane.id)}
          onMinimize={() => onMinimizePane(pane.id)}
          onClose={() => onClosePane(pane.id)}
        >
          {children(pane)}
        </AgentPanel>
      ))}
    </div>
  )
}
