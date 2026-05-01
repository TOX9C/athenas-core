import { useState } from 'react'
import type { EvaluationBlock as EvaluationBlockType } from '../../store/athenaStore'
import { ChevronDown, ChevronRight, ClipboardCheck } from 'lucide-react'

interface EvaluationBlockViewProps {
  block: EvaluationBlockType
}

const statusColors: Record<string, string> = {
  success: '#22c55e',
  partial_success: '#f59e0b',
  failure: '#ef4444',
  needs_replanning: '#f59e0b',
  incomplete: '#6b7280',
}

const actionLabels: Record<string, string> = {
  done: 'Complete',
  replan: 'Replanning',
  retry_steps: 'Retrying',
  escalate_to_user: 'Needs Input',
}

export function EvaluationBlockView({ block }: EvaluationBlockViewProps) {
  const [expanded, setExpanded] = useState(false)

  const statusColor = statusColors[block.overallStatus] || '#6b7280'
  const actionLabel = actionLabels[block.nextAction] || block.nextAction

  return (
    <div
      className="rounded-md mt-1.5 mb-1 overflow-hidden"
      style={{
        border: `1px solid color-mix(in srgb, ${statusColor} 30%, transparent)`,
        background: `color-mix(in srgb, ${statusColor} 3%, transparent)`,
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
        <ClipboardCheck size={13} style={{ color: statusColor }} />
        <span className="text-[11px] font-medium flex-1" style={{ color: 'var(--text)' }}>
          Evaluation: {block.overallStatus.replace('_', ' ')}
        </span>
        <span
          className="text-[9px] px-1.5 py-0.5 rounded-full"
          style={{
            background: `color-mix(in srgb, ${statusColor} 15%, transparent)`,
            color: statusColor,
          }}
        >
          {actionLabel}
        </span>
      </button>

      {expanded && (
        <div className="px-3 pb-2">
          {block.reasoning && (
            <div className="text-[10px] mb-2 leading-relaxed" style={{ color: 'var(--textMuted)' }}>
              {block.reasoning}
            </div>
          )}

          {block.stepEvaluations.map((evalItem) => {
            const color = statusColors[evalItem.status] || '#6b7280'
            return (
              <div
                key={evalItem.step_id}
                className="flex items-start gap-2 py-1"
                style={{
                  borderTop: '1px solid color-mix(in srgb, var(--border) 30%, transparent)',
                }}
              >
                <div className="w-2 h-2 rounded-full mt-1 shrink-0" style={{ background: color }} />
                <div className="flex-1 min-w-0">
                  <span className="text-[10px] font-medium" style={{ color: 'var(--text)' }}>
                    {evalItem.step_id}
                  </span>
                  <span className="text-[9px] ml-1.5" style={{ color: 'var(--textDim)' }}>
                    {evalItem.summary}
                  </span>
                </div>
                <span className="text-[8px] shrink-0" style={{ color }}>
                  {evalItem.status}
                </span>
              </div>
            )
          })}
        </div>
      )}
    </div>
  )
}
