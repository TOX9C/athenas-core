export type AgentStatus = 'running' | 'idle' | 'error' | 'waiting' | 'done' | 'blocked' | 'stalled'

export type NotificationType = 'info' | 'warning' | 'error' | 'success'

export type NotificationPriority = 'low' | 'normal' | 'high' | 'critical'

export interface AthenaNotification {
  type: NotificationType
  title: string
  message: string
  priority: NotificationPriority
  agentId?: string
  timestamp?: number
  metadata?: Record<string, unknown>
  actions?: Array<{ id: string; label: string }>
}

export interface InputRequest {
  prompt: string
  defaultResponse?: string
  timeout?: number
  agentId?: string
}

export interface InputResponse {
  value: string
  cancelled: boolean
  timedOut: boolean
}

export interface StatusUpdate {
  agentId: string
  status: AgentStatus
  message?: string
  progress?: number
  details?: Record<string, unknown>
}

export interface ErrorReport {
  agentId: string
  error: string
  stack?: string
  code?: string | number
  recoverable: boolean
  context?: Record<string, unknown>
}

export interface CompletionReport {
  agentId: string
  summary: string
  artifacts?: string[]
  metrics?: Record<string, number>
  duration?: number
}

export interface AgentState {
  id: string
  type: string
  role?: string
  status: AgentStatus
  cwd?: string
  pid?: number
  startedAt?: number
  lastActivityAt?: number
  message?: string
  progress?: number
}

export interface AthenaAppState {
  activeSpaceId: string | null
  spaces: SpaceState[]
  theme: string
  activePanel: string
  agents: AgentState[]
  tasks: TaskState[]
}

export interface SpaceState {
  id: string
  name: string
  cwd: string
  panes: PaneState[]
}

export interface PaneState {
  id: string
  agentType: string
  label: string
  status: AgentStatus
}

export interface TaskState {
  id: string
  title: string
  status: string
  description?: string
  spaceId?: string
}

export interface OutputEntry {
  timestamp: number
  lineNumber: number
  content: string
  isStderr: boolean
}

export interface OutputReadOptions {
  lines?: number
  sinceTimestamp?: number
}

export interface OutputSinceOptions {
  sinceTimestamp?: number
  sinceLine?: number
}

export interface OutputBufferConfig {
  maxLinesPerPane: number
}

export interface StreamSubscription {
  id: string
  paneId: string
  onChunk: (entry: OutputEntry) => void
  active: boolean
}

export interface AgentListEntry {
  paneId: string
  agentType: string
  status: AgentStatus
  label?: string
  lastActivityAt?: number
}

export type TransportType = 'stdio' | 'websocket' | 'tcp'

export interface ServerConfig {
  name: string
  version: string
  transport: TransportType
  websocketPort?: number
  tcpPort?: number
  athenaHost?: string
  athenaPort?: number
  authToken?: string
}
