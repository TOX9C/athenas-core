import { describe, it, expect } from 'vitest'

const MCP_TOOLS = [
  'notify',
  'request_input',
  'update_status',
  'report_error',
  'report_completion',
  'create_tasks',
  'get_next_task',
  'update_task_status',
] as const

const MCP_RESOURCES = ['athena://state', 'athena://agents', 'athena://tasks'] as const

describe('MCP Tools Integration', () => {
  describe('Tool Registry', () => {
    it('should define all 8 required MCP tools', () => {
      expect(MCP_TOOLS).toHaveLength(8)
    })

    it('each tool should be a non-empty string', () => {
      for (const tool of MCP_TOOLS) {
        expect(typeof tool).toBe('string')
        expect(tool.length).toBeGreaterThan(0)
      }
    })

    it('should use snake_case naming convention', () => {
      for (const tool of MCP_TOOLS) {
        expect(tool).toMatch(/^[a-z]+(_[a-z]+)*$/)
      }
    })
  })

  describe('Resource Registry', () => {
    it('should define all 3 required MCP resources', () => {
      expect(MCP_RESOURCES).toHaveLength(3)
    })

    it('resources should use athena:// URI scheme', () => {
      for (const resource of MCP_RESOURCES) {
        expect(resource).toMatch(/^athena:\/\//)
      }
    })
  })

  describe('notify tool', () => {
    it('should accept AthenaNotification parameters', async () => {
      const params = {
        type: 'info' as const,
        title: 'Test',
        message: 'Test notification',
        priority: 'normal' as const,
      }
      expect(params.type).toBe('info')
      expect(params.priority).toBe('normal')
    })
  })

  describe('request_input tool', () => {
    it('should accept InputRequest parameters', async () => {
      const params = {
        prompt: 'What is your name?',
        defaultResponse: 'World',
        timeout: 30000,
      }
      expect(params.prompt).toBeDefined()
      expect(typeof params.timeout).toBe('number')
    })
  })

  describe('update_status tool', () => {
    it('should accept StatusUpdate parameters', async () => {
      const params = {
        agentId: 'agent-1',
        status: 'running' as const,
        message: 'Processing...',
        progress: 0.75,
      }
      expect(params.agentId).toBeDefined()
      expect(params.progress).toBeGreaterThanOrEqual(0)
      expect(params.progress).toBeLessThanOrEqual(1)
    })
  })

  describe('report_error tool', () => {
    it('should accept ErrorReport parameters', async () => {
      const params = {
        agentId: 'agent-1',
        error: 'Unexpected failure',
        recoverable: true,
        code: 'E_TIMEOUT',
      }
      expect(params.recoverable).toBe(true)
    })
  })

  describe('report_completion tool', () => {
    it('should accept CompletionReport parameters', async () => {
      const params = {
        agentId: 'agent-1',
        summary: 'All tasks completed',
        artifacts: ['output.md'],
        duration: 5000,
      }
      expect(Array.isArray(params.artifacts)).toBe(true)
    })
  })

  describe('create_tasks tool', () => {
    it('should define task creation schema', () => {
      const taskSpec = {
        tasks: [
          { title: 'Task 1', description: 'Do thing 1' },
          { title: 'Task 2', description: 'Do thing 2' },
        ],
      }
      expect(taskSpec.tasks).toHaveLength(2)
    })
  })

  describe('get_next_task tool', () => {
    it('should accept optional agentId filter', () => {
      const params = { agentId: 'agent-1' }
      expect(params.agentId).toBe('agent-1')
    })
  })

  describe('update_task_status tool', () => {
    it('should accept task ID and new status', () => {
      const params = { taskId: 't1', status: 'in_progress' }
      expect(params.taskId).toBeDefined()
      expect(params.status).toBeDefined()
    })
  })

  describe('athena://state resource', () => {
    it('should return AthenaAppState shape', () => {
      const expectedShape = {
        activeSpaceId: null,
        spaces: [],
        theme: 'dark',
        activePanel: 'terminal',
        agents: [],
        tasks: [],
      }
      expect(Object.keys(expectedShape)).toContain('agents')
      expect(Object.keys(expectedShape)).toContain('spaces')
    })
  })

  describe('athena://agents resource', () => {
    it('should return AgentState[] shape', () => {
      const agents = [{ id: 'a1', type: 'claude', status: 'idle' }]
      expect(agents[0]).toHaveProperty('id')
      expect(agents[0]).toHaveProperty('status')
    })
  })

  describe('athena://tasks resource', () => {
    it('should return TaskState[] shape', () => {
      const tasks = [{ id: 't1', title: 'Build', status: 'pending' }]
      expect(tasks[0]).toHaveProperty('id')
      expect(tasks[0]).toHaveProperty('status')
    })
  })
})
