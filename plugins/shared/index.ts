export { McpConnection, createMcpConnection } from './connection'
export {
  OutputForwarder,
  createOutputForwarder,
  hookStreamToForwarder,
  pipeWriteStreamToForwarder,
} from './outputForwarder'
export {
  AGENT_PROFILES,
  discoverAgent,
  discoverOpenCode,
  discoverClaudeCode,
  discoverAll,
  setupAgent,
  setupOpenCode,
  setupClaudeCode,
  removeMcpEntry,
  checkMcpServerReachable,
} from './setup'
export { runPluginSetupCli } from './cli'
export { MCP_PORT, MCP_HOST, COMMS_PORT, PLUGIN_IDS, buildProxyCommand } from './types'
export type {
  PluginDiscoveryResult,
  PluginSetupOptions,
  PluginSetupResult,
  OutputChannel,
  OutputEntry,
  OutputBatch,
  OutputForwarderConfig,
} from './types'
