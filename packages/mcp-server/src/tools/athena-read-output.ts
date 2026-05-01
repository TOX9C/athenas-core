import { z } from 'zod'
import type { OutputBufferManager } from '../output-buffer.js'

export const athenaReadOutputSchema = z.object({
  paneId: z.string().min(1).describe('The pane ID to read output from'),
  lines: z
    .number()
    .int()
    .positive()
    .optional()
    .describe('Number of most recent lines to return. Omit for full buffer.'),
  sinceTimestamp: z
    .number()
    .int()
    .positive()
    .optional()
    .describe('Only return entries at or after this Unix ms timestamp'),
})

export type AthenaReadOutputInput = z.infer<typeof athenaReadOutputSchema>

export async function athenaReadOutput(
  bufferManager: OutputBufferManager,
  input: AthenaReadOutputInput,
) {
  const entries = bufferManager.read(input.paneId, {
    lines: input.lines,
    sinceTimestamp: input.sinceTimestamp,
  })

  if (entries.length === 0) {
    return {
      content: [
        {
          type: 'text' as const,
          text: `No output found for pane "${input.paneId}".`,
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
