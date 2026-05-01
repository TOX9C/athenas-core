export type StepStatus = 'pending' | 'in_progress' | 'completed' | 'failed'
export type PlanStatus = 'pending' | 'in_progress' | 'completed' | 'failed'

export interface PlanStep {
  id: string
  title: string
  description: string
  agent_type: string
  task_prompt: string
  depends_on: string[]
  status: StepStatus
  assigned_pane_id?: string
  result_summary?: string
}

export interface ExecutionPlan {
  id: string
  goal: string
  reasoning: string
  steps: PlanStep[]
  status: PlanStatus
  createdAt: number
}

export interface StepEvaluation {
  step_id: string
  status: 'success' | 'failure' | 'incomplete'
  summary: string
}

export interface EvaluationResult {
  plan_id: string
  overall_status: 'success' | 'partial_success' | 'failure' | 'needs_replanning'
  step_evaluations: StepEvaluation[]
  next_action: 'done' | 'replan' | 'retry_steps' | 'escalate_to_user'
  reasoning: string
}
