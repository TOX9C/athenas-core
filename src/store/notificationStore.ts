import { create } from 'zustand'
import type { AgentType } from '../types/workspace'

export interface Notification {
  id: string
  paneId: string
  paneName: string
  agentType: AgentType
  message: string
  timestamp: number
  read: boolean
  spaceId: string
}

interface NotificationState {
  notifications: Notification[]
  muted: boolean
  addNotification: (n: Notification) => void
  markRead: (id: string) => void
  markAllRead: () => void
  clearAll: () => void
  setMuted: (muted: boolean) => void
}

const MAX_NOTIFICATIONS = 50

export const useNotificationStore = create<NotificationState>((set) => ({
  notifications: [],
  muted: false,
  addNotification: (n) =>
    set((s) => ({
      notifications: [n, ...s.notifications].slice(0, MAX_NOTIFICATIONS),
    })),
  markRead: (id) =>
    set((s) => ({
      notifications: s.notifications.map((n) =>
        n.id === id ? { ...n, read: true } : n
      ),
    })),
  markAllRead: () =>
    set((s) => ({
      notifications: s.notifications.map((n) => ({ ...n, read: true })),
    })),
  clearAll: () => set({ notifications: [] }),
  setMuted: (muted) => set({ muted }),
}))
