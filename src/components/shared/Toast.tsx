import { useState, useEffect, useCallback } from 'react'
import { X, CheckCircle, AlertCircle, AlertTriangle, Info } from 'lucide-react'

type ToastType = 'success' | 'error' | 'warning' | 'info'

interface ToastItem {
  id: string
  message: string
  type: ToastType
  duration?: number
}

let toastListeners: ((toast: ToastItem) => void)[] = []

export function showToast(message: string, type: ToastType = 'info', duration = 5000) {
  const toast: ToastItem = {
    id: Math.random().toString(36).slice(2),
    message,
    type,
    duration,
  }
  toastListeners.forEach((fn) => fn(toast))
}

const ICONS = {
  success: CheckCircle,
  error: AlertCircle,
  warning: AlertTriangle,
  info: Info,
}

const COLORS = {
  success: 'var(--success)',
  error: 'var(--error)',
  warning: 'var(--warning)',
  info: 'var(--accent)',
}

export function Toast({ toast, onDismiss }: { toast: ToastItem; onDismiss: () => void }) {
  const Icon = ICONS[toast.type]

  useEffect(() => {
    if (toast.duration) {
      const timer = setTimeout(onDismiss, toast.duration)
      return () => clearTimeout(timer)
    }
  }, [toast.duration, onDismiss])

  return (
    <div
      className="flex items-center gap-2 px-3 py-2 rounded-lg shadow-lg text-xs max-w-[360px]"
      style={{
        background: 'var(--bgSecondary)',
        border: '1px solid var(--border)',
        color: 'var(--text)',
        animation: 'slideUp 200ms ease-out',
      }}
    >
      <Icon size={14} style={{ color: COLORS[toast.type], flexShrink: 0 }} />
      <span className="flex-1 min-w-0">{toast.message}</span>
      <button onClick={onDismiss} className="p-0.5 rounded hover:bg-white/10 shrink-0">
        <X size={12} style={{ color: 'var(--textDim)' }} />
      </button>
    </div>
  )
}

export function ToastContainer() {
  const [toasts, setToasts] = useState<ToastItem[]>([])

  useEffect(() => {
    const handler = (toast: ToastItem) => {
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
    <div className="fixed bottom-4 right-4 z-[100] flex flex-col gap-2">
      <style>{`@keyframes slideUp { from { opacity: 0; transform: translateY(8px); } to { opacity: 1; transform: translateY(0); } }`}</style>
      {toasts.map((toast) => (
        <Toast key={toast.id} toast={toast} onDismiss={() => dismiss(toast.id)} />
      ))}
    </div>
  )
}
