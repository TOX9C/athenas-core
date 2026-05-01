import { useMemo } from 'react'
import type { OutputLine } from '../../store/agentOutputStore'

function formatTime(ts: number): string {
  const d = new Date(ts)
  return d.toLocaleTimeString('en-US', {
    hour12: false,
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  })
}

function isStderrLike(text: string): boolean {
  const lower = text.toLowerCase()
  return (
    lower.includes('error') ||
    lower.includes('warn') ||
    lower.includes('fail') ||
    lower.includes('exception')
  )
}

export function AgentOutputLine({
  line,
  showLineNumbers,
}: {
  line: OutputLine
  showLineNumbers?: boolean
}) {
  const isErr = useMemo(() => isStderrLike(line.text), [line.text])

  return (
    <div
      className="flex items-start gap-2 px-2 py-px hover:bg-white/[0.02] transition-colors font-mono text-[11px] leading-[1.6]"
      style={{ color: isErr ? 'var(--error)' : 'var(--textMuted)' }}
    >
      {showLineNumbers && (
        <span
          className="shrink-0 text-right select-none"
          style={{ width: 36, color: 'var(--textDim)', opacity: 0.5 }}
        >
          {line.lineNum}
        </span>
      )}
      <span className="shrink-0 select-none" style={{ color: 'var(--textDim)', opacity: 0.4 }}>
        {formatTime(line.timestamp)}
      </span>
      <span className="flex-1 min-w-0 whitespace-pre-wrap break-all">{line.text}</span>
    </div>
  )
}
