import type { AgentRole } from '../../types/swarm'

const ROLE_COLORS: Record<AgentRole, string> = {
  coordinator: '#0ea5e9',
  builder: '#22c55e',
  scout: '#f59e0b',
  reviewer: '#06b6d4',
}

const ROLE_LABELS: Record<AgentRole, string> = {
  coordinator: 'Coordinator',
  builder: 'Builder',
  scout: 'Scout',
  reviewer: 'Reviewer',
}

interface SwarmRoleBadgeProps {
  role: AgentRole
}

export function SwarmRoleBadge({ role }: SwarmRoleBadgeProps) {
  return (
    <span
      className="inline-flex items-center px-1.5 py-0.5 rounded-full text-[9px] font-semibold uppercase tracking-wide"
      style={{
        background: `${ROLE_COLORS[role]}20`,
        color: ROLE_COLORS[role],
      }}
    >
      {ROLE_LABELS[role]}
    </span>
  )
}
