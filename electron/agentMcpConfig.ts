import { getMcpToken } from './mcpServer'
import { getCommsToken } from './services/agent-comms'
import type { AgentType } from '../src/types/workspace'
import { DEFAULT_CAPABILITIES } from '../src/types/plugin'
import { MCP_PORT, MCP_HOST, COMMS_PORT, PLUGIN_IDS } from '../plugins/shared/types'
import * as path from 'path'

interface McpEnvConfig {
  env: Record<string, string>
  mcpJson?: Record<string, unknown>
}

const PROXY_PATH = path.resolve(__dirname, '../../bin/mcp-proxy.js')

export function buildMcpEnv(
  agentType: AgentType,
  paneId?: string,
  sessionId?: string,
): McpEnvConfig {
  const mcpToken = getMcpToken()
  const commsToken = getCommsToken()

  const env: Record<string, string> = {
    ATHENA_MCP_TOKEN: mcpToken,
    ATHENA_MCP_PORT: String(MCP_PORT),
    ATHENA_MCP_HOST: MCP_HOST,
    ATHENA_COMMS_TOKEN: commsToken,
    ATHENA_COMMS_PORT: String(COMMS_PORT),
  }

  if (paneId) {
    env.ATHENA_PANE_ID = paneId
  }
  if (sessionId) {
    env.ATHENA_SESSION_ID = sessionId
  }

  const mcpEntry = {
    command: 'node',
    args: [PROXY_PATH],
    env: {
      ATHENA_MCP_TOKEN: mcpToken,
      ATHENA_MCP_PORT: String(MCP_PORT),
      ATHENA_MCP_HOST: MCP_HOST,
    },
  }

  const mcpJson: Record<string, unknown> = {
    mcpServers: {
      athena: mcpEntry,
    },
  }

  if (agentType === 'claude') {
    env.CLAUDE_MCP_SERVERS = JSON.stringify(mcpJson)
  } else if (agentType === 'opencode') {
    env.OPENCODE_MCP_SERVERS = JSON.stringify(mcpJson)
  }

  return { env, mcpJson }
}

export function buildSpawnPrefix(
  agentType: AgentType,
  paneId?: string,
  sessionId?: string,
): string {
  const { env, mcpJson } = buildMcpEnv(agentType, paneId, sessionId)

  const exports: string[] = []
  for (const [key, value] of Object.entries(env)) {
    exports.push(`export ${key}='${value.replace(/'/g, "'\\''")}'`)
  }

  return exports.join('; ') + '; '
}

export function getPluginIdForAgent(agentType: AgentType): string | null {
  switch (agentType) {
    case 'claude':
      return PLUGIN_IDS['claude-code']
    case 'opencode':
      return PLUGIN_IDS['opencode']
    default:
      return null
  }
}

export function getCapabilitiesForAgent(agentType: AgentType): string[] {
  return DEFAULT_CAPABILITIES[agentType] || ['notifications', 'status']
}
