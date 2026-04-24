import type { AthenaMessage } from '../../store/athenaStore'
import { User, Brain } from 'lucide-react'

interface AthenaChatMessageProps {
  message: AthenaMessage
}

export function AthenaChatMessage({ message }: AthenaChatMessageProps) {
  const isUser = message.role === 'user'
  const time = new Date(message.timestamp).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })

  return (
    <div
      className="flex gap-3 py-3 px-4 text-xs w-full border-b"
      style={{
        background: isUser ? 'transparent' : 'rgba(0,0,0,0.1)',
        borderColor: 'var(--border)',
        borderLeft: isUser ? '3px solid transparent' : '3px solid var(--accent)',
      }}
    >
      <div className="shrink-0 mt-0.5">
        {isUser ? (
          <div className="w-5 h-5 rounded-full flex items-center justify-center border" style={{ background: 'var(--bgSecondary)', borderColor: 'var(--border)' }}>
            <User size={12} style={{ color: 'var(--textMuted)' }} />
          </div>
        ) : (
          <div className="w-5 h-5 rounded-full flex items-center justify-center" style={{ background: 'var(--accent)' }}>
            <Brain size={12} style={{ color: '#fff' }} />
          </div>
        )}
      </div>

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <span className="font-semibold" style={{ color: 'var(--text)' }}>
            {isUser ? 'You' : 'Athena'}
          </span>
          <span className="text-[10px]" style={{ color: 'var(--textDim)' }}>
            {time}
          </span>
        </div>

        <div className="text-xs leading-relaxed whitespace-pre-wrap break-words" style={{ color: 'var(--text)' }}>
          {message.content}
        </div>
      </div>
    </div>
  )
}
