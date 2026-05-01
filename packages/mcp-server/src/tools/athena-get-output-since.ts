import { z } from 'zod'
import type { OutputBufferManager } from '../output-buffer.js'

export const athenaGetOutputSinceSchema = z.object({
  paneId: z.string().min(1).describe('The pane ID to get output from'),
  sinceTimestamp: z
    .number()
    .int()
    .positive()
    .optional()
    .describe('Return entries after this Unix ms timestamp'),
  sinceLine: z
    .number()
    .int()
    .positive()
    .optional()
    .describe('Return entries with line numbers greater than this value'),
})

export type AthenaGetOutputSinceInput = z.infer<typeof athenaGetOutputSinceSchema>

export async function athenaGetOutputSince(
  bufferManager: OutputBufferManager,
  input: AthenaGetOutputSinceInput,
) {
  if (!input.sinceTimestamp && !input.sinceLine) {
    return {
      content: [
        {
          type: 'text' as const,
          text: 'Provide at least one of: sinceTimestamp or sinceLine.',
        },
      ],
      isError: true,
    }
  }

  const entries = bufferManager.readSince(input.paneId, {
    sinceTimestamp: input.sinceTimestamp,
    sinceLine: input.sinceLine,
  })

  if (entries.length === 0) {
    const criterion: string[] = []
    if (input.sinceTimestamp) criterion.push(`timestamp > ${input.sinceTimestamp}`)
    if (input.sinceLine) criterion.push(`line > ${input.sinceLine}`)
    return {
      content: [
        {
          type: 'text' as const,
          text: `No new output for pane "${input.paneId}" since ${criterion.join(' and ')}.`,
        },
      ],
    }
  }

  const formatted = entries
    .map((e) => `[${e.lineNumber}]${e.isStderr ? ' [stderr]' : ''} ${e.content}`)
    .join('\n')

  return {
    content: [
      {
        type: 'text' as const,
        text: formatted,
      },
    ],
  }
}
