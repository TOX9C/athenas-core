import { useRef, useEffect } from 'react'
import { MoreVertical } from 'lucide-react'
import { nanoid } from 'nanoid'
import { useTerminal } from './useTerminal'
import { getAgentLabel, getAgentColor, getAgentCommand } from '../../utils/agentCommands'
import { useNotificationStore } from '../../store/notificationStore'
import { useTaskStore } from '../../store/taskStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { playDing } from '../../utils/notificationSound'
import type { PaneConfig } from '../../types/workspace'

interface TerminalPaneProps {
  pane: PaneConfig
  cwd: string
}

export function TerminalPane({ pane, cwd }: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  useTerminal({ paneId: pane.id, cwd, agentCmd: getAgentCommand(pane.agentType, pane.customCmd, { bypass: pane.bypassMode ?? (pane.agentType === 'claude') }) }, containerRef)

  const agentColor = getAgentColor(pane.agentType)
  const agentLabel = pane.label || getAgentLabel(pane.agentType)

  useEffect(() => {
    if (pane.agentType === 'shell') return

    const unsub = window.athena.pty.onExit(pane.id, () => {
      const { muted, addNotification } = useNotificationStore.getState()
      const activeSpaceId = useWorkspaceStore.getState().activeSpaceId

      if (!muted) playDing()

      if (activeSpaceId) {
        addNotification({
          id: nanoid(),
          paneId: pane.id,
          paneName: agentLabel,
          agentType: pane.agentType,
          message: 'Agent finished',
          timestamp: Date.now(),
          read: false,
          spaceId: activeSpaceId,
        })
      }

      const tasks = useTaskStore.getState().tasks
      const assignedTask = tasks.find(
        (t) => t.assignedAgent === pane.agentType && t.status === 'in_progress'
      )
      if (assignedTask) {
        useTaskStore.getState().moveTask(assignedTask.id, 'in_review')
      }
    })

    return unsub
  }, [pane.id, pane.agentType, agentLabel])

  return (
    <div
      className="flex flex-col h-full w-full overflow-hidden rounded-lg"
      style={{ background: 'var(--bg)', border: '1px solid var(--border)' }}
    >
      {/* Pane header */}
      <div
        className="flex items-center justify-between px-2.5 shrink-0 border-b"
        style={{ height: 28, borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
      >
        <div className="flex items-center gap-1.5 min-w-0">
          <div className="w-2 h-2 rounded-full shrink-0" style={{ background: agentColor }} />
          <span className="text-[11px] font-medium truncate" style={{ color: 'var(--textMuted)' }}>
            {agentLabel}
          </span>
        </div>
        <button className="p-0.5 rounded hover:bg-white/10 transition-colors shrink-0">
          <MoreVertical size={12} style={{ color: 'var(--textDim)' }} />
        </button>
      </div>

      {/* Terminal container */}
      <div ref={containerRef} className="flex-1 min-h-0 p-1" />
    </div>
  )
}
