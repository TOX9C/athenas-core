import { randomUUID } from 'crypto'
import type { ExecutionPlan, PlanStep, PlanStatus, StepStatus } from '../types/orchestrator'

let activePlan: ExecutionPlan | null = null

export function setActivePlan(input: {
  goal: string
  reasoning: string
  steps: Omit<PlanStep, 'status'>[]
}): ExecutionPlan {
  activePlan = {
    id: `plan-${randomUUID().slice(0, 8)}`,
    goal: input.goal,
    reasoning: input.reasoning,
    steps: input.steps.map((s) => ({ ...s, status: 'pending' as const })),
    status: 'pending',
    createdAt: Date.now(),
  }
  return activePlan
}

export function getActivePlan(): ExecutionPlan | null {
  return activePlan
}

export function updateStepStatus(stepId: string, status: StepStatus, paneId?: string): boolean {
  if (!activePlan) return false
  const step = activePlan.steps.find((s) => s.id === stepId)
  if (!step) return false

  step.status = status
  if (paneId) step.assigned_pane_id = paneId

  const hasInProgress = activePlan.steps.some((s) => s.status === 'in_progress')
  if (hasInProgress && activePlan.status === 'pending') {
    activePlan.status = 'in_progress'
  }

  return true
}

export function updatePlanStatus(status: PlanStatus): boolean {
  if (!activePlan) return false
  activePlan.status = status
  return true
}

export function clearActivePlan(): void {
  activePlan = null
}
