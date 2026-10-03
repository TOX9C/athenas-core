import { McpServer, ResourceTemplate } from '@modelcontextprotocol/sdk/server/mcp.js'
import type { AthenaBridge } from '../bridge.js'

export function registerResources(server: McpServer, bridge: AthenaBridge): void {
  server.registerResource(
    'athena://agents',
    'athena://agents',
    {
      title: 'Active Agents',
      description: 'Current state of all agents connected to Athena',
      mimeType: 'application/json',
    },
    async () => ({
      contents: [
        {
          uri: 'athena://agents',
          mimeType: 'application/json',
          text: JSON.stringify(bridge.getAllAgentStates(), null, 2),
        },
      ],
    }),
  )

  server.registerResource(
    'athena-agent',
    new ResourceTemplate('athena://agent/{id}', { list: undefined }),
    {
      title: 'Agent State',
      description: 'State of a specific agent by ID. Use athena://agent/{agentId} as the URI.',
      mimeType: 'application/json',
    },
    async (uri: URL, variables: Record<string, string | string[]>) => {
      // Prefer the template variable; fall back to the URI tail for plain
      // `athena://agent/<id>` reads (pathname carries a leading slash).
      const fromTemplate = Array.isArray(variables.id) ? variables.id[0] : variables.id
      const agentId = (fromTemplate ?? uri.pathname.replace(/^\//, '')).replace(/^\//, '')
      const state = bridge.getAgentState(agentId)

      if (!state) {
        return {
          contents: [
            {
              uri: uri.href,
              mimeType: 'application/json',
              text: JSON.stringify({ error: `Agent ${agentId} not found` }),
            },
          ],
        }
      }

      return {
        contents: [
          {
            uri: uri.href,
            mimeType: 'application/json',
            text: JSON.stringify(state, null, 2),
          },
        ],
      }
    },
  )

  server.registerResource(
    'athena://app-state',
    'athena://app-state',
    {
      title: 'App State',
      description: 'Full Athena application state snapshot including spaces, theme, and agents',
      mimeType: 'application/json',
    },
    async () => ({
      contents: [
        {
          uri: 'athena://app-state',
          mimeType: 'application/json',
          text: JSON.stringify(bridge.getAppState(), null, 2),
        },
      ],
    }),
  )
}
