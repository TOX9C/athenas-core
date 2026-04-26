import { useSwarmStore } from '../../store/swarmStore'
import { AgentCard } from './AgentCard'
import { SwarmActivityFeed } from './SwarmActivityFeed'
import { Pause, Play, XCircle, Users } from 'lucide-react'

export function SwarmBoard() {
  const { activeSwarm, updateSwarm } = useSwarmStore()

  if (!activeSwarm) {
    return (
      <div className="flex-1 h-full w-full flex items-center justify-center">
        <div className="flex flex-col items-center gap-2" style={{ color: 'var(--textDim)' }}>
          <Users size={32} style={{ opacity: 0.3 }} />
          <span className="text-xs">Launch a swarm to see the board</span>
        </div>
      </div>
    )
  }

  const handlePauseResume = () => {
    updateSwarm({
      status: activeSwarm.status === 'active' ? 'paused' : 'active',
    })
  }

  const handleAbort = () => {
    updateSwarm({ status: 'completed' })
  }

  return (
    <div className="flex-1 h-full w-full flex flex-col min-h-0">
      {/* Toolbar */}
      <div
        className="flex items-center justify-between px-3 py-2 border-b shrink-0"
        style={{ borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
      >
        <div className="flex items-center gap-2">
          <Users size={14} style={{ color: 'var(--accent)' }} />
          <span className="text-xs font-semibold" style={{ color: 'var(--text)' }}>
            Swarm: {activeSwarm.goal.slice(0, 50)}{activeSwarm.goal.length > 50 ? '...' : ''}
          </span>
          <span
            className="px-1.5 py-0.5 rounded-full text-[9px] font-medium uppercase"
            style={{
              background: activeSwarm.status === 'active' ? 'rgba(34,197,94,0.15)' : 'rgba(239,68,68,0.15)',
              color: activeSwarm.status === 'active' ? 'var(--success)' : 'var(--error)',
            }}
          >
            {activeSwarm.status}
          </span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={handlePauseResume}
            className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium transition-colors"
            style={{ background: 'var(--bgTertiary)', color: 'var(--textMuted)' }}
          >
            {activeSwarm.status === 'active' ? <Pause size={10} /> : <Play size={10} />}
            {activeSwarm.status === 'active' ? 'Pause' : 'Resume'}
          </button>
          <button
            onClick={handleAbort}
            className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium transition-colors"
            style={{ background: 'rgba(239,68,68,0.15)', color: 'var(--error)' }}
          >
            <XCircle size={10} />
            Abort
          </button>
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 flex min-h-0">
        {/* Agent cards */}
        <div className="flex-1 p-3 overflow-y-auto">
          <div className="grid grid-cols-2 gap-2">
            {activeSwarm.agents.map((agent) => (
              <AgentCard
                key={agent.id}
                agent={agent}
                onNudge={
                  agent.status === 'stalled'
                    ? () => {
                        window.athena.pty.write(agent.paneId, '\nPlease continue working on your assigned task.\n')
                      }
                    : undefined
                }
              />
            ))}
          </div>
        </div>

        {/* Activity feed */}
        <div
          className="w-[280px] shrink-0 border-l flex flex-col"
          style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
        >
          <div className="px-3 py-2 border-b" style={{ borderColor: 'var(--border)' }}>
            <span className="text-[11px] font-semibold" style={{ color: 'var(--textMuted)' }}>
              Activity Feed
            </span>
          </div>
          <SwarmActivityFeed agents={activeSwarm.agents} />
        </div>
      </div>
    </div>
  )
}
