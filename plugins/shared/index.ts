export { McpConnection, createMcpConnection } from './connection'
export { OutputForwarder, createOutputForwarder, hookStreamToForwarder } from './outputForwarder'
export {
  discoverOpenCode,
  discoverClaudeCode,
  discoverAll,
  setupOpenCode,
  setupClaudeCode,
  removeMcpEntry,
  checkMcpServerReachable,
} from './setup'
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
