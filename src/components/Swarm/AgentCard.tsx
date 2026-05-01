import type { SwarmAgent } from '../../types/swarm'
import { SwarmRoleBadge } from './SwarmRoleBadge'
import { getAgentLabel, getAgentColor } from '../../utils/agentCommands'
import { AlertTriangle, Zap } from 'lucide-react'

interface AgentCardProps {
  agent: SwarmAgent
  onNudge?: () => void
}

const STATUS_COLORS: Record<string, string> = {
  idle: 'var(--textDim)',
  thinking: 'var(--accent)',
  writing: 'var(--success)',
  waiting: 'var(--warning)',
  done: 'var(--success)',
  blocked: 'var(--error)',
  stalled: 'var(--error)',
}

export function AgentCard({ agent, onNudge }: AgentCardProps) {
  const isStalled = agent.status === 'stalled'

  return (
    <div
      className="rounded-md p-2.5 flex flex-col gap-1.5"
      style={{
        background: 'var(--bgSecondary)',
        border: `1px solid ${isStalled ? 'var(--error)' : 'var(--border)'}`,
      }}
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-1.5">
          <div
            className="w-2 h-2 rounded-full"
            style={{ background: getAgentColor(agent.agentType) }}
          />
          <span className="text-[11px] font-medium" style={{ color: 'var(--text)' }}>
            {getAgentLabel(agent.agentType)}
          </span>
        </div>
        <SwarmRoleBadge role={agent.role} />
      </div>

      <div className="flex items-center gap-1.5">
        <div
          className="w-1.5 h-1.5 rounded-full animate-pulse"
          style={{ background: STATUS_COLORS[agent.status] ?? 'var(--textDim)' }}
        />
        <span className="text-[10px] capitalize" style={{ color: 'var(--textMuted)' }}>
          {agent.status}
        </span>
      </div>

      {agent.lastAction && (
        <p className="text-[10px] truncate" style={{ color: 'var(--textDim)' }}>
          {agent.lastAction}
        </p>
      )}

      {isStalled && onNudge && (
        <button
          onClick={onNudge}
          className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium mt-1 transition-colors"
          style={{ background: 'var(--error)', color: '#fff' }}
        >
          <Zap size={10} />
          Nudge Agent
        </button>
      )}
    </div>
  )
}
