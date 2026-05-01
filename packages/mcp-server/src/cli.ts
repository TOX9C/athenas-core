#!/usr/bin/env node
import { AthenaMcpServer } from './server.js'
import type { TransportType, ServerConfig } from './types/index.js'

function parseArgs(): Partial<ServerConfig> {
  const args = process.argv.slice(2)
  const config: Partial<ServerConfig> = {}

  for (let i = 0; i < args.length; i++) {
    const arg = args[i]
    switch (arg) {
      case '--transport':
      case '-t':
        config.transport = (args[++i] ?? 'stdio') as TransportType
        break
      case '--port':
      case '-p':
        config.websocketPort = parseInt(args[++i] ?? '4546', 10)
        break
      case '--tcp-port':
        config.tcpPort = parseInt(args[++i] ?? '4545', 10)
        break
      case '--athena-host':
        config.athenaHost = args[++i]
        break
      case '--athena-port':
        config.athenaPort = parseInt(args[++i] ?? '4545', 10)
        break
      case '--auth-token':
        config.authToken = args[++i]
        break
      case '--name':
        config.name = args[++i]
        break
      case '--help':
      case '-h':
        printHelp()
        process.exit(0)
    }
  }

  return config
}

function printHelp(): void {
  console.log(`
athena-mcp-server — MCP Server for Athena's Core

USAGE
  athena-mcp-server [OPTIONS]

OPTIONS
  --transport, -t <type>       Transport: stdio (default), websocket, or tcp
  --port, -p <number>          WebSocket server port (default: 4546)
  --tcp-port <number>          TCP server port (default: 4545)
  --athena-host <host>         Athena Electron app host (default: 127.0.0.1)
  --athena-port <number>       Athena Electron app port (default: 4545)
  --auth-token <token>         Authentication token for Athena connection
  --name <name>                Server name in MCP protocol (default: athena-mcp-server)
  --help, -h                   Show this help message

TRANSPORTS
  stdio        JSON-RPC over stdin/stdout (for CLI agent integration like Claude Code)
  tcp          Newline-delimited JSON-RPC over TCP (spec-compliant primary transport)
  websocket    WebSocket server (for real-time communication with the Electron app)

ENVIRONMENT VARIABLES
  ATHENA_MCP_TRANSPORT     Transport type (overrides --transport)
  ATHENA_MCP_PORT          WebSocket port (overrides --port)
  ATHENA_MCP_TCP_PORT      TCP port (overrides --tcp-port)
  ATHENA_MCP_HOST          Athena host (overrides --athena-host)
  ATHENA_MCP_ATHENA_PORT   Athena port (overrides --athena-port)
  ATHENA_MCP_TOKEN         Auth token (overrides --auth-token)

EXAMPLES
  # Start with stdio (for Claude Code / OpenCode MCP config)
  athena-mcp-server

  # Start TCP server (spec-compliant primary transport)
  athena-mcp-server --transport tcp --tcp-port 4545

  # Start WebSocket server on custom port
  athena-mcp-server --transport websocket --port 5000

  # Connect to Athena on a specific port with auth
  athena-mcp-server --athena-port 4545 --auth-token my-secret-token
`)
}

function applyEnvOverrides(config: Partial<ServerConfig>): Partial<ServerConfig> {
  const merged = { ...config }

  if (process.env.ATHENA_MCP_TRANSPORT) {
    merged.transport = process.env.ATHENA_MCP_TRANSPORT as TransportType
  }
  if (process.env.ATHENA_MCP_PORT) {
    merged.websocketPort = parseInt(process.env.ATHENA_MCP_PORT, 10)
  }
  if (process.env.ATHENA_MCP_TCP_PORT) {
    merged.tcpPort = parseInt(process.env.ATHENA_MCP_TCP_PORT, 10)
  }
  if (process.env.ATHENA_MCP_HOST) {
    merged.athenaHost = process.env.ATHENA_MCP_HOST
  }
  if (process.env.ATHENA_MCP_ATHENA_PORT) {
    merged.athenaPort = parseInt(process.env.ATHENA_MCP_ATHENA_PORT, 10)
  }
  if (process.env.ATHENA_MCP_TOKEN) {
    merged.authToken = process.env.ATHENA_MCP_TOKEN
  }

  return merged
}

async function main(): Promise<void> {
  const cliConfig = parseArgs()
  const config = applyEnvOverrides(cliConfig)

  const server = new AthenaMcpServer(config)

  const cleanup = async () => {
    await server.stop()
    process.exit(0)
  }

  process.on('SIGINT', cleanup)
  process.on('SIGTERM', cleanup)
  process.on('SIGHUP', cleanup)

  try {
    await server.start()

    if (config.transport === 'tcp') {
      console.error(`Athena MCP Server started on TCP port ${config.tcpPort ?? 4545}`)
    } else if (config.transport === 'websocket') {
      console.error(`Athena MCP Server started on WebSocket port ${config.websocketPort ?? 4546}`)
    } else {
      console.error('Athena MCP Server started on stdio')
    }
  } catch (err) {
    console.error('Failed to start Athena MCP Server:', err instanceof Error ? err.message : err)
    process.exit(1)
  }
}

main()
