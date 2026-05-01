import { useState, useRef, useEffect } from 'react'
import { Bell, Check, Trash2 } from 'lucide-react'
import { useNotificationStore, isEnhanced } from '../../store/notificationStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { getAgentColor } from '../../utils/agentCommands'

function timeAgo(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

export function NotificationBell() {
  const [open, setOpen] = useState(false)
  const ref = useRef<HTMLDivElement>(null)
  const { notifications, markRead, markAllRead, clearAll } = useNotificationStore()
  const { setActiveSpace } = useWorkspaceStore()

  const unreadCount = notifications.filter((n) => !n.read).length

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setOpen(false)
      }
    }
    if (open) document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [open])

  const handleClick = (n: (typeof notifications)[0]) => {
    markRead(n.id)
    const spaceId = 'spaceId' in n ? n.spaceId : undefined
    if (spaceId) setActiveSpace(spaceId)
    setOpen(false)
  }

  return (
    <div ref={ref} className="relative">
      <button
        onClick={() => setOpen(!open)}
        className="p-1.5 rounded-md hover:bg-white/10 transition-colors relative"
        title="Notifications"
      >
        <Bell size={13} style={{ color: 'var(--textMuted)' }} />
        {unreadCount > 0 && (
          <span
            className="absolute -top-0.5 -right-0.5 w-3.5 h-3.5 rounded-full flex items-center justify-center text-[8px] font-bold"
            style={{ background: 'var(--error)', color: '#fff' }}
          >
            {unreadCount > 9 ? '9+' : unreadCount}
          </span>
        )}
      </button>

      {open && (
        <div
          className="absolute right-0 top-full mt-1 rounded-lg shadow-2xl overflow-hidden z-50"
          style={{
            width: 300,
            maxHeight: 400,
            background: 'var(--bgSecondary)',
            border: '1px solid var(--border)',
          }}
        >
          <div
            className="flex items-center justify-between px-3 py-2 border-b"
            style={{ borderColor: 'var(--border)' }}
          >
            <span className="text-[11px] font-semibold" style={{ color: 'var(--text)' }}>
              Notifications
            </span>
            <div className="flex items-center gap-1">
              <button
                onClick={markAllRead}
                className="p-1 rounded hover:bg-white/10 transition-colors"
                title="Mark all read"
              >
                <Check size={11} style={{ color: 'var(--textDim)' }} />
              </button>
              <button
                onClick={clearAll}
                className="p-1 rounded hover:bg-white/10 transition-colors"
                title="Clear all"
              >
                <Trash2 size={11} style={{ color: 'var(--textDim)' }} />
              </button>
            </div>
          </div>

          <div className="overflow-y-auto" style={{ maxHeight: 350 }}>
            {notifications.length === 0 ? (
              <div className="p-4 text-center">
                <span className="text-[11px]" style={{ color: 'var(--textDim)' }}>
                  No notifications
                </span>
              </div>
            ) : (
              notifications.map((n) => (
                <button
                  key={n.id}
                  onClick={() => handleClick(n)}
                  className="w-full flex items-start gap-2.5 px-3 py-2 text-left transition-colors hover:bg-white/[0.03]"
                  style={{
                    background: n.read ? 'transparent' : 'rgba(14, 165, 233, 0.06)',
                    borderBottom: '1px solid var(--border)',
                  }}
                >
                  <div
                    className="w-2 h-2 rounded-full shrink-0 mt-1"
                    style={{
                      background: getAgentColor(
                        isEnhanced(n) ? (n.agentType ?? 'custom') : n.agentType,
                      ),
                    }}
                  />
                  <div className="flex-1 min-w-0">
                    <span
                      className="text-[11px] font-medium block truncate"
                      style={{ color: 'var(--text)' }}
                    >
                      {isEnhanced(n) ? n.title : n.paneName}
                    </span>
                    <span
                      className="text-[10px] block truncate"
                      style={{ color: 'var(--textDim)' }}
                    >
                      {n.message}
                    </span>
                  </div>
                  <span className="text-[9px] shrink-0 mt-0.5" style={{ color: 'var(--textDim)' }}>
                    {timeAgo(n.timestamp)}
                  </span>
                </button>
              ))
            )}
          </div>
        </div>
      )}
    </div>
  )
}
