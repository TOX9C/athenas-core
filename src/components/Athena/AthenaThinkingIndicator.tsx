import { useEffect, useState } from 'react'
import { Brain, Wrench } from 'lucide-react'

const THINKING_LABELS = [
  'Athena is thinking',
  'Processing context',
  'Reasoning',
  'Analyzing workspace',
  'Formulating response',
]

const TOOL_LABELS: Record<string, string> = {
  tool_dispatched: 'Executing tool',
  tool_done: 'Tool complete',
  complete: 'Done',
  failed: 'Failed',
  thinking: 'Thinking',
}

interface StatusLogItem {
  status: string
  message?: string
  id?: string
  tool?: string
  timestamp?: number
}

interface AthenaThinkingIndicatorProps {
  statusLog: StatusLogItem[]
}

export function AthenaThinkingIndicator({ statusLog }: AthenaThinkingIndicatorProps) {
  const [labelIndex, setLabelIndex] = useState(0)
  const [dotCount, setDotCount] = useState(0)
  const [elapsedSec, setElapsedSec] = useState(0)

  const latestStatus = statusLog[statusLog.length - 1]
  const isToolActive = latestStatus?.status === 'tool_dispatched'
  const statusLabel = latestStatus
    ? (TOOL_LABELS[latestStatus.status] ?? latestStatus.status)
    : null
  const toolName = (latestStatus as any)?.tool ?? latestStatus?.message

  useEffect(() => {
    const labelInterval = setInterval(() => {
      setLabelIndex((i) => (i + 1) % THINKING_LABELS.length)
    }, 3000)
    return () => clearInterval(labelInterval)
  }, [])

  useEffect(() => {
    const dotInterval = setInterval(() => {
      setDotCount((d) => (d + 1) % 4)
    }, 400)
    return () => clearInterval(dotInterval)
  }, [])

  useEffect(() => {
    const timer = setInterval(() => {
      setElapsedSec((s) => s + 1)
    }, 1000)
    return () => clearInterval(timer)
  }, [])

  const dots = '.'.repeat(dotCount)

  const formatElapsed = (sec: number) => {
    if (sec < 60) return `${sec}s`
    const m = Math.floor(sec / 60)
    const s = sec % 60
    return `${m}m ${s}s`
  }

  const statusSteps = statusLog.slice(-4)

  return (
    <div className={`athena-thinking-indicator${isToolActive ? ' tool-active' : ''}`}>
      <div className="athena-thinking-glow" />

      <div className="flex items-center gap-2.5">
        <div className="athena-thinking-icon-wrap">
          {isToolActive ? (
            <Wrench size={12} className="athena-thinking-icon" />
          ) : (
            <Brain size={12} className="athena-thinking-icon" />
          )}
          <div
            className="athena-thinking-ring"
            style={isToolActive ? { borderColor: 'var(--warning)' } : undefined}
          />
        </div>

        <div className="flex flex-col gap-0.5 flex-1 min-w-0">
          <div className="flex items-center gap-1.5">
            <span
              className="text-[11px] font-medium athena-thinking-text"
              style={{ color: 'var(--accent)' }}
            >
              {isToolActive ? (statusLabel ?? 'Working') : THINKING_LABELS[labelIndex]}
            </span>
            <span
              className="text-[11px] athena-thinking-dots"
              style={{ color: 'var(--accent)', opacity: 0.6, width: 12 }}
            >
              {dots}
            </span>

            <span
              className="text-[9px] ml-auto shrink-0 tabular-nums"
              style={{ color: 'var(--textDim)' }}
            >
              {formatElapsed(elapsedSec)}
            </span>
          </div>

          {toolName && (
            <span
              className="text-[9px] font-mono px-1.5 py-0.5 rounded truncate inline-block max-w-[200px]"
              style={{
                color: isToolActive ? 'var(--warning)' : 'var(--textDim)',
                background: isToolActive ? 'rgba(245,158,11,0.08)' : 'transparent',
                border: isToolActive ? '1px solid rgba(245,158,11,0.15)' : 'none',
              }}
            >
              {toolName}
            </span>
          )}

          {statusSteps.length > 1 && (
            <div className="flex items-center gap-1 mt-1 flex-wrap">
              {statusSteps.map((step, i) => {
                const isActive = i === statusSteps.length - 1
                const dotColor =
                  step.status === 'failed'
                    ? 'var(--error)'
                    : step.status === 'complete' || step.status === 'tool_done'
                      ? 'var(--success)'
                      : step.status === 'tool_dispatched'
                        ? 'var(--warning)'
                        : 'var(--accent)'
                return (
                  <span
                    key={i}
                    className="w-1.5 h-1.5 rounded-full shrink-0 transition-all"
                    style={{
                      background: dotColor,
                      opacity: isActive ? 1 : 0.4,
                      boxShadow: isActive ? `0 0 4px ${dotColor}` : 'none',
                    }}
                    title={`${step.status}${step.tool ? `: ${step.tool}` : ''}`}
                  />
                )
              })}
            </div>
          )}
        </div>
      </div>

      <div className="athena-thinking-pulse-bar">
        <div
          className="athena-thinking-pulse-bar-inner"
          style={{
            background: isToolActive ? 'var(--warning)' : 'var(--accent)',
            opacity: isToolActive ? 0.5 : 0.6,
          }}
        />
      </div>
    </div>
  )
}
