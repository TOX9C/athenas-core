import { Bot, Circle, Loader, AlertTriangle, CheckCircle, Pause, XCircle, Zap } from 'lucide-react'
import { useMemo } from 'react'
import { useAgentStatusStore } from '../../store/agentStatusStore'
import { useNotificationStore } from '../../store/notificationStore'
import { getAgentColor, getAgentLabel } from '../../utils/agentCommands'
import type { AgentType } from '../../types/workspace'

const STATUS_CONFIG: Record<string, { icon: typeof Circle; color: string; label: string }> = {
  idle: { icon: Circle, color: 'var(--textDim)', label: 'Idle' },
  thinking: { icon: Loader, color: 'var(--accent)', label: 'Thinking' },
  working: { icon: Loader, color: 'var(--accent)', label: 'Working' },
  writing: { icon: Loader, color: 'var(--success)', label: 'Writing' },
  waiting_for_input: { icon: Pause, color: 'var(--warning)', label: 'Waiting' },
  waiting: { icon: Pause, color: 'var(--warning)', label: 'Waiting' },
  completed: { icon: CheckCircle, color: 'var(--success)', label: 'Completed' },
  done: { icon: CheckCircle, color: 'var(--success)', label: 'Done' },
  error: { icon: AlertTriangle, color: 'var(--error)', label: 'Error' },
  blocked: { icon: XCircle, color: 'var(--error)', label: 'Blocked' },
  stalled: { icon: AlertTriangle, color: 'var(--error)', label: 'Stalled' },
  cancelled: { icon: XCircle, color: 'var(--textDim)', label: 'Cancelled' },
  disconnected: { icon: Circle, color: 'var(--textDim)', label: 'Disconnected' },
}

function timeAgo(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 5) return 'now'
  if (diff < 60) return `${diff}s`
  if (diff < 3600) return `${Math.floor(diff / 60)}m`
  return `${Math.floor(diff / 3600)}h`
}

function AgentStatusRow({
  status,
}: {
  status: {
    paneId: string
    status: string
    message?: string
    progress?: { current: number; total: number; label: string }
    lastUpdatedAt: number
  }
}) {
  const cfg = STATUS_CONFIG[status.status] ?? STATUS_CONFIG.idle
  const Icon = cfg.icon
  const isAnimated = ['thinking', 'working', 'writing'].includes(status.status)

  return (
    <div
      className="flex items-center gap-2 px-3 py-2 rounded-md transition-colors hover:bg-white/[0.03]"
      style={{ borderBottom: '1px solid var(--border)' }}
    >
      <Icon
        size={11}
        style={{ color: cfg.color, flexShrink: 0 }}
        className={isAnimated ? 'animate-spin' : ''}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-[11px] font-medium truncate" style={{ color: 'var(--text)' }}>
            {status.paneId.slice(0, 8)}
          </span>
          <span
            className="text-[9px] px-1 py-px rounded"
            style={{ background: `${cfg.color}22`, color: cfg.color }}
          >
            {cfg.label}
          </span>
        </div>
        {status.message && (
          <span className="text-[9px] block truncate mt-0.5" style={{ color: 'var(--textDim)' }}>
            {status.message}
          </span>
        )}
        {status.progress && (
          <div className="flex items-center gap-1.5 mt-1">
            <div
              className="flex-1 h-1 rounded-full overflow-hidden"
              style={{ background: 'var(--bgTertiary)' }}
            >
              <div
                className="h-full rounded-full transition-all"
                style={{
                  width: `${Math.min(100, (status.progress.current / status.progress.total) * 100)}%`,
                  background: cfg.color,
                }}
              />
            </div>
            <span className="text-[8px] shrink-0" style={{ color: 'var(--textDim)' }}>
              {status.progress.current}/{status.progress.total}
            </span>
          </div>
        )}
      </div>
      <span className="text-[8px] shrink-0" style={{ color: 'var(--textDim)' }}>
        {timeAgo(status.lastUpdatedAt)}
      </span>
    </div>
  )
}

function AgentStatusEntryRow({
  entry,
}: {
  entry: {
    id: string
    name: string
    agentType: AgentType
    status: string
    lastAction: string
    lastActionAt: number
    connectedAt: number
  }
}) {
  const cfg = STATUS_CONFIG[entry.status] ?? STATUS_CONFIG.idle
  const Icon = cfg.icon
  const isAnimated = ['thinking', 'working', 'writing'].includes(entry.status)

  return (
    <div
      className="flex items-start gap-2 px-3 py-2 transition-colors hover:bg-white/[0.03]"
      style={{ borderBottom: '1px solid var(--border)' }}
    >
      <div className="flex flex-col items-center gap-0.5 pt-0.5">
        <div
          className="w-2 h-2 rounded-full"
          style={{ background: getAgentColor(entry.agentType) }}
        />
        <Icon size={9} style={{ color: cfg.color }} className={isAnimated ? 'animate-spin' : ''} />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5">
          <span className="text-[11px] font-medium truncate" style={{ color: 'var(--text)' }}>
            {entry.name}
          </span>
          <span
            className="text-[9px] px-1 py-px rounded"
            style={{ background: `${cfg.color}22`, color: cfg.color }}
          >
            {cfg.label}
          </span>
        </div>
        <span className="text-[9px] block truncate mt-0.5" style={{ color: 'var(--textDim)' }}>
          {entry.lastAction}
        </span>
      </div>
      <span className="text-[8px] shrink-0 mt-1" style={{ color: 'var(--textDim)' }}>
        {timeAgo(entry.lastActionAt)}
      </span>
    </div>
  )
}

export function AgentStatusList() {
  // Use stable selectors — primitives and direct references only.
  // Object.values() creates a new array on every store update, causing
  // infinite re-renders when agents push frequent status updates.
  const paneStatusMap = useAgentStatusStore((s) => s.statuses)
  const agentEntries = useNotificationStore((s) => s.agentStatuses)
  const connectedCount = useNotificationStore(
    (s) => s.agentStatuses.filter((a) => a.status !== 'disconnected').length,
  )

  const paneStatuses = useMemo(() => Object.values(paneStatusMap), [paneStatusMap])
  const activePanes = useMemo(() => paneStatuses.filter((s) => s.status !== 'idle'), [paneStatuses])
  const idlePanes = useMemo(() => paneStatuses.filter((s) => s.status === 'idle'), [paneStatuses])

  return (
    <div className="flex flex-col h-full">
      <div
        className="flex items-center justify-between px-3 py-2 border-b"
        style={{ borderColor: 'var(--border)' }}
      >
        <div className="flex items-center gap-2">
          <Bot size={13} style={{ color: 'var(--accent)' }} />
          <span className="text-[11px] font-semibold" style={{ color: 'var(--text)' }}>
            Agents
          </span>
          {connectedCount > 0 && (
            <span
              className="text-[9px] px-1.5 py-0.5 rounded-full"
              style={{ background: 'var(--success)', color: '#fff', opacity: 0.8 }}
            >
              {connectedCount} connected
            </span>
          )}
        </div>
      </div>

      <div className="flex-1 overflow-y-auto">
        {agentEntries.length > 0 && (
          <div>
            <div className="px-3 py-1.5">
              <span
                className="text-[9px] font-semibold uppercase tracking-wider"
                style={{ color: 'var(--textDim)' }}
              >
                Connected Agents
              </span>
            </div>
            {agentEntries.map((entry) => (
              <AgentStatusEntryRow key={entry.id} entry={entry} />
            ))}
          </div>
        )}

        {activePanes.length > 0 && (
          <div>
            <div className="px-3 py-1.5">
              <span
                className="text-[9px] font-semibold uppercase tracking-wider"
                style={{ color: 'var(--textDim)' }}
              >
                Active
              </span>
            </div>
            {activePanes.map((s) => (
              <AgentStatusRow key={s.paneId} status={s} />
            ))}
          </div>
        )}

        {idlePanes.length > 0 && (
          <div>
            <div className="px-3 py-1.5">
              <span
                className="text-[9px] font-semibold uppercase tracking-wider"
                style={{ color: 'var(--textDim)' }}
              >
                Idle
              </span>
            </div>
            {idlePanes.map((s) => (
              <AgentStatusRow key={s.paneId} status={s} />
            ))}
          </div>
        )}

        {agentEntries.length === 0 && paneStatuses.length === 0 && (
          <div className="flex flex-col items-center justify-center py-10 gap-2">
            <Bot size={24} style={{ color: 'var(--textDim)', opacity: 0.3 }} />
            <span className="text-[10px]" style={{ color: 'var(--textDim)' }}>
              No agents active
            </span>
            <span className="text-[9px]" style={{ color: 'var(--textDim)' }}>
              Launch a terminal to see agent status
            </span>
          </div>
        )}
      </div>
    </div>
  )
}
