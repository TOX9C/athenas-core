import type { AgentType } from './workspace'

export type KanbanStatus = 'todo' | 'in_progress' | 'in_review' | 'complete'

export interface KanbanTask {
  id: string
  spaceId: string
  title: string
  description?: string
  assignedAgent?: AgentType
  status: KanbanStatus
  order: number
  createdAt: number
}
