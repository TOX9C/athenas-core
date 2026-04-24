import { useEffect, useRef } from 'react'
import { X, Brain, Settings } from 'lucide-react'
import { useAthena } from './useAthena'
import { AthenaChatMessage } from './AthenaChatMessage'
import { AthenaInput } from './AthenaInput'
import { useAthenaStore } from '../../store/athenaStore'

export function AthenaPanel() {
  const { messages, isOpen, isPtyReady, sendMessage, spawnAthena, setOpen } = useAthena()
  const model = useAthenaStore((s) => s.model)
  const scrollRef = useRef<HTMLDivElement>(null)

  useEffect(() => {
    if (isOpen && !isPtyReady) {
      spawnAthena()
    }
  }, [isOpen, isPtyReady, spawnAthena])

  useEffect(() => {
    if (scrollRef.current) {
      scrollRef.current.scrollTop = scrollRef.current.scrollHeight
    }
  }, [messages])

  if (!isOpen) return null

  const modelLabels: Record<string, string> = {
    claude: 'Claude Code',
    codex: 'Codex',
    opencode: 'OpenCode',
    gemini: 'Gemini CLI',
    custom: 'Custom',
  }

  return (
    <div
      className="shrink-0 flex flex-col border-l"
      style={{
        width: 320,
        background: 'var(--bg)',
        borderColor: 'var(--border)',
      }}
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-3 py-2 border-b shrink-0"
        style={{ borderColor: 'var(--border)', background: 'var(--bgSecondary)' }}
      >
        <div className="flex items-center gap-2">
          <Brain size={14} style={{ color: 'var(--accent)' }} />
          <span className="text-xs font-semibold" style={{ color: 'var(--text)' }}>Athena</span>
          <span
            className="text-[9px] px-1.5 py-0.5 rounded-full"
            style={{ background: 'var(--bgTertiary)', color: 'var(--textDim)' }}
          >
            {modelLabels[model] ?? model}
          </span>
        </div>
        <div className="flex items-center gap-1">
          <button
            onClick={() => setOpen(false)}
            className="p-1 rounded hover:bg-white/10 transition-colors"
          >
            <X size={14} style={{ color: 'var(--textMuted)' }} />
          </button>
        </div>
      </div>

      {/* Chat area */}
      <div ref={scrollRef} className="flex-1 overflow-y-auto p-3 flex flex-col gap-2.5">
        {messages.length === 0 && (
          <div className="flex-1 flex items-center justify-center">
            <div className="flex flex-col items-center gap-2 text-center">
              <Brain size={28} style={{ color: 'var(--textDim)', opacity: 0.4 }} />
              <p className="text-[11px]" style={{ color: 'var(--textDim)' }}>
                Ask Athena to orchestrate agents, run tasks, or explain your codebase.
              </p>
            </div>
          </div>
        )}
        {messages.map((msg) => (
          <AthenaChatMessage key={msg.id} message={msg} />
        ))}
      </div>

      {/* Input */}
      <AthenaInput onSend={sendMessage} disabled={!isPtyReady} />
    </div>
  )
}
