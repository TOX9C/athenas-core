import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'

export const requestInputSchema = z.object({
  prompt: z.string().min(1).describe('The question or prompt to display to the user'),
  options: z.array(z.string()).optional().describe('Predefined response options'),
  allowFreeText: z.boolean().default(true).describe('Whether the user can type a custom response'),
  timeoutMs: z
    .number()
    .min(0)
    .max(600_000)
    .default(120_000)
    .describe(
      'Maximum wait time in milliseconds (max 600000). 0 = wait indefinitely with no timeout',
    ),
  agentId: z.string().optional().describe('ID of the agent requesting input'),
})

export type RequestInputInput = z.infer<typeof requestInputSchema>

export async function requestInput(bridge: AthenaBridge, input: RequestInputInput) {
  const promptParts = [input.prompt]
  if (input.options && input.options.length > 0) {
    promptParts.push(`Options: ${input.options.join(', ')}`)
  }
  if (input.allowFreeText) {
    promptParts.push('(Free text input is allowed)')
  }

  const response = await bridge.requestInput({
    prompt: promptParts.join('\n'),
    defaultResponse: input.options?.[0] ?? '',
    // 0 disables the timeout entirely; bridge treats absent as 120s default.
    timeout: input.timeoutMs === 0 ? 0 : input.timeoutMs,
    agentId: input.agentId ?? 'unknown',
  })

  if (response.timedOut) {
    return {
      content: [
        {
          type: 'text' as const,
          text: JSON.stringify({ error: `Input request timed out after ${input.timeoutMs}ms` }),
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
          text: JSON.stringify({ response: '', type: 'cancelled' }),
        },
      ],
    }
  }

  const responseType = input.options?.includes(response.value) ? 'option' : 'freetext'
  return {
    content: [
      {
        type: 'text' as const,
        text: JSON.stringify({ response: response.value, type: responseType }),
      },
    ],
  }
}
