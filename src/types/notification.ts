import type { AgentType } from './workspace'
import type { AgentStatus } from './plugin'

export type NotificationType =
  | 'info'
  | 'warning'
  | 'error'
  | 'success'
  | 'needs_input'
  | 'task_complete'
  | 'task_error'

export type NotificationPriority = 'low' | 'normal' | 'high' | 'urgent'

export interface NotificationInputRequest {
  requestId: string
  prompt: string
  options?: string[]
  responding: boolean
  response?: string
}

export interface Notification {
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
  inputRequest?: NotificationInputRequest
  actions?: NotificationAction[]
}

export interface NotificationAction {
  id: string
  label: string
  style: 'primary' | 'secondary' | 'danger'
}

export interface AgentStatusEntry {
  id: string
  name: string
  agentType: AgentType
  status: AgentStatus
  paneId?: string
  spaceId?: string
  lastAction: string
  lastActionAt: number
  connectedAt: number
  progress?: { current: number; total: number; label: string }
}

export type PluginStatus = 'active' | 'inactive' | 'error' | 'installing' | 'updating'

export interface Plugin {
  id: string
  name: string
  description: string
  version: string
  author: string
  status: PluginStatus
  enabled: boolean
  icon?: string
  config?: Record<string, unknown>
  installedAt: number
  updatedAt: number
  error?: string
  agentCount: number
  capabilities: string[]
}
