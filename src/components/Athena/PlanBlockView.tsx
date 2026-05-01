import { useState } from 'react'
import type { PlanBlock } from '../../store/athenaStore'
import { ChevronDown, ChevronRight, ListChecks } from 'lucide-react'

interface PlanBlockViewProps {
  block: PlanBlock
}

const statusIndicator: Record<string, { color: string; label: string }> = {
  pending: { color: 'var(--textDim)', label: 'Pending' },
  in_progress: { color: '#f59e0b', label: 'Running' },
  completed: { color: '#22c55e', label: 'Done' },
  failed: { color: '#ef4444', label: 'Failed' },
}

const agentBadgeColors: Record<string, string> = {
  claude: '#d97706',
  codex: '#3b82f6',
  opencode: '#8b5cf6',
  gemini: '#06b6d4',
  shell: '#6b7280',
}

export function PlanBlockView({ block }: PlanBlockViewProps) {
  const [expanded, setExpanded] = useState(true)

  const completedCount = block.steps.filter((s) => s.status === 'completed').length
  const totalCount = block.steps.length
  const planStatus = statusIndicator[block.status] || statusIndicator.pending

  return (
    <div
      className="rounded-md mt-1.5 mb-1 overflow-hidden"
      style={{
        border: '1px solid color-mix(in srgb, var(--border) 60%, transparent)',
        background: 'color-mix(in srgb, var(--bgSecondary) 50%, transparent)',
      }}
    >
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left transition-colors hover:bg-white/5"
      >
        {expanded ? (
          <ChevronDown size={12} style={{ color: 'var(--textDim)' }} />
        ) : (
          <ChevronRight size={12} style={{ color: 'var(--textDim)' }} />
        )}
        <ListChecks size={13} style={{ color: 'var(--accent)' }} />
        <span className="text-[11px] font-medium flex-1" style={{ color: 'var(--text)' }}>
          {block.goal}
        </span>
        <span
          className="text-[9px] px-1.5 py-0.5 rounded-full"
          style={{
            background: `color-mix(in srgb, ${planStatus.color} 15%, transparent)`,
            color: planStatus.color,
          }}
        >
          {completedCount}/{totalCount} {planStatus.label}
        </span>
      </button>

      {expanded && (
        <div className="px-3 pb-2">
          <div className="flex gap-0.5 mb-2 rounded overflow-hidden" style={{ height: 3 }}>
            {block.steps.map((step) => {
              const si = statusIndicator[step.status] || statusIndicator.pending
              return (
                <div
                  key={step.id}
                  style={{
                    flex: 1,
                    background:
                      step.status === 'pending'
                        ? 'color-mix(in srgb, var(--textDim) 20%, transparent)'
                        : si.color,
                    transition: 'background 0.3s ease',
                  }}
                />
              )
            })}
          </div>

          {block.steps.map((step) => {
            const si = statusIndicator[step.status] || statusIndicator.pending
            const badgeColor = agentBadgeColors[step.agent_type] || '#6b7280'

            return (
              <div
                key={step.id}
                className="flex items-start gap-2 py-1.5"
                style={{
                  borderTop: '1px solid color-mix(in srgb, var(--border) 30%, transparent)',
                }}
              >
                <div
                  className="w-2 h-2 rounded-full mt-1 shrink-0"
                  style={{ background: si.color }}
                />
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-1.5">
                    <span className="text-[10px] font-medium" style={{ color: 'var(--text)' }}>
                      {step.title}
                    </span>
                    <span
                      className="text-[8px] px-1 py-px rounded"
                      style={{
                        background: `color-mix(in srgb, ${badgeColor} 15%, transparent)`,
                        color: badgeColor,
                      }}
                    >
                      {step.agent_type}
                    </span>
                  </div>
                  {step.result_summary && (
                    <div className="text-[9px] mt-0.5" style={{ color: 'var(--textDim)' }}>
                      {step.result_summary}
                    </div>
                  )}
                  {step.assigned_pane_id && (
                    <div
                      className="text-[8px] mt-0.5 font-mono"
                      style={{ color: 'var(--textDim)', opacity: 0.6 }}
                    >
                      {step.assigned_pane_id}
                    </div>
                  )}
                </div>
                <span className="text-[8px] shrink-0 mt-0.5" style={{ color: si.color }}>
                  {si.label}
                </span>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
