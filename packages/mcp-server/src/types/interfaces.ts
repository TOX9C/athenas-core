export type AgentStatus = 'running' | 'idle' | 'error' | 'waiting' | 'done' | 'blocked' | 'stalled'

export type SpecAgentStatus =
  | 'idle'
  | 'thinking'
  | 'working'
  | 'waiting_for_input'
  | 'completed'
  | 'error'
  | 'cancelled'

export type NotificationType = 'info' | 'warning' | 'error' | 'success'

export type NotificationPriority = 'low' | 'normal' | 'high' | 'critical'

export type PluginEventType =
  | 'notification'
  | 'status_update'
  | 'task_complete'
  | 'task_error'
  | 'needs_input'
  | 'agent_spawned'
  | 'agent_exited'
  | 'agent_stalled'
  | 'progress_update'
  | 'artifact_produced'
  | 'user_response'
  | 'control_command'

export type PluginCapability =
  | 'notifications'
  | 'status'
  | 'tasks'
  | 'agent_control'
  | 'user_input'
  | 'file_access'
  | 'swarm'

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

export interface PluginEvent {
  id: string
  type: PluginEventType
  source: {
    sessionId: string
    paneId: string | null
    agentType: string
    agentId: string | null
  }
  payload: PluginEventPayload
  timestamp: number
}

export interface PluginEventPayload {
  level?: NotificationType
  message?: string
  title?: string
  metadata?: Record<string, unknown>
  actions?: Array<{ id: string; label: string }>
  status?: SpecAgentStatus
  progress?: { current: number; total: number; label: string }
  artifacts?: Array<{ path: string; type: 'file' | 'url' | 'image' | 'log' }>
  taskId?: string
  taskTitle?: string
  result?: string
  error?: string
  prompt?: string
  options?: string[]
  requestId?: string
  response?: string
  responseType?: 'option' | 'freetext'
  exitCode?: number
  command?: 'pause' | 'resume' | 'cancel'
}

export interface McpSession {
  sessionId: string
  token: string
  paneId: string | null
  agentType: string
  capabilities: PluginCapability[]
  connectedAt: number
  lastActivityAt: number
}

export interface PluginManifest {
  id: string
  name: string
  version: string
  description: string
  author: string
  minAthenaVersion: string
  capabilities: PluginCapability[]
  tools: PluginToolDefinition[]
  subscribesTo?: PluginEventType[]
  config?: PluginConfigSchema
  install: PluginInstallMethod
}

export interface PluginToolDefinition {
  name: string
  description: string
  inputSchema: Record<string, unknown>
  capability: PluginCapability
  phase: 1 | 2 | 3
}

export interface PluginConfigSchema {
  schema: Record<string, unknown>
  defaults: Record<string, unknown>
}

export type PluginInstallMethod =
  | { type: 'builtin' }
  | { type: 'mcp_server'; command: string; args?: string[]; env?: Record<string, string> }
  | { type: 'hook'; script: string }

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
