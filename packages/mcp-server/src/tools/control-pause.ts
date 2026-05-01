import { z } from 'zod'

export const controlPauseSchema = z.object({
  paneId: z.string().min(1).describe('The pane ID to pause'),
  reason: z.string().optional().describe('Optional reason for pausing'),
})

export type ControlPauseInput = z.infer<typeof controlPauseSchema>

export async function controlPause(_input: ControlPauseInput) {
  return {
    content: [
      {
        type: 'text' as const,
        text: 'control_pause is not yet available. This tool will be enabled in Phase 2.',
      },
    ],
    isError: true,
  }
}
