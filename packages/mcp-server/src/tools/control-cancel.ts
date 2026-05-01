import { z } from 'zod'

export const controlCancelSchema = z.object({
  paneId: z.string().min(1).describe('The pane ID to cancel'),
  force: z.boolean().default(false).describe('Force-kill the process'),
})

export type ControlCancelInput = z.infer<typeof controlCancelSchema>

export async function controlCancel(_input: ControlCancelInput) {
  return {
    content: [
      {
        type: 'text' as const,
        text: 'control_cancel is not yet available. This tool will be enabled in Phase 2.',
      },
    ],
    isError: true,
  }
}
