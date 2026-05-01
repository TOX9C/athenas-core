import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'
import type { AgentStatus } from '../types/index.js'

export const statusUpdateSchema = z.object({
  status: z
    .enum(['idle', 'thinking', 'working', 'waiting_for_input', 'completed', 'error', 'cancelled'])
    .describe('Current agent status'),
  message: z.string().optional().describe('Optional human-readable status detail'),
  progress: z
    .object({
      current: z.number().describe('Current step'),
      total: z.number().describe('Total steps'),
      label: z.string().optional().describe('Step description'),
    })
    .optional()
    .describe('Optional progress indicator'),
  artifacts: z
    .array(
      z.object({
        path: z.string().describe('File path or URI'),
        type: z.enum(['file', 'url', 'image', 'log']).describe('Artifact type'),
      }),
    )
    .optional()
    .describe('Optional list of files or outputs produced'),
  agentId: z.string().optional().describe('ID of the agent reporting status'),
})

export type StatusUpdateInput = z.infer<typeof statusUpdateSchema>

const STATUS_MAP: Record<string, AgentStatus> = {
  idle: 'idle',
  thinking: 'running',
  working: 'running',
  waiting_for_input: 'waiting',
  completed: 'done',
  error: 'error',
  cancelled: 'idle',
}

export async function statusUpdate(bridge: AthenaBridge, input: StatusUpdateInput) {
  const mappedStatus = STATUS_MAP[input.status] ?? 'idle'
  const agentId = input.agentId ?? 'unknown'
  const progressPct = input.progress
    ? Math.round((input.progress.current / input.progress.total) * 100)
    : undefined

  await bridge.updateStatus({
    agentId,
    status: mappedStatus,
    message: input.message ?? input.progress?.label,
    progress: progressPct,
    details: {
      rawStatus: input.status,
      progress: input.progress,
      artifacts: input.artifacts,
    },
  })

  return {
    content: [
      {
        type: 'text' as const,
        text: `Status updated to: ${input.status}`,
      },
    ],
  }
}
