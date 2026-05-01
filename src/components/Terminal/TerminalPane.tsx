import { useRef, useEffect, useState, useMemo, useCallback } from 'react'
import { Maximize2, Minimize2, X } from 'lucide-react'
import { nanoid } from 'nanoid'
import { useTerminal } from './useTerminal'
import { getAgentLabel, getAgentColor, getAgentCommand } from '../../utils/agentCommands'
import { useNotificationStore, isEnhanced } from '../../store/notificationStore'
import { useAgentStatusStore } from '../../store/agentStatusStore'
import { useTaskStore } from '../../store/taskStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { playDing } from '../../utils/notificationSound'
import { useShallow } from 'zustand/shallow'
import type { PaneConfig } from '../../types/workspace'

interface TerminalPaneProps {
  pane: PaneConfig
  cwd: string
}

function getPaneBorderColor(args: {
  agentStatus: string | undefined
  hasUnreadInput: boolean
  hasUnreadError: boolean
  isReady: boolean
  isFinished: boolean
}): string | null {
  const { agentStatus, hasUnreadInput, hasUnreadError, isReady, isFinished } = args
  if (hasUnreadInput) return '#f97316'
  if (hasUnreadError) return '#ef4444'
  if (agentStatus === 'error') return '#ef4444'
  if (agentStatus === 'waiting_for_input') return '#f97316'
  if (agentStatus === 'thinking' || agentStatus === 'working') return '#3b82f6'
  if (isReady || isFinished || agentStatus === 'completed') return '#10b981'
  if (agentStatus === 'idle' || agentStatus === 'cancelled' || agentStatus === 'disconnected')
    return null
  return null
}

export function TerminalPane({ pane, cwd }: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  useTerminal(
    {
      paneId: pane.id,
      cwd,
      agentCmd: getAgentCommand(pane.agentType, pane.customCmd, {
        bypass: pane.bypassMode ?? pane.agentType === 'claude',
      }),
    },
    containerRef,
  )

  const agentColor = getAgentColor(pane.agentType)
  const agentLabel = pane.label || getAgentLabel(pane.agentType)

  const [isFullScreen, setIsFullScreen] = useState(false)
  const [isFinished, setIsFinished] = useState(false)
  const [isReady, setIsReady] = useState(false)

  const agentStatus = useAgentStatusStore((s) => s.statuses[pane.id]?.status)
  const agentStatusObj = useAgentStatusStore((s) => s.statuses[pane.id])

  // Use stable selectors to avoid infinite re-renders when the notification
  // store updates from other panes — only re-render when this pane's
  // unread input/error state actually changes
  const hasUnreadInput = useNotificationStore((s) =>
    s.notifications.some(
      (n) => isEnhanced(n) && n.paneId === pane.id && !n.read && n.type === 'needs_input',
    ),
  )
  const hasUnreadError = useNotificationStore((s) =>
    s.notifications.some(
      (n) =>
        isEnhanced(n) &&
        n.paneId === pane.id &&
        !n.read &&
        (n.type === 'error' || n.type === 'warning'),
    ),
  )

  const borderColor = useMemo(
    () =>
      getPaneBorderColor({
        agentStatus,
        hasUnreadInput,
        hasUnreadError,
        isReady,
        isFinished,
      }),
    [agentStatus, hasUnreadInput, hasUnreadError, isReady, isFinished],
  )

  const statusLabel = useMemo(() => {
    if (isFinished) return '(Finished)'
    if (isReady) return '(Ready)'
    if (agentStatus === 'waiting_for_input') return '(Waiting)'
    if (agentStatus === 'thinking') return '(Thinking)'
    if (agentStatus === 'working') return '(Working)'
    if (agentStatus === 'error') return '(Error)'
    return ''
  }, [isFinished, isReady, agentStatus])

  const dotColor = borderColor ?? (isReady || isFinished ? '#10b981' : agentColor)

  useEffect(() => {
    if (pane.agentType === 'shell') return

    const unsubExit = window.athena.pty.onExit(pane.id, () => {
      setIsFinished(true)
      setIsReady(false)
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
        (t) => t.assignedAgent === pane.agentType && t.status === 'in_progress',
      )
      if (assignedTask) {
        useTaskStore.getState().moveTask(assignedTask.id, 'in_review')
      }
    })

    const unsubReady = (window.athena.pty.onReady as any)(pane.id, () => {
      setIsReady(true)
    })

    return () => {
      unsubExit()
      unsubReady()
    }
  }, [pane.id, pane.agentType, agentLabel])

  const handleClose = () => {
    window.athena.pty.kill(pane.id)
    const activeSpaceId = useWorkspaceStore.getState().activeSpaceId
    if (activeSpaceId) {
      useWorkspaceStore.getState().removePaneFromSpace(activeSpaceId, pane.id)
    }
  }

  return (
    <>
      {isFullScreen && <div className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm" />}
      <div
        className={
          isFullScreen
            ? 'fixed inset-2 z-50 flex flex-col overflow-hidden shadow-2xl transition-all duration-300'
            : 'flex flex-col h-full w-full overflow-hidden transition-all duration-300'
        }
        style={{
          background: 'var(--bg)',
          outline: borderColor ? `1.5px solid ${borderColor}` : 'none',
        }}
      >
        {/* Pane header */}
        <div
          className="flex items-center justify-between px-2.5 shrink-0 transition-colors duration-300"
          style={{
            height: 28,
            background: 'var(--bgSecondary)',
          }}
        >
          <div className="flex items-center gap-1.5 min-w-0">
            <div
              className="w-2 h-2 rounded-full shrink-0 transition-colors duration-300"
              style={{ background: dotColor }}
            />
            <span
              className="text-[11px] font-medium truncate transition-colors duration-300"
              style={{ color: borderColor ?? 'var(--textMuted)' }}
            >
              {agentLabel} {statusLabel}
            </span>
            {agentStatusObj?.progress && (
              <span className="text-[9px]" style={{ color: 'var(--textDim)' }}>
                {agentStatusObj.progress.current}/{agentStatusObj.progress.total}
              </span>
            )}
          </div>
          <div className="flex items-center gap-0.5">
            <button
              onClick={() => setIsFullScreen(!isFullScreen)}
              className="p-0.5 rounded hover:bg-white/10 transition-colors"
              title={isFullScreen ? 'Exit Full Screen' : 'Full Screen'}
            >
              {isFullScreen ? (
                <Minimize2 size={11} style={{ color: 'var(--textDim)' }} />
              ) : (
                <Maximize2 size={11} style={{ color: 'var(--textDim)' }} />
              )}
            </button>
            <button
              onClick={handleClose}
              className="p-0.5 rounded hover:bg-red-500/20 transition-colors"
              title="Close"
            >
              <X size={11} style={{ color: 'var(--textDim)' }} />
            </button>
          </div>
        </div>

        {/* Terminal container */}
        <div ref={containerRef} className="flex-1 min-h-0 p-1" />
      </div>
    </>
  )
}
