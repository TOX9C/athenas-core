#!/usr/bin/env node
import {
  discoverOpenCode,
  setupOpenCode,
  removeMcpEntry,
  checkMcpServerReachable,
  MCP_PORT,
  MCP_HOST,
  createOutputForwarder,
  hookStreamToForwarder,
} from '../shared'
import type { PluginSetupOptions, OutputForwarderConfig } from '../shared/types'

const args = process.argv.slice(2)
const command = args[0] || 'setup'

async function main() {
  const token = process.env.ATHENA_MCP_TOKEN || ''
  const port = parseInt(process.env.ATHENA_MCP_PORT || String(MCP_PORT), 10)
  const host = process.env.ATHENA_MCP_HOST || MCP_HOST
  const sessionId = process.env.ATHENA_SESSION_ID
  const projectRoot = process.cwd()
  const useGlobal = args.includes('--global')
  const autoForwardOutput = (process.env.ATHENA_AUTO_FORWARD_OUTPUT ?? 'false') === 'true'

  if (command === 'discover') {
    const result = discoverOpenCode(projectRoot)
    console.log(JSON.stringify(result, null, 2))
    return
  }

  if (command === 'remove') {
    const result = removeMcpEntry('opencode', useGlobal ? undefined : projectRoot)
    console.log(JSON.stringify(result, null, 2))
    return
  }

  if (command === 'check') {
    const reachable = await checkMcpServerReachable(port, host)
    const discovery = discoverOpenCode(projectRoot)
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

    const unhookStdout = hookStreamToForwarder(process.stdout, forwarder, 'stdout')
    const unhookStderr = hookStreamToForwarder(process.stderr, forwarder, 'stderr')

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

    const result = setupOpenCode(options)

    if (result.success) {
      const verb = result.created ? 'Created' : result.updated ? 'Updated' : 'Configured'
      console.log(`${verb} OpenCode MCP config at: ${result.configPath}`)

      if (autoForwardOutput) {
        console.log('\nOutput forwarding: ENABLED (ATHENA_AUTO_FORWARD_OUTPUT=true)')
        console.log('Agent stdout/stderr will be forwarded to Athena via athena_forward_output.')
      } else {
        console.log('\nOutput forwarding: disabled (set ATHENA_AUTO_FORWARD_OUTPUT=true to enable)')
      }

      console.log('\nAthena MCP server is now available in OpenCode as the "athena" MCP server.')
      console.log('Restart OpenCode to pick up the new configuration.')
    } else {
      console.error(`Error: ${result.error}`)
      process.exit(1)
    }
    return
  }

  console.log(`Usage: node setup.js [command]

Commands:
  setup, install Configure OpenCode to connect to Athena MCP server
  discover       Check if OpenCode is installed and show config status
  remove         Remove Athena MCP entry from OpenCode config
  check          Check if Athena MCP server is reachable
  forward        Start output forwarding (hooks stdout/stderr to Athena)

Options:
  --global Use global config directory (~/.opencode/) instead of project

Environment:
  ATHENA_MCP_TOKEN          Required for setup. Auth token from Athena.
  ATHENA_MCP_PORT           MCP server port (default: ${MCP_PORT})
  ATHENA_MCP_HOST           MCP server host (default: ${MCP_HOST})
  ATHENA_SESSION_ID         Optional session ID for agent identification
  ATHENA_AUTO_FORWARD_OUTPUT  Enable automatic output forwarding (true/false, default: false)`)
}

main().catch((err) => {
  console.error(err.message)
  process.exit(1)
})
