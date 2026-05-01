export type PluginCapability =
  | 'notifications'
  | 'status'
  | 'tasks'
  | 'agent_control'
  | 'user_input'
  | 'file_access'
  | 'swarm'

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
  | 'agent_connected'
  | 'agent_disconnected'
  | 'plugin_registered'
  | 'plugin_error'
  | 'output_forwarded'

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
  level?: 'info' | 'warning' | 'error' | 'success'
  message?: string
  title?: string
  metadata?: Record<string, unknown>
  actions?: Array<{ id: string; label: string }>

  status?:
    | 'idle'
    | 'thinking'
    | 'working'
    | 'waiting_for_input'
    | 'completed'
    | 'error'
    | 'cancelled'
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

  entries?: Array<{ channel: 'stdout' | 'stderr'; text: string; timestamp?: number }>
  sessionId?: string

  agentId?: string
  name?: string
  agentType?: string
  pluginId?: string
  description?: string
  version?: string
  author?: string
  capabilities?: PluginCapability[]
  priority?: 'low' | 'normal' | 'high' | 'urgent'
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

export type AgentStatus =
  | 'idle'
  | 'thinking'
  | 'working'
  | 'waiting_for_input'
  | 'completed'
  | 'error'
  | 'cancelled'
  | 'disconnected'

export interface PerPaneAgentStatus {
  paneId: string
  status: AgentStatus
  message?: string
  progress?: { current: number; total: number; label: string }
  lastUpdatedAt: number
}

export const DEFAULT_CAPABILITIES: Record<string, PluginCapability[]> = {
  claude: ['notifications', 'status', 'tasks', 'user_input'],
  codex: ['notifications', 'status', 'tasks', 'user_input'],
  opencode: ['notifications', 'status', 'tasks', 'user_input'],
  gemini: ['notifications', 'status', 'tasks', 'user_input'],
  custom: ['notifications', 'status'],
  shell: ['notifications', 'status'],
}
