import type { AgentType } from './workspace'

export type AgentRole = 'coordinator' | 'builder' | 'scout' | 'reviewer'
export type SwarmTaskStatus = 'queued' | 'building' | 'review' | 'done' | 'blocked' | 'stalled'

export interface SwarmTask {
  id: string
  title: string
  description: string
  assignedAgentId: string
  ownedFiles: string[]
  status: SwarmTaskStatus
  dependsOn: string[]
  createdAt: number
  completedAt: number | null
  lastUpdatedAt: number
}

export interface SwarmAgent {
  id: string
  role: AgentRole
  agentType: AgentType
  paneId: string
  status: 'idle' | 'thinking' | 'writing' | 'waiting' | 'done' | 'blocked' | 'stalled'
  currentTask: string | null
  lastAction: string
  lastActionAt: number
}

export interface MailboxMessage {
  id: string
  from: string
  to: string
  content: string
  timestamp: number
  read: boolean
}

export interface SwarmState {
  id: string
  goal: string
  agents: SwarmAgent[]
  tasks: SwarmTask[]
  messages: MailboxMessage[]
  status: 'active' | 'paused' | 'completed'
  startedAt: number
}
