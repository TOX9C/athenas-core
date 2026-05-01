import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'
import type { AgentListEntry } from '../types/index.js'

export const athenaListAgentsSchema = z.object({})

export type AthenaListAgentsInput = z.infer<typeof athenaListAgentsSchema>

export async function athenaListAgents(bridge: AthenaBridge, _input: AthenaListAgentsInput) {
  const agents = bridge.getAllAgentStates()

  if (agents.length === 0) {
    return {
      content: [
        {
          type: 'text' as const,
          text: 'No active agents.',
        },
      ],
    }
  }

  const entries: AgentListEntry[] = agents.map((a) => ({
    paneId: a.id,
    agentType: a.type,
    status: a.status,
    label: a.role,
    lastActivityAt: a.lastActivityAt,
  }))

  const formatted = entries
    .map((e) => {
      const activity = e.lastActivityAt
        ? ` | last: ${new Date(e.lastActivityAt).toISOString()}`
        : ''
      const label = e.label ? ` (${e.label})` : ''
      return `${e.paneId} — ${e.agentType}${label} [${e.status}]${activity}`
    })
    .join('\n')

  return {
    content: [
      {
        type: 'text' as const,
        text: `Active agents (${entries.length}):\n${formatted}`,
      },
    ],
  }
}
