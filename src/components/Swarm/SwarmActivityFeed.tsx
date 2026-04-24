import type { SwarmAgent, AgentRole } from '../../types/swarm'

interface ActivityEntry {
  timestamp: number
  role: AgentRole
  agentType: string
  action: string
}

const ROLE_COLORS: Record<AgentRole, string> = {
  coordinator: '#6366f1',
  builder: '#22c55e',
  scout: '#f59e0b',
  reviewer: '#a855f7',
}

interface SwarmActivityFeedProps {
  agents: SwarmAgent[]
}

export function SwarmActivityFeed({ agents }: SwarmActivityFeedProps) {
  const entries: ActivityEntry[] = agents
    .filter((a) => a.lastAction)
    .map((a) => ({
      timestamp: a.lastActionAt,
      role: a.role,
      agentType: a.agentType,
      action: a.lastAction,
    }))
    .sort((a, b) => b.timestamp - a.timestamp)

  if (entries.length === 0) {
    return (
      <div className="p-3 text-[11px]" style={{ color: 'var(--textDim)' }}>
        No activity yet
      </div>
    )
  }

  return (
    <div className="flex flex-col gap-0.5 p-2 overflow-y-auto max-h-[300px]">
      {entries.map((entry, idx) => {
        const time = new Date(entry.timestamp)
        const timeStr = time.toLocaleTimeString('en-US', { hour12: false })
        return (
          <div
            key={idx}
            className="flex items-start gap-2 py-1 px-1.5 rounded text-[10px] font-mono"
            style={{ color: 'var(--textMuted)' }}
          >
            <span style={{ color: 'var(--textDim)' }}>{timeStr}</span>
            <span
              className="font-semibold uppercase shrink-0"
              style={{ color: ROLE_COLORS[entry.role], minWidth: 80 }}
            >
              [{entry.role}]
            </span>
            <span className="truncate">{entry.action}</span>
          </div>
        )
      })}
    </div>
  )
}
