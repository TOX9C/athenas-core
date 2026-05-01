export type {
  PluginEventType,
  PluginCapability,
  PluginEvent,
  PluginEventPayload,
  PluginManifest,
  PluginToolDefinition,
  PluginConfigSchema,
  PluginInstallMethod,
} from '../../src/types/plugin'

export type { AgentType } from '../../src/types/workspace'

export { DEFAULT_CAPABILITIES } from '../../src/types/plugin'

export interface PluginDiscoveryResult {
  agentType: 'opencode' | 'claude-code'
  installed: boolean
  configPath: string | null
  configExists: boolean
  mcpEntryExists: boolean
  binaryPath: string | null
}

export interface PluginSetupOptions {
  token: string
  port?: number
  host?: string
  sessionId?: string
  projectRoot?: string
  global?: boolean
}

export interface PluginSetupResult {
  success: boolean
  configPath: string
  created: boolean
  updated: boolean
  error?: string
}

export const MCP_PORT = 4545
export const MCP_HOST = '127.0.0.1'
export const COMMS_PORT = 4546

export const PLUGIN_IDS = {
  opencode: 'athena-opencode-plugin',
  'claude-code': 'athena-claude-code-plugin',
} as const

export function buildProxyCommand(proxyPath: string): { command: string; args: string[] } {
  return { command: 'node', args: [proxyPath] }
}

export type OutputChannel = 'stdout' | 'stderr'

export interface OutputEntry {
  channel: OutputChannel
  text: string
  timestamp: number
  sessionId?: string
}

export interface OutputBatch {
  entries: OutputEntry[]
  sessionId?: string
}

export interface OutputForwarderConfig {
  token: string
  port?: number
  host?: string
  sessionId?: string
  autoForwardOutput?: boolean
  batchIntervalMs?: number
  batchMaxLines?: number
  bufferOnReconnect?: boolean
}
