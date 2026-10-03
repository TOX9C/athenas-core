import * as fs from 'fs'
import * as path from 'path'
import * as os from 'os'
import * as net from 'net'
import type { PluginDiscoveryResult, PluginSetupOptions, PluginSetupResult } from './types'
import { MCP_PORT, MCP_HOST, buildProxyCommand } from './types'

export { McpConnection, createMcpConnection } from './connection'

type AgentType = PluginDiscoveryResult['agentType']

interface AgentProfile {
  agentType: AgentType
  displayName: string
  binaries: string[]
  configDir: string
}

export const AGENT_PROFILES: Record<AgentType, AgentProfile> = {
  opencode: {
    agentType: 'opencode',
    displayName: 'OpenCode',
    binaries: ['opencode'],
    configDir: '.opencode',
  },
  'claude-code': {
    agentType: 'claude-code',
    displayName: 'Claude Code',
    binaries: ['claude'],
    configDir: '.claude',
  },
}

const PROXY_RELATIVE = '../../bin/mcp-proxy.js'

function resolveProxyPath(): string {
  return path.resolve(__dirname, PROXY_RELATIVE)
}

function findBinary(names: string[]): string | null {
  const pathEnv = process.env.PATH || ''
  const dirs = pathEnv.split(path.delimiter)
  for (const dir of dirs) {
    for (const name of names) {
      const full = path.join(dir, name)
      try {
        fs.accessSync(full, fs.constants.X_OK)
        return full
      } catch {}
    }
  }
  return null
}

export function discoverAgent(agentType: AgentType, projectRoot?: string): PluginDiscoveryResult {
  const profile = AGENT_PROFILES[agentType]
  const configPaths: string[] = []
  if (projectRoot) configPaths.push(path.join(projectRoot, profile.configDir, 'mcp.json'))
  configPaths.push(path.join(os.homedir(), profile.configDir, 'mcp.json'))

  let configPath: string | null = null
  let configExists = false
  let mcpEntryExists = false

  for (const p of configPaths) {
    if (fs.existsSync(p)) {
      configPath = p
      configExists = true
      try {
        const cfg = JSON.parse(fs.readFileSync(p, 'utf8'))
        mcpEntryExists = !!cfg?.mcpServers?.athena
      } catch {}
      break
    }
  }

  if (!configPath) {
    configPath = configPaths[0]
  }

  const binaryPath = findBinary(profile.binaries)
  return {
    agentType,
    installed: !!binaryPath,
    configPath,
    configExists,
    mcpEntryExists,
    binaryPath,
  }
}

export function discoverOpenCode(projectRoot?: string): PluginDiscoveryResult {
  return discoverAgent('opencode', projectRoot)
}

export function discoverClaudeCode(projectRoot?: string): PluginDiscoveryResult {
  return discoverAgent('claude-code', projectRoot)
}

export function discoverAll(projectRoot?: string): PluginDiscoveryResult[] {
  return [discoverOpenCode(projectRoot), discoverClaudeCode(projectRoot)]
}

export function setupAgent(agentType: AgentType, options: PluginSetupOptions): PluginSetupResult {
  const profile = AGENT_PROFILES[agentType]
  const projectScope = !!options.projectRoot && !options.global

  // When configuring a project, the auth token is written ONLY to the
  // user-level config (~/.<agent>/mcp.json); agents merge user- and
  // project-level MCP server entries, so the project file never carries
  // the secret and is safe to leave untracked.
  if (projectScope) {
    const globalResult = upsertMcpEntry(discoverAgent(agentType), options, true)
    if (!globalResult.success) {
      return globalResult
    }
  }

  const discovery = discoverAgent(agentType, options.projectRoot)
  const result = upsertMcpEntry(discovery, options, !projectScope)
  if (result.success && projectScope && options.projectRoot) {
    ensureGitignoreEntry(options.projectRoot, path.join(profile.configDir, 'mcp.json'))
  }
  return result
}

export function setupOpenCode(options: PluginSetupOptions): PluginSetupResult {
  return setupAgent('opencode', options)
}

export function setupClaudeCode(options: PluginSetupOptions): PluginSetupResult {
  return setupAgent('claude-code', options)
}

function buildMcpEntry(
  options: PluginSetupOptions,
  includeToken: boolean,
): Record<string, unknown> {
  const proxyPath = resolveProxyPath()
  const { command, args } = buildProxyCommand(proxyPath)

  const env: Record<string, string> = {
    ATHENA_MCP_TCP_PORT: String(options.port || MCP_PORT),
    ATHENA_MCP_HOST: options.host || MCP_HOST,
  }
  if (includeToken) {
    env.ATHENA_MCP_TOKEN = options.token
  }
  if (options.sessionId) {
    env.ATHENA_SESSION_ID = options.sessionId
  }

  return { command, args, env }
}

function readExistingConfig(
  configPath: string | null,
  configExists: boolean,
): { value: Record<string, unknown> } | { error: string } {
  if (!configExists || !configPath) {
    return { value: {} }
  }
  try {
    return { value: JSON.parse(fs.readFileSync(configPath, 'utf8')) }
  } catch (err) {
    // Refuse to overwrite an unreadable config: doing so would silently
    // delete every other MCP server the user configured.
    return {
      error: `Existing MCP config at ${configPath} is not valid JSON; refusing to overwrite it (${err instanceof Error ? err.message : String(err)})`,
    }
  }
}

function writeConfigAtomic(configPath: string, config: Record<string, unknown>): void {
  const dir = path.dirname(configPath)
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true })
  }
  const tmpPath = configPath + '.tmp'
  fs.writeFileSync(tmpPath, JSON.stringify(config, null, 2) + '\n')
  fs.renameSync(tmpPath, configPath)
}

function upsertMcpEntry(
  discovery: PluginDiscoveryResult,
  options: PluginSetupOptions,
  includeToken: boolean,
): PluginSetupResult {
  const existing = readExistingConfig(discovery.configPath, discovery.configExists)
  if ('error' in existing) {
    return {
      success: false,
      configPath: discovery.configPath || '',
      created: false,
      updated: false,
      error: existing.error,
    }
  }

  const config = existing.value
  const rawServers = config.mcpServers
  if (
    rawServers !== undefined &&
    (typeof rawServers !== 'object' || rawServers === null || Array.isArray(rawServers))
  ) {
    return {
      success: false,
      configPath: discovery.configPath || '',
      created: false,
      updated: false,
      error: `Existing MCP config at ${discovery.configPath} has a non-object "mcpServers" key; refusing to overwrite it`,
    }
  }
  const servers = (rawServers ?? {}) as Record<string, unknown>
  const wasUpdated = 'athena' in servers
  servers.athena = buildMcpEntry(options, includeToken)
  config.mcpServers = servers

  writeConfigAtomic(discovery.configPath!, config)

  return {
    success: true,
    configPath: discovery.configPath!,
    created: !discovery.configExists,
    updated: wasUpdated,
  }
}

/// If a `.gitignore` exists (or the repo has a `.git`), keep the MCP config
/// (which may hold an auth token in older installs) out of version control.
function ensureGitignoreEntry(projectRoot: string, relConfigPath: string): void {
  try {
    const gitignorePath = path.join(projectRoot, '.gitignore')
    let content = fs.existsSync(gitignorePath) ? fs.readFileSync(gitignorePath, 'utf8') : ''
    const covered = content
      .split('\n')
      .some((line) => line.trim() === relConfigPath || line.trim() === '/' + relConfigPath)
    if (covered) return
    if (content && !content.endsWith('\n')) content += '\n'
    content += `# Athena MCP config (may have contained an auth token in older installs)\n${relConfigPath}\n`
    fs.writeFileSync(gitignorePath, content)
  } catch {}
}

export function removeMcpEntry(agentType: AgentType, projectRoot?: string): PluginSetupResult {
  const discovery = discoverAgent(agentType, projectRoot)

  if (!discovery.configExists || !discovery.configPath) {
    return { success: true, configPath: discovery.configPath || '', created: false, updated: false }
  }

  let cfg: Record<string, unknown>
  try {
    cfg = JSON.parse(fs.readFileSync(discovery.configPath, 'utf8'))
  } catch (err) {
    return {
      success: false,
      configPath: discovery.configPath,
      created: false,
      updated: false,
      error: `Existing MCP config at ${discovery.configPath} is not valid JSON; refusing to modify it (${err instanceof Error ? err.message : String(err)})`,
    }
  }

  const servers = cfg.mcpServers as Record<string, unknown> | undefined
  const hasEntry = !!servers?.athena || 'athena' in cfg // legacy pre-mcpServers layout
  if (!hasEntry) {
    return { success: true, configPath: discovery.configPath, created: false, updated: false }
  }

  if (servers?.athena) delete servers.athena
  if ('athena' in cfg) delete cfg.athena
  writeConfigAtomic(discovery.configPath, cfg)

  return { success: true, configPath: discovery.configPath, created: false, updated: true }
}

export function checkMcpServerReachable(
  port: number = MCP_PORT,
  host: string = MCP_HOST,
): Promise<boolean> {
  return new Promise((resolve) => {
    let settled = false
    const socket = net.createConnection({ port, host })
    const finish = (reachable: boolean) => {
      if (settled) return
      settled = true
      socket.removeAllListeners()
      socket.destroy()
      resolve(reachable)
    }
    socket.setTimeout(2000, () => finish(false))
    socket.on('connect', () => finish(true))
    socket.on('error', () => finish(false))
  })
}
