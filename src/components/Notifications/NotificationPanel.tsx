import { useState, useRef, useEffect } from 'react'
import {
  X,
  Check,
  Trash2,
  Bell,
  Filter,
  MessageSquare,
  AlertTriangle,
  AlertCircle,
  CheckCircle,
  Info,
} from 'lucide-react'
import { useNotificationStore, isEnhanced } from '../../store/notificationStore'
import { useWorkspaceStore } from '../../store/workspaceStore'
import { getAgentColor, getAgentLabel } from '../../utils/agentCommands'
import type { EnhancedNotification } from '../../store/notificationStore'
import type { LegacyNotification } from '../../store/notificationStore'
import type { NotificationType } from '../../types/notification'

type FilterTab = 'all' | NotificationType

const TYPE_ICONS: Record<string, typeof Info> = {
  info: Info,
  warning: AlertTriangle,
  error: AlertCircle,
  success: CheckCircle,
  needs_input: MessageSquare,
  task_complete: CheckCircle,
  task_error: AlertCircle,
}

const TYPE_COLORS: Record<string, string> = {
  info: 'var(--accent)',
  warning: 'var(--warning)',
  error: 'var(--error)',
  success: 'var(--success)',
  needs_input: '#f97316',
  task_complete: 'var(--success)',
  task_error: 'var(--error)',
}

const TYPE_BGS: Record<string, string> = {
  info: 'color-mix(in srgb, var(--accent) 4%, transparent)',
  warning: 'color-mix(in srgb, var(--warning) 4%, transparent)',
  error: 'color-mix(in srgb, var(--error) 4%, transparent)',
  success: 'color-mix(in srgb, var(--success) 4%, transparent)',
  needs_input: 'rgba(249,115,22,0.04)',
  task_complete: 'color-mix(in srgb, var(--success) 4%, transparent)',
  task_error: 'color-mix(in srgb, var(--error) 4%, transparent)',
}

function timeAgo(ts: number): string {
  const diff = Math.floor((Date.now() - ts) / 1000)
  if (diff < 60) return 'just now'
  if (diff < 3600) return `${Math.floor(diff / 60)}m ago`
  if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`
  return `${Math.floor(diff / 86400)}d ago`
}

function NotificationItem({
  notification,
  onDismiss,
  onClick,
}: {
  notification: EnhancedNotification | LegacyNotification
  onDismiss: (id: string) => void
  onClick: (n: typeof notification) => void
}) {
  const enhanced = isEnhanced(notification)
  const type = enhanced ? notification.type : 'info'
  const Icon = TYPE_ICONS[type] ?? Info
  const color = TYPE_COLORS[type] ?? 'var(--accent)'
  const bg = TYPE_BGS[type] ?? TYPE_BGS.info
  const title = enhanced ? notification.title : (notification as LegacyNotification).paneName
  const message = notification.message
  const agentType = enhanced
    ? notification.agentType
    : (notification as LegacyNotification).agentType
  const isInputRequest =
    enhanced && notification.type === 'needs_input' && !notification.inputResponse

  return (
    <div
      className="flex items-start gap-2.5 px-3 py-2.5 text-left transition-colors hover:bg-white/[0.03] cursor-pointer group"
      style={{
        background: notification.read ? 'transparent' : bg,
        borderBottom: '1px solid var(--border)',
        borderLeft: enhanced && !notification.read ? `2px solid ${color}` : '2px solid transparent',
      }}
      onClick={() => onClick(notification)}
    >
      <Icon size={13} style={{ color, flexShrink: 0, marginTop: 2 }} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 mb-0.5">
          {agentType && (
            <span
              className="text-[9px] font-bold px-1 py-px rounded"
              style={{
                background: `${getAgentColor(agentType)}22`,
                color: getAgentColor(agentType),
              }}
            >
              {getAgentLabel(agentType)}
            </span>
          )}
          {title && (
            <span className="text-[11px] font-semibold truncate" style={{ color: 'var(--text)' }}>
              {title}
            </span>
          )}
          {isInputRequest && (
            <span
              className="text-[9px] font-bold px-1 py-px rounded animate-pulse"
              style={{ background: '#f97316', color: '#fff' }}
            >
              ACTION NEEDED
            </span>
          )}
        </div>
        <span className="text-[10px] block leading-tight" style={{ color: 'var(--textDim)' }}>
          {message}
        </span>
        {enhanced && notification.inputResponse && (
          <span className="text-[9px] block mt-1 italic" style={{ color: 'var(--success)' }}>
            Responded: {notification.inputResponse}
          </span>
        )}
      </div>
      <div className="flex flex-col items-end gap-1 shrink-0">
        <span className="text-[9px]" style={{ color: 'var(--textDim)' }}>
          {timeAgo(notification.timestamp)}
        </span>
        <button
          onClick={(e) => {
            e.stopPropagation()
            onDismiss(notification.id)
          }}
          className="p-0.5 rounded hover:bg-white/10 opacity-0 group-hover:opacity-100 transition-opacity"
        >
          <X size={10} style={{ color: 'var(--textDim)' }} />
        </button>
      </div>
    </div>
  )
}

const FILTER_TABS: { key: FilterTab; label: string; icon: typeof Info }[] = [
  { key: 'all', label: 'All', icon: Bell },
  { key: 'needs_input', label: 'Input', icon: MessageSquare },
  { key: 'error', label: 'Errors', icon: AlertCircle },
  { key: 'warning', label: 'Warnings', icon: AlertTriangle },
  { key: 'success', label: 'Success', icon: CheckCircle },
  { key: 'task_complete', label: 'Done', icon: CheckCircle },
]

export function NotificationPanel() {
  const { notifications, markRead, markAllRead, dismiss, clearAll, panelOpen, setPanelOpen } =
    useNotificationStore()
  const { setActiveSpace } = useWorkspaceStore()
  const [filter, setFilter] = useState<FilterTab>('all')
  const ref = useRef<HTMLDivElement>(null)

  const unreadCount = notifications.filter((n) => !n.read).length

  useEffect(() => {
    const handler = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setPanelOpen(false)
      }
    }
    if (panelOpen) document.addEventListener('mousedown', handler)
    return () => document.removeEventListener('mousedown', handler)
  }, [panelOpen, setPanelOpen])

  const filtered = notifications.filter((n) => {
    if (filter === 'all') return true
    if (isEnhanced(n)) return n.type === filter
    return filter === 'info'
  })

  const handleClick = (n: (typeof notifications)[0]) => {
    markRead(n.id)
    const spaceId = isEnhanced(n) ? n.spaceId : (n as LegacyNotification).spaceId
    if (spaceId) setActiveSpace(spaceId)
  }

  return (
    <>
      <button
        onClick={() => setPanelOpen(!panelOpen)}
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

      {panelOpen && (
        <div
          ref={ref}
          className="absolute right-0 top-full mt-1 rounded-lg shadow-2xl overflow-hidden z-50"
          style={{
            width: 380,
            maxHeight: 500,
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

          <div
            className="flex items-center gap-0.5 px-2 py-1.5 border-b overflow-x-auto"
            style={{ borderColor: 'var(--border)' }}
          >
            {FILTER_TABS.map((tab) => {
              const count =
                tab.key === 'all'
                  ? notifications.length
                  : notifications.filter((n) => isEnhanced(n) && n.type === tab.key).length
              if (tab.key !== 'all' && count === 0) return null
              return (
                <button
                  key={tab.key}
                  onClick={() => setFilter(tab.key)}
                  className="flex items-center gap-1 px-2 py-1 rounded text-[10px] font-medium transition-colors whitespace-nowrap"
                  style={{
                    background: filter === tab.key ? 'var(--accent)' : 'transparent',
                    color: filter === tab.key ? '#fff' : 'var(--textMuted)',
                  }}
                >
                  <tab.icon size={10} />
                  {tab.label}
                  {count > 0 && (
                    <span
                      className="text-[8px] px-1 rounded-full"
                      style={{
                        background:
                          filter === tab.key ? 'rgba(255,255,255,0.2)' : 'var(--bgTertiary)',
                      }}
                    >
                      {count}
                    </span>
                  )}
                </button>
              )
            })}
          </div>

          <div className="overflow-y-auto" style={{ maxHeight: 390 }}>
            {filtered.length === 0 ? (
              <div className="p-6 text-center">
                <Bell size={20} style={{ color: 'var(--textDim)', margin: '0 auto 8px' }} />
                <span className="text-[11px]" style={{ color: 'var(--textDim)' }}>
                  No notifications
                </span>
              </div>
            ) : (
              filtered.map((n) => (
                <NotificationItem
                  key={n.id}
                  notification={n as EnhancedNotification | LegacyNotification}
                  onDismiss={dismiss}
                  onClick={handleClick}
                />
              ))
            )}
          </div>
        </div>
      )}
    </>
  )
}
