import { z } from 'zod'
import type { AthenaBridge } from '../bridge.js'
import type { AthenaNotification, NotificationType, NotificationPriority } from '../types/index.js'

export const athenaNotifySchema = z.object({
  type: z.enum(['info', 'warning', 'error', 'success']).describe('The notification severity type'),
  title: z.string().min(1).describe('Short title for the notification'),
  message: z.string().min(1).describe('Detailed message body'),
  priority: z
    .enum(['low', 'normal', 'high', 'critical'])
    .default('normal')
    .describe('Priority level affecting display urgency'),
  agentId: z.string().optional().describe('ID of the agent sending this notification'),
})

export type AthenaNotifyInput = z.infer<typeof athenaNotifySchema>

export async function athenaNotify(bridge: AthenaBridge, input: AthenaNotifyInput) {
  const notification: AthenaNotification = {
    type: input.type as NotificationType,
    title: input.title,
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
        text: `Notification sent: [${input.priority}] ${input.type.toUpperCase()} — ${input.title}`,
      },
    ],
  }
}
