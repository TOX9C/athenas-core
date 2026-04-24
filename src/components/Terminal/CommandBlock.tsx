import { useState } from 'react'
import { ChevronRight, ChevronDown, Check, X, Copy, RotateCw, Clock } from 'lucide-react'

interface CommandBlockProps {
  id: string
  command: string
  output: string
  exitCode: number | null
  startedAt: number
  finishedAt: number | null
  collapsed: boolean
  onToggle: () => void
  onRerun?: () => void
  onCopyCommand?: () => void
  onCopyOutput?: () => void
}

function formatRelativeTime(ts: number): string {
  const diff = Date.now() - ts
  if (diff < 60_000) return 'just now'
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`
  return `${Math.floor(diff / 3_600_000)}h ago`
}

function formatDuration(start: number, end: number | null): string {
  if (!end) return 'running...'
  const ms = end - start
  if (ms < 1000) return `${ms}ms`
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`
  return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`
}

export function CommandBlock({
  command,
  output,
  exitCode,
  startedAt,
  finishedAt,
  collapsed,
  onToggle,
  onRerun,
  onCopyCommand,
  onCopyOutput,
}: CommandBlockProps) {
  const [hovering, setHovering] = useState(false)
  const isRunning = exitCode === null
  const isSuccess = exitCode === 0
  const previewLine = output.split('\n').find((l) => l.trim()) ?? ''

  return (
    <div
      className="rounded-md overflow-hidden transition-colors"
      style={{
        background: 'var(--bgSecondary)',
        border: '1px solid var(--border)',
      }}
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
    >
      {/* Header */}
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-2 px-2.5 py-1.5 text-left transition-colors hover:bg-white/[0.03]"
      >
        {collapsed ? (
          <ChevronRight size={12} style={{ color: 'var(--textDim)', flexShrink: 0 }} />
        ) : (
          <ChevronDown size={12} style={{ color: 'var(--textDim)', flexShrink: 0 }} />
        )}

        <span className="font-mono text-[12px] font-medium flex-1 truncate" style={{ color: 'var(--text)' }}>
          {command}
        </span>

        {/* Exit code badge */}
        {isRunning ? (
          <span
            className="flex items-center gap-1 px-1.5 py-0.5 rounded-full text-[9px] font-medium"
            style={{ background: 'var(--warning)', color: '#000' }}
          >
            <Clock size={9} className="animate-pulse" />
            running
          </span>
        ) : isSuccess ? (
          <span
            className="flex items-center gap-0.5 px-1.5 py-0.5 rounded-full text-[9px] font-medium"
            style={{ background: 'rgba(34,197,94,0.15)', color: 'var(--success)' }}
          >
            <Check size={9} />
            0
          </span>
        ) : (
          <span
            className="flex items-center gap-0.5 px-1.5 py-0.5 rounded-full text-[9px] font-medium"
            style={{ background: 'rgba(239,68,68,0.15)', color: 'var(--error)' }}
          >
            <X size={9} />
            {exitCode}
          </span>
        )}

        <span className="text-[10px] shrink-0" style={{ color: 'var(--textDim)' }}>
          {formatDuration(startedAt, finishedAt)}
        </span>

        {/* Action buttons on hover */}
        {hovering && (
          <div className="flex items-center gap-0.5 shrink-0" onClick={(e) => e.stopPropagation()}>
            {onCopyCommand && (
              <button
                onClick={onCopyCommand}
                className="p-1 rounded hover:bg-white/10 transition-colors"
                title="Copy command"
              >
                <Copy size={10} style={{ color: 'var(--textDim)' }} />
              </button>
            )}
            {onRerun && (
              <button
                onClick={onRerun}
                className="p-1 rounded hover:bg-white/10 transition-colors"
                title="Re-run"
              >
                <RotateCw size={10} style={{ color: 'var(--textDim)' }} />
              </button>
            )}
          </div>
        )}
      </button>

      {/* Body */}
      {collapsed ? (
        previewLine && (
          <div className="px-2.5 pb-1.5">
            <span
              className="font-mono text-[11px] truncate block"
              style={{ color: 'var(--textDim)' }}
            >
              {previewLine.replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '').slice(0, 80)}
            </span>
          </div>
        )
      ) : (
        output && (
          <div
            className="px-2.5 pb-2 max-h-[300px] overflow-y-auto"
            style={{ borderTop: '1px solid var(--border)' }}
          >
            <pre
              className="font-mono text-[11px] leading-[1.5] whitespace-pre-wrap break-all pt-1.5"
              style={{ color: 'var(--textMuted)' }}
            >
              {output}
            </pre>
          </div>
        )
      )}
    </div>
  )
}
