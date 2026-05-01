import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'

export const athenaReportCompletionSchema = z.object({
  agentId: z.string().min(1).describe('Unique identifier for the completing agent'),
  summary: z.string().min(1).describe('Summary of what was accomplished'),
  artifacts: z
    .array(z.string())
    .optional()
    .describe('List of file paths or resources created/modified'),
  metrics: z
    .record(z.number())
    .optional()
    .describe('Quantitative metrics (e.g. filesChanged: 5, linesAdded: 120)'),
  duration: z.number().optional().describe('Duration in seconds the task took to complete'),
})

export type AthenaReportCompletionInput = z.infer<typeof athenaReportCompletionSchema>

export async function athenaReportCompletion(
  bridge: AthenaBridge,
  input: AthenaReportCompletionInput,
) {
  await bridge.reportCompletion({
    agentId: input.agentId,
    summary: input.summary,
    artifacts: input.artifacts,
    metrics: input.metrics,
    duration: input.duration,
  })

  const durationStr = input.duration !== undefined ? ` in ${input.duration}s` : ''
  const artifactCount = input.artifacts?.length ?? 0
  const artifactStr =
    artifactCount > 0 ? ` (${artifactCount} artifact${artifactCount !== 1 ? 's' : ''})` : ''

  return {
    content: [
      {
        type: 'text' as const,
        text: `Agent ${input.agentId} completed${durationStr}${artifactStr}: ${input.summary}`,
      },
    ],
  }
}
