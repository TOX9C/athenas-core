import type { AthenaBridge } from '../bridge.js'

export function registerResources(server: any, bridge: AthenaBridge): void {
  server.registerResource(
    'athena://agents',
    'athena://agents',
    {
      name: 'Active Agents',
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
    'athena://agent/{id}',
    'athena://agent/{id}',
    {
      name: 'Agent State',
      description: 'State of a specific agent by ID. Use athena://agent/{agentId} as the URI.',
      mimeType: 'application/json',
    },
    async (uri: URL) => {
      const agentId = uri.pathname.replace('/agent/', '')
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
      name: 'App State',
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
