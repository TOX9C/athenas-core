import { useState, useEffect, useCallback } from 'react'
import { X, CheckCircle, AlertCircle, AlertTriangle, Info, MessageSquare } from 'lucide-react'
import { getAgentColor, getAgentLabel } from '../../utils/agentCommands'
import type { AgentType } from '../../types/workspace'
import type { NotificationType } from '../../types/notification'

interface NotificationToastItem {
  id: string
  type: NotificationType
  message: string
  title?: string
  agentType?: AgentType
  duration?: number
}

let toastListeners: ((toast: NotificationToastItem) => void)[] = []

export function showNotificationToast(item: Omit<NotificationToastItem, 'id'>) {
  const toast: NotificationToastItem = {
    id: Math.random().toString(36).slice(2),
    ...item,
  }
  toastListeners.forEach((fn) => fn(toast))
}

const TYPE_CONFIG: Record<
  NotificationType,
  { icon: typeof CheckCircle; color: string; border: string; bg: string }
> = {
  info: {
    icon: Info,
    color: 'var(--accent)',
    border: 'color-mix(in srgb, var(--accent) 27%, transparent)',
    bg: 'color-mix(in srgb, var(--accent) 4%, transparent)',
  },
  warning: {
    icon: AlertTriangle,
    color: 'var(--warning)',
    border: 'color-mix(in srgb, var(--warning) 27%, transparent)',
    bg: 'color-mix(in srgb, var(--warning) 4%, transparent)',
  },
  error: {
    icon: AlertCircle,
    color: 'var(--error)',
    border: 'color-mix(in srgb, var(--error) 27%, transparent)',
    bg: 'color-mix(in srgb, var(--error) 4%, transparent)',
  },
  success: {
    icon: CheckCircle,
    color: 'var(--success)',
    border: 'color-mix(in srgb, var(--success) 27%, transparent)',
    bg: 'color-mix(in srgb, var(--success) 4%, transparent)',
  },
  needs_input: {
    icon: MessageSquare,
    color: '#f97316',
    border: 'rgba(249,115,22,0.27)',
    bg: 'rgba(249,115,22,0.04)',
  },
  task_complete: {
    icon: CheckCircle,
    color: 'var(--success)',
    border: 'color-mix(in srgb, var(--success) 27%, transparent)',
    bg: 'color-mix(in srgb, var(--success) 4%, transparent)',
  },
  task_error: {
    icon: AlertCircle,
    color: 'var(--error)',
    border: 'color-mix(in srgb, var(--error) 27%, transparent)',
    bg: 'color-mix(in srgb, var(--error) 4%, transparent)',
  },
}

export function NotificationToast({
  toast,
  onDismiss,
}: {
  toast: NotificationToastItem
  onDismiss: () => void
}) {
  const config = TYPE_CONFIG[toast.type]
  const Icon = config.icon

  useEffect(() => {
    const d = toast.duration ?? (toast.type === 'needs_input' ? 0 : 6000)
    if (d > 0) {
      const timer = setTimeout(onDismiss, d)
      return () => clearTimeout(timer)
    }
  }, [toast.duration, toast.type, onDismiss])

  return (
    <div
      className="flex items-start gap-2.5 px-3 py-2.5 rounded-lg shadow-lg max-w-[380px]"
      style={{
        background: 'var(--bgSecondary)',
        border: `1px solid ${config.border}`,
        borderLeft: `3px solid ${config.color}`,
        color: 'var(--text)',
        animation: 'slideInRight 250ms ease-out',
      }}
    >
      <Icon size={14} style={{ color: config.color, flexShrink: 0, marginTop: 2 }} />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-1.5 mb-0.5">
          {toast.agentType && (
            <span
              className="text-[9px] font-bold px-1 py-px rounded"
              style={{
                background: `${getAgentColor(toast.agentType)}22`,
                color: getAgentColor(toast.agentType),
              }}
            >
              {getAgentLabel(toast.agentType)}
            </span>
          )}
          {toast.title && <span className="text-[11px] font-semibold truncate">{toast.title}</span>}
        </div>
        <span className="text-[11px] block leading-tight" style={{ color: 'var(--textMuted)' }}>
          {toast.message}
        </span>
      </div>
      <button onClick={onDismiss} className="p-0.5 rounded hover:bg-white/10 shrink-0 mt-0.5">
        <X size={12} style={{ color: 'var(--textDim)' }} />
      </button>
    </div>
  )
}

export function NotificationToastContainer() {
  const [toasts, setToasts] = useState<NotificationToastItem[]>([])

  useEffect(() => {
    const handler = (toast: NotificationToastItem) => {
      setToasts((prev) => [...prev, toast])
    }
    toastListeners.push(handler)
    return () => {
      toastListeners = toastListeners.filter((fn) => fn !== handler)
    }
  }, [])

  const dismiss = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id))
  }, [])

  if (toasts.length === 0) return null

  return (
    <div className="fixed top-12 right-4 z-[100] flex flex-col gap-2 pointer-events-none">
      <style>{`
        @keyframes slideInRight {
          from { opacity: 0; transform: translateX(16px); }
          to { opacity: 1; transform: translateX(0); }
        }
      `}</style>
      {toasts.map((toast) => (
        <div key={toast.id} className="pointer-events-auto">
          <NotificationToast toast={toast} onDismiss={() => dismiss(toast.id)} />
        </div>
      ))}
    </div>
  )
}
