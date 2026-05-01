import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'
import type { AthenaNotification, NotificationType, NotificationPriority } from '../types/index.js'

export const notifySchema = z.object({
  level: z
    .enum(['info', 'warning', 'error', 'success'])
    .describe('Severity level of the notification'),
  message: z.string().min(1).describe('Human-readable notification text'),
  title: z.string().optional().describe('Optional short title for the notification'),
  metadata: z
    .record(z.unknown())
    .optional()
    .describe('Optional structured data attached to the notification'),
  actions: z
    .array(
      z.object({
        id: z.string().describe('Action identifier'),
        label: z.string().describe('Button label'),
      }),
    )
    .optional()
    .describe('Optional action buttons the user can tap'),
  priority: z
    .enum(['low', 'normal', 'high', 'critical'])
    .default('normal')
    .describe('Priority level affecting display urgency'),
  agentId: z.string().optional().describe('ID of the agent sending this notification'),
})

export type NotifyInput = z.infer<typeof notifySchema>

export async function notify(bridge: AthenaBridge, input: NotifyInput) {
  const notification: AthenaNotification = {
    type: input.level as NotificationType,
    title: input.title ?? input.message.slice(0, 60),
    message: input.message,
    priority: input.priority as NotificationPriority,
    agentId: input.agentId,
    timestamp: Date.now(),
  }

  await bridge.sendNotification(notification)

  return {
    content: [
      {
        type: 'text' as const,
        text: 'Notification delivered.',
      },
    ],
  }
}
