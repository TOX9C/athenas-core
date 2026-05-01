import type { AgentType } from '../types/workspace'

export function getAgentCommand(
  agentType: AgentType,
  customCmd?: string,
  options?: { bypass?: boolean },
): string | undefined {
  switch (agentType) {
    case 'claude':
      return options?.bypass ? 'claude --dangerously-skip-permissions' : 'claude'
    case 'codex':
      return 'codex'
    case 'opencode':
      return 'opencode'
    case 'gemini':
      return 'gemini'
    case 'custom':
      return customCmd
    case 'shell':
      return undefined
  }
}

export function getAgentLabel(agentType: AgentType): string {
  switch (agentType) {
    case 'claude':
      return 'Claude Code'
    case 'codex':
      return 'Codex'
    case 'opencode':
      return 'OpenCode'
    case 'gemini':
      return 'Gemini CLI'
    case 'custom':
      return 'Custom'
    case 'shell':
      return 'Shell'
  }
}

export function getAgentColor(agentType: AgentType): string {
  switch (agentType) {
    case 'claude':
      return '#d97706'
    case 'codex':
      return '#10b981'
    case 'opencode':
      return '#3b82f6'
    case 'gemini':
      return '#0891b2'
    case 'custom':
      return '#6b7280'
    case 'shell':
      return '#64748b'
  }
}
