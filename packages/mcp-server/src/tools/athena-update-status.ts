import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'
import type { AgentStatus } from '../types/index.js'

export const athenaUpdateStatusSchema = z.object({
  agentId: z.string().min(1).describe('Unique identifier for the agent'),
  status: z
    .enum(['running', 'idle', 'error', 'waiting', 'done', 'blocked', 'stalled'])
    .describe('Current agent status'),
  message: z.string().optional().describe('Human-readable status description'),
  progress: z.number().min(0).max(100).optional().describe('Completion percentage 0–100'),
  details: z
    .record(z.unknown())
    .optional()
    .describe('Arbitrary structured details about the current state'),
})

export type AthenaUpdateStatusInput = z.infer<typeof athenaUpdateStatusSchema>

export async function athenaUpdateStatus(bridge: AthenaBridge, input: AthenaUpdateStatusInput) {
  await bridge.updateStatus({
    agentId: input.agentId,
    status: input.status as AgentStatus,
    message: input.message,
    progress: input.progress,
    details: input.details,
  })

  const progressStr = input.progress !== undefined ? ` (${input.progress}%)` : ''
  const messageStr = input.message ? `: ${input.message}` : ''

  return {
    content: [
      {
        type: 'text' as const,
        text: `Status updated for agent ${input.agentId}: ${input.status}${progressStr}${messageStr}`,
      },
    ],
  }
}
