import { BrowserWindow } from 'electron'
import { randomUUID } from 'node:crypto'
import { ipcMain } from 'electron'
import { write as ptyWrite, hasSession } from './ptyManager'

export interface ToolInput {
  task_prompt?: string
  agent_count?: number
  agent_type?: string
  command?: string
  pane_ids?: string[]
  pane_id?: string
  limit?: number
  since_line?: number
  since_time?: number
  agent_id?: string
  goal?: string
  reasoning?: string
  steps?: any[]
  step_id?: string
  prompt?: string
  plan_id?: string
  overall_status?: string
  step_evaluations?: any[]
  next_action?: string
  question?: string
  options?: any[]
  message?: string
  target_agent_id?: string
  message_type?: string
}

export interface ToolCallResult {
  text: string
}

export const ORCHESTRATOR_TOOLS = [
  {
    name: 'close_terminals',
    description:
      'Close, remove, or replace terminal panes/agents from the UI entirely (using pane IDs). Use this tool whenever the user asks to close, exit, completely remove, or replace an existing running terminal/agent.',
    input_schema: {
      type: 'object' as const,
      properties: {
        pane_ids: {
          type: 'array',
          items: { type: 'string' },
          description: 'Array of string IDs of the panes to drop/remove.',
        },
      },
      required: ['pane_ids'],
    },
  },
  {
    name: 'launch_builtin_agent',
    description:
      "Launch one or more standard background agents using system built-in integrations. If the user doesn't specify a task, you MUST leave task_prompt empty to launch an interactive agent shell.",
    input_schema: {
      type: 'object' as const,
      properties: {
        agent_type: {
          type: 'string',
          description:
            "The type of agent to spawn. Must be one of: 'claude', 'codex', 'opencode', 'gemini', 'shell'. Examples: 'Open Code' -> 'opencode', 'Gemini' -> 'gemini'.",
        },
        task_prompt: {
          type: 'string',
          description:
            'Optional. The prompt to start the background agent with. Leave entirely empty or omit it to open a blank terminal.',
        },
        agent_count: {
          type: 'number',
          description: 'The number of agents to spawn.',
        },
      },
      required: ['agent_type', 'agent_count'],
    },
  },
  {
    name: 'run_command_in_terminals',
    description: 'Run a CLI command inside one or more ALREADY OPEN shell/terminal panes.',
    input_schema: {
      type: 'object' as const,
      properties: {
        pane_ids: {
          type: 'array',
          items: { type: 'string' },
          description:
            'Array of string IDs of the panes (from the Currently Running Terminals list).',
        },
        command: {
          type: 'string',
          description: 'The command string to execute in the shells.',
        },
      },
      required: ['pane_ids', 'command'],
    },
  },
  {
    name: 'launch_custom_agent',
    description: "Launch one of the user's custom-defined agents using a direct CLI command.",
    input_schema: {
      type: 'object' as const,
      properties: {
        command: {
          type: 'string',
          description: 'The exact CLI command of the custom agent to launch.',
        },
        agent_count: {
          type: 'number',
          description: 'The number of custom agents to spawn.',
        },
      },
      required: ['command', 'agent_count'],
    },
  },
  {
    name: 'read_agent_output',
    description:
      'Read the captured terminal output from a specific agent pane. Use this to see what an agent has been doing, check for errors, or read results.',
    input_schema: {
      type: 'object' as const,
      properties: {
        pane_id: {
          type: 'string',
          description: 'The pane ID of the agent to read output from.',
        },
        limit: {
          type: 'number',
          description: 'Maximum number of lines to return. Defaults to 100.',
        },
        since_line: {
          type: 'number',
          description: 'Only return lines after this line number (for pagination).',
        },
        since_time: {
          type: 'number',
          description: 'Only return lines after this Unix ms timestamp.',
        },
      },
      required: ['pane_id'],
    },
  },
  {
    name: 'list_agents',
    description:
      'List all currently running agent panes with their IDs, types, line counts, and last activity timestamps. Use this to discover which agents are available to monitor.',
    input_schema: {
      type: 'object' as const,
      properties: {},
    },
  },
  {
    name: 'check_agent_status',
    description:
      'Check the current status of a specific agent by its pane or agent ID. Returns connection status, last activity time, output line count, and whether the agent is waiting for input.',
    input_schema: {
      type: 'object' as const,
      properties: {
        agent_id: {
          type: 'string',
          description: 'The agent or pane ID to check status for.',
        },
      },
      required: ['agent_id'],
    },
  },
  {
    name: 'create_execution_plan',
    description:
      'Create a structured execution plan before dispatching any agents. You MUST call this tool before launching agents for any non-trivial task. Each step must have a DISTINCT task_prompt tailored to what that specific agent should do. Never give the same prompt to multiple agents.',
    input_schema: {
      type: 'object' as const,
      properties: {
        goal: {
          type: 'string',
          description: 'The high-level goal this plan achieves.',
        },
        reasoning: {
          type: 'string',
          description: 'Your reasoning for why this plan structure was chosen.',
        },
        steps: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              id: { type: 'string', description: 'Unique step ID, e.g. "step-1"' },
              title: { type: 'string', description: 'Short title for this step' },
              description: { type: 'string', description: 'What this step accomplishes' },
              agent_type: {
                type: 'string',
                enum: ['claude', 'codex', 'opencode', 'gemini', 'shell'],
                description: 'Type of agent to use for this step',
              },
              task_prompt: {
                type: 'string',
                description:
                  'The SPECIFIC, DETAILED prompt for this agent. Must be unique to this step.',
              },
              depends_on: {
                type: 'array',
                items: { type: 'string' },
                description: 'Step IDs that must complete before this step can start.',
              },
            },
            required: ['id', 'title', 'description', 'agent_type', 'task_prompt'],
          },
        },
      },
      required: ['goal', 'reasoning', 'steps'],
    },
  },
  {
    name: 'dispatch_plan_step',
    description:
      'Launch an agent to execute a specific step from the active execution plan. The agent receives the step-specific task_prompt. Use this instead of launch_builtin_agent when executing a plan.',
    input_schema: {
      type: 'object' as const,
      properties: {
        step_id: {
          type: 'string',
          description: 'The step ID from the execution plan to dispatch.',
        },
      },
      required: ['step_id'],
    },
  },
  {
    name: 'prompt_agent',
    description:
      'Send a specific prompt or instruction to an already-running agent pane. Use this to give follow-up instructions, ask clarifying questions, or re-direct an agent.',
    input_schema: {
      type: 'object' as const,
      properties: {
        pane_id: {
          type: 'string',
          description: 'The pane ID of the running agent.',
        },
        prompt: {
          type: 'string',
          description: 'The prompt or instruction to send to the agent.',
        },
      },
      required: ['pane_id', 'prompt'],
    },
  },
  {
    name: 'ask_user',
    description:
      'Ask the user a clarifying question with selectable options. Use this when you need user input to proceed — choosing between approaches, confirming scope, selecting preferences. The user clicks an option and you immediately continue.',
    input_schema: {
      type: 'object' as const,
      properties: {
        question: { type: 'string', description: 'The question to ask.' },
        options: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              label: { type: 'string', description: 'Short option label (1-5 words)' },
              description: { type: 'string', description: 'What this option means' },
            },
            required: ['label', 'description'],
          },
          description: 'Available choices (2-5 options). User can also type a custom response.',
        },
      },
      required: ['question', 'options'],
    },
  },
  {
    name: 'evaluate_results',
    description:
      'Evaluate the results of an execution plan. Read agent outputs for each completed step and assess whether the goal was achieved. This tool records the evaluation and determines next action.',
    input_schema: {
      type: 'object' as const,
      properties: {
        plan_id: { type: 'string', description: 'The plan ID to evaluate.' },
        overall_status: {
          type: 'string',
          enum: ['success', 'partial_success', 'failure', 'needs_replanning'],
          description: 'Your assessment of the overall plan outcome.',
        },
        step_evaluations: {
          type: 'array',
          items: {
            type: 'object',
            properties: {
              step_id: { type: 'string' },
              status: { type: 'string', enum: ['success', 'failure', 'incomplete'] },
              summary: { type: 'string' },
            },
            required: ['step_id', 'status', 'summary'],
          },
        },
        next_action: {
          type: 'string',
          enum: ['done', 'replan', 'retry_steps', 'escalate_to_user'],
          description: 'What to do next based on the evaluation.',
        },
        reasoning: {
          type: 'string',
          description: 'Your reasoning for this evaluation.',
        },
      },
      required: ['plan_id', 'overall_status', 'step_evaluations', 'next_action', 'reasoning'],
    },
  },
]

export function toOpenAITools() {
  return ORCHESTRATOR_TOOLS.map((t) => ({
    type: 'function' as const,
    function: {
      name: t.name,
      description: t.description,
      parameters: {
        type: 'object',
        properties: t.input_schema.properties,
        required: t.input_schema.required,
      },
    },
  }))
}

function getWindow(): BrowserWindow | null {
  return BrowserWindow.getAllWindows()[0] ?? null
}

function shellEscape(arg: string): string {
  return "'" + arg.replace(/'/g, "'\\''") + "'"
}

function buildAgentCommand(agentType: string, taskPrompt?: string): string {
  let baseCmd = 'claude'
  if (agentType === 'codex') baseCmd = 'codex'
  if (agentType === 'opencode') baseCmd = 'opencode'
  if (agentType === 'gemini') baseCmd = 'gemini'
  if (agentType === 'shell') return ''

  if (!taskPrompt) return baseCmd
  return `${baseCmd} -p ${shellEscape(taskPrompt)}`
}

export async function executeToolCall(name: string, args: ToolInput): Promise<ToolCallResult> {
  const win = getWindow()
  if (!win) {
    return { text: 'Failed: No window available.' }
  }

  switch (name) {
    case 'launch_builtin_agent': {
      const { task_prompt, agent_type = 'claude', agent_count = 1 } = args
      const agentCommand = buildAgentCommand(agent_type, task_prompt)
      const promises = []
      for (let i = 0; i < agent_count; i++) {
        const id = `agent-${randomUUID()}`
        promises.push(
          new Promise<void>((resolve) => {
            ipcMain.once(`athena:agent-spawned:ack:${id}`, () => resolve())
            win.webContents.send('athena:agent-spawned', {
              id,
              agentType: agent_type,
              agentCmd: agentCommand,
            })
          }),
        )
      }
      await Promise.all(promises)
      return { text: `Done, launched ${agent_count} ${agent_type} agents.` }
    }

    case 'launch_custom_agent': {
      const { command, agent_count = 1 } = args
      const promises = []
      for (let i = 0; i < agent_count; i++) {
        const id = `custom-agent-${randomUUID()}`
        promises.push(
          new Promise<void>((resolve) => {
            ipcMain.once(`athena:agent-spawned:ack:${id}`, () => resolve())
            win.webContents.send('athena:agent-spawned', {
              id,
              agentType: 'custom',
              agentCmd: command,
            })
          }),
        )
      }
      await Promise.all(promises)
      return { text: `Done, launched ${agent_count} custom agents.` }
    }

    case 'close_terminals': {
      const { pane_ids } = args
      if (Array.isArray(pane_ids)) {
        await new Promise<void>((resolve) => {
          ipcMain.once('athena:close-panes:ack', () => resolve())
          win.webContents.send('athena:close-panes', pane_ids)
        })
      }
      return { text: `Closed ${pane_ids?.length ?? 0} terminal(s).` }
    }

    case 'run_command_in_terminals': {
      const { pane_ids, command } = args
      if (Array.isArray(pane_ids) && command) {
        pane_ids.forEach((id) => {
          ptyWrite(id, command)
          setTimeout(() => ptyWrite(id, '\r'), 150)
        })
      }
      return { text: `Sent command to ${pane_ids?.length ?? 0} terminal(s).` }
    }

    case 'read_agent_output': {
      const { getOutput } = await import('./services/output-buffer-service')
      const lines = getOutput(args.pane_id!, {
        limit: args.limit || 100,
        sinceLine: args.since_line,
        sinceTime: args.since_time,
      })
      if (lines.length === 0) {
        return {
          text: `No output captured for pane '${args.pane_id}'. The pane may not exist or has not produced output yet.`,
        }
      }
      const formatted = lines.map((l) => `[${l.lineNum}] ${l.text}`).join('\n')
      return { text: formatted }
    }

    case 'list_agents': {
      const { getAgentList } = await import('./services/output-buffer-service')
      const { getAgentSessions } = await import('./services/agent-comms')
      const panes = getAgentList()
      const sessions = getAgentSessions()

      if (panes.length === 0 && sessions.length === 0) {
        return { text: 'No agents currently running.' }
      }

      const parts: string[] = []
      if (panes.length > 0) {
        parts.push('Terminal Panes:')
        for (const p of panes) {
          parts.push(
            `  ${p.paneId} (${p.agentType}) — ${p.lineCount} lines, last activity: ${new Date(p.lastActivityAt).toISOString()}`,
          )
        }
      }
      if (sessions.length > 0) {
        parts.push('Agent Sessions:')
        for (const s of sessions) {
          parts.push(
            `  ${s.agentId} [${s.status}] — plugin: ${s.pluginId}, connected: ${new Date(s.connectedAt).toISOString()}`,
          )
        }
      }
      return { text: parts.join('\n') }
    }

    case 'check_agent_status': {
      const { getPaneBufferInfo } = await import('./services/output-buffer-service')
      const { getAgentSessions } = await import('./services/agent-comms')
      const agentId = args.agent_id!

      const paneInfo = getPaneBufferInfo(agentId)
      const sessions = getAgentSessions()
      const session = sessions.find((s) => s.agentId === agentId || s.id === agentId)

      if (!paneInfo && !session) {
        return { text: `No agent found with ID '${agentId}'.` }
      }

      const parts: string[] = []
      if (paneInfo) {
        parts.push(`Pane: ${paneInfo.paneId}`)
        parts.push(`Type: ${paneInfo.agentType}`)
        parts.push(`Lines: ${paneInfo.lineCount} (${paneInfo.totalLines} total)`)
        parts.push(`Size: ${paneInfo.totalBytes} bytes`)
        parts.push(`Created: ${new Date(paneInfo.createdAt).toISOString()}`)
        parts.push(`Last Activity: ${new Date(paneInfo.lastActivityAt).toISOString()}`)
        const isActive = Date.now() - paneInfo.lastActivityAt < 30_000
        parts.push(`Status: ${isActive ? 'active' : 'idle'}`)
      }
      if (session) {
        parts.push(`Session: ${session.id}`)
        parts.push(`Agent ID: ${session.agentId}`)
        parts.push(`Connection Status: ${session.status}`)
        parts.push(`Connected: ${new Date(session.connectedAt).toISOString()}`)
      }
      const ptyConnected = hasSession(agentId)
      parts.push(`PTY Connected: ${ptyConnected}`)

      return { text: parts.join('\n') }
    }

    case 'create_execution_plan': {
      const { setActivePlan } = await import('./services/plan-manager')
      const plan = setActivePlan({
        goal: args.goal!,
        reasoning: args.reasoning!,
        steps: (args.steps || []).map((s: any) => ({
          id: s.id,
          title: s.title,
          description: s.description,
          agent_type: s.agent_type,
          task_prompt: s.task_prompt,
          depends_on: s.depends_on || [],
          status: 'pending' as const,
        })),
      })

      win.webContents.send('athena:planUpdate', plan)

      const stepSummary = plan.steps
        .map((s) => {
          const deps = s.depends_on.length > 0 ? ` (after: ${s.depends_on.join(', ')})` : ''
          return `  ${s.id}: [${s.agent_type}] ${s.title}${deps}`
        })
        .join('\n')

      return { text: `Plan created (${plan.id}):\nGoal: ${plan.goal}\nSteps:\n${stepSummary}` }
    }

    case 'dispatch_plan_step': {
      const { getActivePlan, updateStepStatus } = await import('./services/plan-manager')
      const plan = getActivePlan()
      if (!plan)
        return { text: 'No active execution plan. Create one first with create_execution_plan.' }

      const step = plan.steps.find((s) => s.id === args.step_id)
      if (!step) return { text: `Step '${args.step_id}' not found in active plan.` }

      const unmetDeps = step.depends_on.filter((depId) => {
        const dep = plan.steps.find((s) => s.id === depId)
        return dep && dep.status !== 'completed'
      })
      if (unmetDeps.length > 0) {
        return { text: `Cannot dispatch: dependencies not met: ${unmetDeps.join(', ')}` }
      }

      const agentCommand = buildAgentCommand(step.agent_type, step.task_prompt)
      const paneId = `plan-${plan.id}-${step.id}-${randomUUID().slice(0, 8)}`

      await new Promise<void>((resolve) => {
        ipcMain.once(`athena:agent-spawned:ack:${paneId}`, () => resolve())
        win.webContents.send('athena:agent-spawned', {
          id: paneId,
          agentType: step.agent_type,
          agentCmd: agentCommand,
        })
      })

      updateStepStatus(step.id, 'in_progress', paneId)
      win.webContents.send('athena:planUpdate', getActivePlan())

      return { text: `Dispatched step '${step.id}' (${step.title}) → pane ${paneId}` }
    }

    case 'prompt_agent': {
      const { pane_id, prompt } = args
      if (!pane_id || !prompt) return { text: 'Missing pane_id or prompt.' }
      if (!hasSession(pane_id)) return { text: `No active PTY session for pane '${pane_id}'.` }
      ptyWrite(pane_id, prompt)
      setTimeout(() => ptyWrite(pane_id, '\r'), 150)
      return { text: `Prompt sent to ${pane_id}.` }
    }

    case 'ask_user': {
      const requestId = randomUUID()
      return new Promise<ToolCallResult>((resolve) => {
        win.webContents.send('athena:askUser', {
          requestId,
          question: args.question,
          options: args.options,
        })
        ipcMain.once(`athena:userAnswer:${requestId}`, (_event: any, answer: string) => {
          resolve({ text: `User selected: ${answer}` })
        })
      })
    }

    case 'evaluate_results': {
      const { getActivePlan, updateStepStatus, updatePlanStatus } =
        await import('./services/plan-manager')
      const plan = getActivePlan()
      if (!plan) return { text: 'No active execution plan to evaluate.' }

      if (args.step_evaluations) {
        for (const evalItem of args.step_evaluations) {
          updateStepStatus(evalItem.step_id, evalItem.status === 'success' ? 'completed' : 'failed')
          const step = plan.steps.find((s) => s.id === evalItem.step_id)
          if (step) step.result_summary = evalItem.summary
        }
      }

      const statusMap: Record<string, 'completed' | 'failed'> = {
        success: 'completed',
        partial_success: 'completed',
        failure: 'failed',
        needs_replanning: 'failed',
      }
      updatePlanStatus(statusMap[args.overall_status!] || 'completed')

      const updatedPlan = getActivePlan()
      win.webContents.send('athena:planUpdate', updatedPlan)
      win.webContents.send('athena:planEvaluated', {
        planId: plan.id,
        overallStatus: args.overall_status,
        stepEvaluations: args.step_evaluations,
        nextAction: args.next_action,
        reasoning: args.reasoning,
      })

      const actionInstructions: Record<string, string> = {
        done: 'Plan complete. Report results to the user.',
        replan: 'Create a new execution plan addressing the failures.',
        retry_steps: 'Re-dispatch the failed steps.',
        escalate_to_user: 'Ask the user for guidance on how to proceed.',
      }

      return {
        text: `Evaluation recorded. Overall: ${args.overall_status}. Next: ${actionInstructions[args.next_action!] || args.next_action}`,
      }
    }

    default:
      return { text: `Unknown tool: ${name}` }
  }
}
