import { useState } from 'react'
import { Plus, MessageSquare, Trash2, Clock } from 'lucide-react'
import { useSessionStore } from '../../store/sessionStore'

function relativeTime(timestamp: number): string {
  const now = Date.now()
  const diffMs = now - timestamp
  const diffSec = Math.floor(diffMs / 1000)
  const diffMin = Math.floor(diffSec / 60)
  const diffHr = Math.floor(diffMin / 60)
  const diffDay = Math.floor(diffHr / 24)

  if (diffSec < 60) return 'just now'
  if (diffMin < 60) return `${diffMin}m ago`
  if (diffHr < 24) return `${diffHr}h ago`
  if (diffDay === 1) return 'yesterday'
  if (diffDay < 7) return `${diffDay}d ago`
  return new Date(timestamp).toLocaleDateString()
}

export function SessionList() {
  const sessions = useSessionStore((s) => s.sessions)
  const activeSessionId = useSessionStore((s) => s.activeSessionId)
  const switchSession = useSessionStore((s) => s.switchSession)
  const newSession = useSessionStore((s) => s.newSession)
  const deleteSession = useSessionStore((s) => s.deleteSession)
  const [hoveredId, setHoveredId] = useState<string | null>(null)

  const handleNewSession = async () => {
    try {
      await newSession()
    } catch (err) {
      console.error('[SessionList] Failed to create session:', err)
    }
  }

  const handleDelete = async (e: React.MouseEvent, sessionId: string) => {
    e.stopPropagation()
    try {
      await deleteSession(sessionId)
    } catch (err) {
      console.error('[SessionList] Failed to delete session:', err)
    }
  }

  return (
    <div className="flex flex-col h-full">
      <div
        className="flex items-center justify-between px-3 py-2 border-b shrink-0"
        style={{ borderColor: 'var(--border)' }}
      >
        <span
          className="text-[10px] font-semibold uppercase tracking-wider"
          style={{ color: 'var(--textDim)' }}
        >
          Sessions
        </span>
        <button
          onClick={handleNewSession}
          className="p-1 rounded hover:bg-white/10 transition-colors"
          title="New Session"
        >
          <Plus size={12} style={{ color: 'var(--textMuted)' }} />
        </button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {sessions.length === 0 && (
          <div className="flex flex-col items-center justify-center py-8 px-4 text-center">
            <MessageSquare size={20} style={{ color: 'var(--textDim)', opacity: 0.3 }} />
            <p className="text-[10px] mt-2 leading-relaxed" style={{ color: 'var(--textDim)' }}>
              No sessions yet.
            </p>
            <p className="text-[10px] mt-1" style={{ color: 'var(--textDim)' }}>
              Click{' '}
              <Plus
                size={9}
                className="inline"
                style={{ color: 'var(--accent)', verticalAlign: 'middle' }}
              />{' '}
              to create one.
            </p>
          </div>
        )}

        {sessions.map((session) => {
          const isActive = session.id === activeSessionId
          const isHovered = session.id === hoveredId

          return (
            <div
              key={session.id}
              onClick={() => switchSession(session.id)}
              onMouseEnter={() => setHoveredId(session.id)}
              onMouseLeave={() => setHoveredId(null)}
              className="flex items-start gap-2 px-3 py-2 cursor-pointer transition-colors group border-b"
              style={{
                borderColor: 'var(--border)',
                background: isActive
                  ? 'color-mix(in srgb, var(--accent) 10%, transparent)'
                  : isHovered
                    ? 'rgba(255,255,255,0.03)'
                    : 'transparent',
                borderLeft: isActive ? '2px solid var(--accent)' : '2px solid transparent',
              }}
            >
              <MessageSquare
                size={12}
                className="shrink-0 mt-0.5"
                style={{ color: isActive ? 'var(--accent)' : 'var(--textDim)' }}
              />

              <div className="flex-1 min-w-0">
                <div className="flex items-center justify-between gap-1">
                  <span
                    className="text-[11px] font-medium truncate"
                    style={{ color: isActive ? 'var(--text)' : 'var(--textMuted)' }}
                  >
                    {session.title}
                  </span>
                  {(isHovered || isActive) && (
                    <button
                      onClick={(e) => handleDelete(e, session.id)}
                      className="p-0.5 rounded hover:bg-red-500/20 transition-colors shrink-0"
                      title="Delete session"
                    >
                      <Trash2 size={10} style={{ color: 'var(--error)' }} />
                    </button>
                  )}
                </div>

                {session.lastMessagePreview && (
                  <p
                    className="text-[10px] truncate mt-0.5 leading-tight"
                    style={{ color: 'var(--textDim)' }}
                  >
                    {session.lastMessagePreview}
                  </p>
                )}

                <div className="flex items-center gap-1.5 mt-1">
                  <Clock size={8} style={{ color: 'var(--textDim)', opacity: 0.6 }} />
                  <span className="text-[9px]" style={{ color: 'var(--textDim)', opacity: 0.6 }}>
                    {relativeTime(session.updatedAt)}
                  </span>
                  {session.messageCount > 0 && (
                    <span className="text-[9px]" style={{ color: 'var(--textDim)', opacity: 0.6 }}>
                      &middot; {session.messageCount} msgs
                    </span>
                  )}
                </div>
              </div>
            </div>
          )
        })}
      </div>
    </div>
  )
}
