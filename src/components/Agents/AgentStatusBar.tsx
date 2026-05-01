import { Bot, Circle, Loader, AlertTriangle, CheckCircle, Pause, XCircle } from 'lucide-react'
import { useAgentStatusStore } from '../../store/agentStatusStore'
import { getAgentColor } from '../../utils/agentCommands'
import { useAgentOutputStore } from '../../store/agentOutputStore'

const STATUS_ICON: Record<string, { icon: typeof Circle; color: string }> = {
  idle: { icon: Circle, color: 'var(--textDim)' },
  thinking: { icon: Loader, color: 'var(--accent)' },
  working: { icon: Loader, color: 'var(--accent)' },
  waiting_for_input: { icon: Pause, color: 'var(--warning)' },
  completed: { icon: CheckCircle, color: 'var(--success)' },
  error: { icon: AlertTriangle, color: 'var(--error)' },
  cancelled: { icon: XCircle, color: 'var(--textDim)' },
  disconnected: { icon: Circle, color: 'var(--textDim)' },
}

function timeAgo(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 5) return 'now'
  if (diff < 60) return `${diff}s`
  if (diff < 3600) return `${Math.floor(diff / 60)}m`
  return `${Math.floor(diff / 3600)}h`
}

export function AgentStatusBar({ paneId }: { paneId: string }) {
  const status = useAgentStatusStore((s) => s.statuses[paneId])
  const agents = useAgentOutputStore((s) => s.agents)
  const outputInfo = agents.find((a) => a.paneId === paneId)

  const cfg = STATUS_ICON[status?.status ?? 'idle'] ?? STATUS_ICON.idle
  const Icon = cfg.icon
  const isSpinning = ['thinking', 'working'].includes(status?.status ?? '')

  return (
    <div
      className="flex items-center gap-1.5 px-2 py-0.5 border-t shrink-0"
      style={{ borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
    >
      <Bot size={10} style={{ color: getAgentColor((status as any)?.agentType ?? 'shell') }} />
      <Icon size={9} style={{ color: cfg.color }} className={isSpinning ? 'animate-spin' : ''} />
      <span className="text-[9px] font-medium truncate" style={{ color: 'var(--textMuted)' }}>
        {paneId.slice(0, 10)}
      </span>
      {status?.message && (
        <span className="text-[8px] truncate flex-1" style={{ color: 'var(--textDim)' }}>
          {status.message.slice(0, 40)}
        </span>
      )}
      {outputInfo && (
        <span className="text-[8px] shrink-0" style={{ color: 'var(--textDim)' }}>
          {outputInfo.lineCount} lines
        </span>
      )}
      {status && (
        <span className="text-[8px] shrink-0" style={{ color: 'var(--textDim)' }}>
          {timeAgo(status.lastUpdatedAt)}
        </span>
      )}
    </div>
  )
}
