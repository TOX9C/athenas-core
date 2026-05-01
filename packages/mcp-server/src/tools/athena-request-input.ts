import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'

export const athenaRequestInputSchema = z.object({
  prompt: z.string().min(1).describe('The question or prompt to present to the user'),
  defaultResponse: z
    .string()
    .optional()
    .describe('Default value if the user dismisses without answering'),
  timeout: z
    .number()
    .min(1000)
    .max(600_000)
    .default(120_000)
    .describe('Timeout in milliseconds (1s–10min, default 2min)'),
  agentId: z.string().optional().describe('ID of the agent requesting input'),
})

export type AthenaRequestInputInput = z.infer<typeof athenaRequestInputSchema>

export async function athenaRequestInput(bridge: AthenaBridge, input: AthenaRequestInputInput) {
  const response = await bridge.requestInput({
    prompt: input.prompt,
    defaultResponse: input.defaultResponse,
    timeout: input.timeout,
    agentId: input.agentId,
  })

  if (response.timedOut) {
    return {
      content: [
        {
          type: 'text' as const,
          text: `Input request timed out after ${input.timeout}ms. ${input.defaultResponse ? `Default: "${input.defaultResponse}"` : 'No default provided.'}`,
        },
      ],
      isError: true,
    }
  }

  if (response.cancelled) {
    return {
      content: [
        {
          type: 'text' as const,
          text: `User dismissed the input request. ${input.defaultResponse ? `Default: "${input.defaultResponse}"` : 'No response provided.'}`,
        },
      ],
    }
  }

  return {
    content: [
      {
        type: 'text' as const,
        text: response.value,
      },
    ],
  }
}
