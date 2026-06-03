import * as fs from 'fs'
import * as path from 'path'
import * as os from 'os'
import * as net from 'net'
import type { PluginDiscoveryResult, PluginSetupOptions, PluginSetupResult } from './types'
import { MCP_PORT, MCP_HOST, PLUGIN_IDS, buildProxyCommand } from './types'

export { McpConnection, createMcpConnection } from './connection'

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

export function discoverOpenCode(projectRoot?: string): PluginDiscoveryResult {
  const configPaths: string[] = []
  if (projectRoot) configPaths.push(path.join(projectRoot, '.opencode', 'mcp.json'))
  configPaths.push(path.join(os.homedir(), '.opencode', 'mcp.json'))

  let configPath: string | null = null
  let configExists = false
  let mcpEntryExists = false

  for (const p of configPaths) {
    if (fs.existsSync(p)) {
      configPath = p
      configExists = true
      try {
        const cfg = JSON.parse(fs.readFileSync(p, 'utf8'))
        mcpEntryExists = !!cfg?.athena
      } catch {}
      break
    }
  }

  if (!configPath) {
    configPath = configPaths[0]
  }

  return {
    agentType: 'opencode',
    installed: !!findBinary(['opencode']),
    configPath,
    configExists,
    mcpEntryExists,
    binaryPath: findBinary(['opencode']),
  }
}

export function discoverClaudeCode(projectRoot?: string): PluginDiscoveryResult {
  const configPaths: string[] = []
  if (projectRoot) configPaths.push(path.join(projectRoot, '.claude', 'mcp.json'))
  configPaths.push(path.join(os.homedir(), '.claude', 'mcp.json'))

  let configPath: string | null = null
  let configExists = false
  let mcpEntryExists = false

  for (const p of configPaths) {
    if (fs.existsSync(p)) {
      configPath = p
      configExists = true
      try {
        const cfg = JSON.parse(fs.readFileSync(p, 'utf8'))
        mcpEntryExists = !!cfg?.athena
      } catch {}
      break
    }
  }

  if (!configPath) {
    configPath = configPaths[0]
  }

  return {
    agentType: 'claude-code',
    installed: !!findBinary(['claude']),
    configPath,
    configExists,
    mcpEntryExists,
    binaryPath: findBinary(['claude']),
  }
}

export function discoverAll(projectRoot?: string): PluginDiscoveryResult[] {
  return [discoverOpenCode(projectRoot), discoverClaudeCode(projectRoot)]
}

export function setupOpenCode(options: PluginSetupOptions): PluginSetupResult {
  const discovery = discoverOpenCode(options.projectRoot)
  return writeMcpConfig(discovery, options, 'opencode')
}

export function setupClaudeCode(options: PluginSetupOptions): PluginSetupResult {
  const discovery = discoverClaudeCode(options.projectRoot)
  return writeMcpConfig(discovery, options, 'claude-code')
}

function writeMcpConfig(
  discovery: PluginDiscoveryResult,
  options: PluginSetupOptions,
  agentType: 'opencode' | 'claude-code',
): PluginSetupResult {
  const proxyPath = resolveProxyPath()
  const { command, args } = buildProxyCommand(proxyPath)

  const env: Record<string, string> = {
    ATHENA_MCP_TOKEN: options.token,
    ATHENA_MCP_PORT: String(options.port || MCP_PORT),
    ATHENA_MCP_HOST: options.host || MCP_HOST,
  }
  if (options.sessionId) {
    env.ATHENA_SESSION_ID = options.sessionId
  }

  const mcpEntry = {
    command,
    args,
    env,
  }

  let existing: Record<string, any> = {}
  if (discovery.configExists && discovery.configPath) {
    try {
      existing = JSON.parse(fs.readFileSync(discovery.configPath, 'utf8'))
    } catch {}
  }

  const wasUpdated = !!existing.athena
  existing.athena = mcpEntry

  const dir = path.dirname(discovery.configPath!)
  if (!fs.existsSync(dir)) {
    fs.mkdirSync(dir, { recursive: true })
  }

  const tmpPath = discovery.configPath! + '.tmp'
  fs.writeFileSync(tmpPath, JSON.stringify(existing, null, 2) + '\n')
  fs.renameSync(tmpPath, discovery.configPath!)

  return {
    success: true,
    configPath: discovery.configPath!,
    created: !discovery.configExists,
    updated: wasUpdated,
  }
}

export function removeMcpEntry(
  agentType: 'opencode' | 'claude-code',
  projectRoot?: string,
): PluginSetupResult {
  const discovery =
    agentType === 'opencode' ? discoverOpenCode(projectRoot) : discoverClaudeCode(projectRoot)

  if (!discovery.configExists || !discovery.configPath) {
    return { success: true, configPath: discovery.configPath || '', created: false, updated: false }
  }

  try {
    const cfg = JSON.parse(fs.readFileSync(discovery.configPath, 'utf8'))
    if (!cfg.athena) {
      return { success: true, configPath: discovery.configPath, created: false, updated: false }
    }

    delete cfg.athena
    const tmpPath = discovery.configPath + '.tmp'
    fs.writeFileSync(tmpPath, JSON.stringify(cfg, null, 2) + '\n')
    fs.renameSync(tmpPath, discovery.configPath)

    return { success: true, configPath: discovery.configPath, created: false, updated: true }
  } catch (err: any) {
    return {
      success: false,
      configPath: discovery.configPath,
      created: false,
      updated: false,
      error: err.message,
    }
  }
}

export function checkMcpServerReachable(
  port: number = MCP_PORT,
  host: string = MCP_HOST,
): Promise<boolean> {
  return new Promise((resolve) => {
    const socket = net.createConnection({ port, host }, () => {
      socket.end()
      resolve(true)
    })
    socket.on('error', () => resolve(false))
    socket.setTimeout(2000, () => {
      socket.destroy()
      resolve(false)
    })
  })
}
