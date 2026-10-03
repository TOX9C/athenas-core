import {
  AGENT_PROFILES,
  discoverAgent,
  setupAgent,
  removeMcpEntry,
  checkMcpServerReachable,
} from './setup'
import { createOutputForwarder, pipeWriteStreamToForwarder } from './outputForwarder'
import { MCP_PORT, MCP_HOST } from './types'
import type { PluginSetupOptions, OutputForwarderConfig } from './types'

type AgentType = keyof typeof AGENT_PROFILES

/**
 * Single parameterized CLI entrypoint shared by the per-agent wrappers in
 * `plugins/<agent>-athena/setup.ts`.
 */
export async function runPluginSetupCli(agentType: AgentType): Promise<void> {
  const profile = AGENT_PROFILES[agentType]
  const args = process.argv.slice(2)
  const command = args[0] || 'setup'

  const token = process.env.ATHENA_MCP_TOKEN || ''
  const port = parseInt(
    process.env.ATHENA_MCP_TCP_PORT || process.env.ATHENA_MCP_PORT || String(MCP_PORT),
    10,
  )
  const host = process.env.ATHENA_MCP_HOST || MCP_HOST
  const sessionId = process.env.ATHENA_SESSION_ID
  const projectRoot = process.cwd()
  const useGlobal = args.includes('--global')
  const autoForwardOutput = (process.env.ATHENA_AUTO_FORWARD_OUTPUT ?? 'false') === 'true'

  if (command === 'discover') {
    const result = discoverAgent(agentType, projectRoot)
    console.log(JSON.stringify(result, null, 2))
    return
  }

  if (command === 'remove') {
    const result = removeMcpEntry(agentType, useGlobal ? undefined : projectRoot)
    console.log(JSON.stringify(result, null, 2))
    return
  }

  if (command === 'check') {
    const reachable = await checkMcpServerReachable(port, host)
    const discovery = discoverAgent(agentType, projectRoot)
    console.log(JSON.stringify({ reachable, ...discovery }, null, 2))
    return
  }

  if (command === 'forward') {
    if (!token) {
      console.error('Error: ATHENA_MCP_TOKEN is required for output forwarding.')
      process.exit(1)
    }

    const config: OutputForwarderConfig = {
      token,
      port,
      host,
      sessionId,
      autoForwardOutput: true,
    }
    const forwarder = createOutputForwarder(config)
    await forwarder.start()

    // process.stdout/stderr are WriteStreams: hooking `data` never fires, so
    // wrap `write` instead.
    const unhookStdout = pipeWriteStreamToForwarder(process.stdout, forwarder, 'stdout')
    const unhookStderr = pipeWriteStreamToForwarder(process.stderr, forwarder, 'stderr')

    const cleanup = () => {
      unhookStdout()
      unhookStderr()
      forwarder.stop().catch(() => {})
    }
    process.on('SIGTERM', cleanup)
    process.on('SIGINT', cleanup)
    process.on('exit', cleanup)

    console.error('[athena-plugin] Output forwarding active (session: %s)', sessionId || 'unknown')
    return
  }

  if (command === 'setup' || command === 'install') {
    if (!token) {
      console.error('Error: ATHENA_MCP_TOKEN is required. Athena must be running to get a token.')
      process.exit(1)
    }

    const reachable = await checkMcpServerReachable(port, host)
    if (!reachable) {
      console.error(`Error: Athena MCP server not reachable at ${host}:${port}. Is Athena running?`)
      process.exit(1)
    }

    const options: PluginSetupOptions = {
      token,
      port,
      host,
      sessionId,
      projectRoot: useGlobal ? undefined : projectRoot,
      global: useGlobal,
    }

    const result = setupAgent(agentType, options)

    if (result.success) {
      const verb = result.created ? 'Created' : result.updated ? 'Updated' : 'Configured'
      console.log(`${verb} ${profile.displayName} MCP config at: ${result.configPath}`)

      if (!useGlobal) {
        console.log(
          `Auth token stored in user-level config (~/${profile.configDir}/mcp.json), not in the project config.`,
        )
      }

      if (autoForwardOutput) {
        console.log('\nOutput forwarding: ENABLED (ATHENA_AUTO_FORWARD_OUTPUT=true)')
        console.log('Agent stdout/stderr will be forwarded to Athena via athena_forward_output.')
      } else {
        console.log('\nOutput forwarding: disabled (set ATHENA_AUTO_FORWARD_OUTPUT=true to enable)')
      }

      console.log(
        `\nAthena MCP server is now available in ${profile.displayName} as the "athena" MCP server.`,
      )
      console.log(`Restart ${profile.displayName} to pick up the new configuration.`)
    } else {
      console.error(`Error: ${result.error}`)
      process.exit(1)
    }
    return
  }

  console.log(`Usage: node setup.js [command]

Commands:
  setup, install Configure ${profile.displayName} to connect to Athena MCP server
  discover       Check if ${profile.displayName} is installed and show config status
  remove         Remove Athena MCP entry from ${profile.displayName} config
  check          Check if Athena MCP server is reachable
  forward        Start output forwarding (hooks stdout/stderr to Athena)

Options:
  --global Use global config directory (~/${profile.configDir}/) instead of project

Environment:
  ATHENA_MCP_TOKEN          Required for setup. Auth token from Athena.
  ATHENA_MCP_TCP_PORT     MCP server TCP port (default: ${MCP_PORT})
  ATHENA_MCP_HOST           MCP server host (default: ${MCP_HOST})
  ATHENA_SESSION_ID         Optional session ID for agent identification
  ATHENA_AUTO_FORWARD_OUTPUT  Enable automatic output forwarding (true/false, default: false)`)
}
