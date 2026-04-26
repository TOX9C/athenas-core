import { useRef, useEffect, useState } from 'react'
import { Maximize2, Minimize2, X } from 'lucide-react'
import { nanoid } from 'nanoid'
import { useTerminal } from './useTerminal'
import { getAgentLabel, getAgentColor, getAgentCommand } from '../../utils/agentCommands'
import { useNotificationStore } from '../../store/notificationStore'
import { useTaskStore } from '../../store/taskStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { playDing } from '../../utils/notificationSound'
import type { PaneConfig } from '../../types/workspace'

const READY_PATTERNS = [
  /\$\s*$/,
  /❯\s*$/,
  />\s*$/,
  />>>\s*$/,
  /% \s*$/,
  /\? $/,
  /╰─+>\s*$/,
  /\(y\/n\)\s*$/i,
]

interface TerminalPaneProps {
  pane: PaneConfig
  cwd: string
}

export function TerminalPane({ pane, cwd }: TerminalPaneProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  useTerminal({ paneId: pane.id, cwd, agentCmd: getAgentCommand(pane.agentType, pane.customCmd, { bypass: pane.bypassMode ?? (pane.agentType === 'claude') }) }, containerRef)

  const agentColor = getAgentColor(pane.agentType)
  const agentLabel = pane.label || getAgentLabel(pane.agentType)

  const [isFullScreen, setIsFullScreen] = useState(false)
  const [isFinished, setIsFinished] = useState(false)
  const [isReady, setIsReady] = useState(false)
  const readyTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null)

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
        (t) => t.assignedAgent === pane.agentType && t.status === 'in_progress'
      )
      if (assignedTask) {
        useTaskStore.getState().moveTask(assignedTask.id, 'in_review')
      }
    })

    const checkReady = () => {
      window.athena.pty.getHistory(pane.id).then((history: string) => {
        if (!history) return
        const stripped = history
          .replace(/\x1b\].*?(?:\x07|\x1b\\)/g, '')
          .replace(/\x1b\[[0-9;?]*[A-Za-z]/g, '')
          .replace(/\x1b[()][0-9A-B]/g, '')
          .replace(/\r/g, '')
        const lines = stripped.split('\n').filter((l) => l.trim().length > 0)
        const lastLine = lines[lines.length - 1]?.trimEnd() ?? ''
        if (READY_PATTERNS.some((re) => re.test(lastLine))) {
          setIsReady(true)
        }
      })
    }

    const unsubData = window.athena.pty.onData(pane.id, () => {
      setIsReady(false)
      if (readyTimerRef.current) clearTimeout(readyTimerRef.current)
      readyTimerRef.current = setTimeout(checkReady, 1200)
    })

    return () => {
      unsubExit()
      unsubData()
      if (readyTimerRef.current) clearTimeout(readyTimerRef.current)
    }
  }, [pane.id, pane.agentType, agentLabel])

  const handleClose = () => {
    window.athena.pty.kill(pane.id)
    const activeSpaceId = useWorkspaceStore.getState().activeSpaceId
    if (activeSpaceId) {
      useWorkspaceStore.getState().removePaneFromSpace(activeSpaceId, pane.id)
    }
  }

  const showGreen = isReady || isFinished

  return (
    <>
      {isFullScreen && <div className="fixed inset-0 z-40 bg-black/50 backdrop-blur-sm" />}
      <div
        className={
          isFullScreen
            ? "fixed inset-2 z-50 flex flex-col overflow-hidden shadow-2xl transition-all duration-300"
            : "flex flex-col h-full w-full overflow-hidden transition-all duration-300"
        }
        style={{
          background: 'var(--bg)',
          outline: showGreen ? '1.5px solid #10b981' : 'none',
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
            <div className="w-2 h-2 rounded-full shrink-0 transition-colors duration-300" style={{ background: showGreen ? '#10b981' : agentColor }} />
            <span className="text-[11px] font-medium truncate transition-colors duration-300" style={{ color: showGreen ? '#10b981' : 'var(--textMuted)' }}>
              {agentLabel} {isFinished ? '(Finished)' : isReady ? '(Ready)' : ''}
            </span>
          </div>
          <div className="flex items-center gap-0.5">
            <button
              onClick={() => setIsFullScreen(!isFullScreen)}
              className="p-0.5 rounded hover:bg-white/10 transition-colors"
              title={isFullScreen ? 'Exit Full Screen' : 'Full Screen'}
            >
              {isFullScreen ? <Minimize2 size={11} style={{ color: 'var(--textDim)' }} /> : <Maximize2 size={11} style={{ color: 'var(--textDim)' }} />}
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
