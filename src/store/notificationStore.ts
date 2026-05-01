import { create } from 'zustand'
import type { AgentType } from '../types/workspace'
import type {
  NotificationType,
  NotificationPriority,
  NotificationAction,
  Plugin,
  PluginStatus,
  AgentStatusEntry,
} from '../types/notification'

export interface LegacyNotification {
  id: string
  paneId: string
  paneName: string
  agentType: AgentType
  message: string
  timestamp: number
  read: boolean
  spaceId: string
}

export interface EnhancedNotification {
  id: string
  type: NotificationType
  priority: NotificationPriority
  title: string
  message: string
  timestamp: number
  read: boolean
  dismissed: boolean
  source: string
  agentType?: AgentType
  spaceId?: string
  paneId?: string
  inputRequestId?: string
  inputRequestPrompt?: string
  inputRequestOptions?: string[]
  inputResponding?: boolean
  inputResponse?: string
  actions?: NotificationAction[]
}

type NotificationItem = LegacyNotification | EnhancedNotification

function isEnhanced(n: NotificationItem): n is EnhancedNotification {
  return 'type' in n && 'source' in n
}

interface NotificationState {
  notifications: NotificationItem[]
  muted: boolean
  panelOpen: boolean
  addNotification: (n: NotificationItem) => void
  addEnhancedNotification: (n: EnhancedNotification) => void
  markRead: (id: string) => void
  markAllRead: () => void
  dismiss: (id: string) => void
  clearAll: () => void
  setMuted: (muted: boolean) => void
  setPanelOpen: (open: boolean) => void
  togglePanel: () => void
  respondToInput: (notificationId: string, response: string) => void
  setInputResponding: (notificationId: string, responding: boolean) => void
  unreadCount: () => number
  pendingInputRequests: () => EnhancedNotification[]
}

interface PluginState {
  plugins: Plugin[]
  setPlugins: (plugins: Plugin[]) => void
  addPlugin: (plugin: Plugin) => void
  removePlugin: (id: string) => void
  updatePlugin: (id: string, updates: Partial<Plugin>) => void
  togglePlugin: (id: string) => void
  setPluginStatus: (id: string, status: PluginStatus) => void
}

interface AgentStatusState {
  agentStatuses: AgentStatusEntry[]
  setAgentStatuses: (statuses: AgentStatusEntry[]) => void
  addAgentStatusEntry: (entry: AgentStatusEntry) => void
  updateAgentStatus: (id: string, updates: Partial<AgentStatusEntry>) => void
  removeAgentStatus: (id: string) => void
  connectedAgentCount: () => number
}

const MAX_NOTIFICATIONS = 100

export const useNotificationStore = create<NotificationState & PluginState & AgentStatusState>(
  (set, get) => ({
    notifications: [],
    muted: false,
    panelOpen: false,

    addNotification: (n) =>
      set((s) => ({
        notifications: [n, ...s.notifications].slice(0, MAX_NOTIFICATIONS),
      })),

    addEnhancedNotification: (n) =>
      set((s) => ({
        notifications: [n, ...s.notifications].slice(0, MAX_NOTIFICATIONS),
      })),

    markRead: (id) =>
      set((s) => ({
        notifications: s.notifications.map((n) => (n.id === id ? { ...n, read: true } : n)),
      })),

    markAllRead: () =>
      set((s) => ({
        notifications: s.notifications.map((n) => ({ ...n, read: true })),
      })),

    dismiss: (id) =>
      set((s) => ({
        notifications: s.notifications.map((n) =>
          isEnhanced(n) && n.id === id ? { ...n, dismissed: true } : n,
        ),
      })),

    clearAll: () => set({ notifications: [] }),
    setMuted: (muted) => set({ muted }),
    setPanelOpen: (open) => set({ panelOpen: open }),
    togglePanel: () => set((s) => ({ panelOpen: !s.panelOpen })),

    respondToInput: (notificationId, response) =>
      set((s) => ({
        notifications: s.notifications.map((n) =>
          isEnhanced(n) && n.id === notificationId
            ? { ...n, inputResponse: response, inputResponding: false, read: true }
            : n,
        ),
      })),

    setInputResponding: (notificationId, responding) =>
      set((s) => ({
        notifications: s.notifications.map((n) =>
          isEnhanced(n) && n.id === notificationId ? { ...n, inputResponding: responding } : n,
        ),
      })),

    unreadCount: () => get().notifications.filter((n) => !n.read).length,

    pendingInputRequests: () =>
      get().notifications.filter(
        (n): n is EnhancedNotification =>
          isEnhanced(n) && n.type === 'needs_input' && !n.dismissed && !n.inputResponse,
      ),

    plugins: [],
    setPlugins: (plugins) => set({ plugins }),
    addPlugin: (plugin) => set((s) => ({ plugins: [...s.plugins, plugin] })),
    removePlugin: (id) => set((s) => ({ plugins: s.plugins.filter((p) => p.id !== id) })),
    updatePlugin: (id, updates) =>
      set((s) => ({
        plugins: s.plugins.map((p) => (p.id === id ? { ...p, ...updates } : p)),
      })),
    togglePlugin: (id) =>
      set((s) => ({
        plugins: s.plugins.map((p) =>
          p.id === id
            ? { ...p, enabled: !p.enabled, status: !p.enabled ? 'active' : 'inactive' }
            : p,
        ),
      })),
    setPluginStatus: (id, status) =>
      set((s) => ({
        plugins: s.plugins.map((p) => (p.id === id ? { ...p, status } : p)),
      })),

    agentStatuses: [],
    setAgentStatuses: (statuses) => set({ agentStatuses: statuses }),
    addAgentStatusEntry: (entry) =>
      set((s) => {
        if (s.agentStatuses.find((a) => a.id === entry.id)) return s
        return { agentStatuses: [...s.agentStatuses, entry] }
      }),
    updateAgentStatus: (id, updates) =>
      set((s) => ({
        agentStatuses: s.agentStatuses.map((a) => (a.id === id ? { ...a, ...updates } : a)),
      })),
    removeAgentStatus: (id) =>
      set((s) => ({ agentStatuses: s.agentStatuses.filter((a) => a.id !== id) })),
    connectedAgentCount: () =>
      get().agentStatuses.filter((a) => a.status !== 'disconnected').length,
  }),
)

export { isEnhanced }
