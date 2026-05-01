import { z } from 'zod'

export const controlResumeSchema = z.object({
  paneId: z.string().min(1).describe('The pane ID to resume'),
})

export type ControlResumeInput = z.infer<typeof controlResumeSchema>

export async function controlResume(_input: ControlResumeInput) {
  return {
    content: [
      {
        type: 'text' as const,
        text: 'control_resume is not yet available. This tool will be enabled in Phase 2.',
      },
    ],
    isError: true,
  }
}
