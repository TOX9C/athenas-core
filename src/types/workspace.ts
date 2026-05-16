export type AgentType = 'claude' | 'codex' | 'opencode' | 'gemini' | 'custom' | 'shell'

export type GridTemplate = '1x1' | '1x2' | '2x2' | '2x3' | '3x3' | '3x4' | '4x4'

export interface PaneConfig {
  id: string
  agentType: AgentType
  customCmd?: string
  customAgentId?: string
  label?: string
  bypassMode?: boolean
  /** Name of the project/workspace associated with this pane */
  projectName?: string
  /** Name of the model currently used by this pane's agent */
  modelName?: string
}

export interface Space {
  id: string
  name: string
  dir: string
  grid: GridTemplate
  panes: PaneConfig[]
  color: string
  createdAt: number
  lastOpenedAt: number
}
