import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'

export const athenaReportErrorSchema = z.object({
  agentId: z.string().min(1).describe('Unique identifier for the agent reporting the error'),
  error: z.string().min(1).describe('Error message or description'),
  stack: z.string().optional().describe('Stack trace if available'),
  code: z.union([z.string(), z.number()]).optional().describe('Error code (e.g. EXIT_1, 137)'),
  recoverable: z
    .boolean()
    .default(true)
    .describe('Whether the agent can continue after this error'),
  context: z
    .record(z.unknown())
    .optional()
    .describe('Additional structured context about the error'),
})

export type AthenaReportErrorInput = z.infer<typeof athenaReportErrorSchema>

export async function athenaReportError(bridge: AthenaBridge, input: AthenaReportErrorInput) {
  await bridge.reportError({
    agentId: input.agentId,
    error: input.error,
    stack: input.stack,
    code: input.code,
    recoverable: input.recoverable,
    context: input.context,
  })

  const severity = input.recoverable ? 'RECOVERABLE' : 'FATAL'
  const codeStr = input.code ? ` [${input.code}]` : ''

  return {
    content: [
      {
        type: 'text' as const,
        text: `Error reported for agent ${input.agentId}: ${severity}${codeStr} — ${input.error}`,
      },
    ],
  }
}
