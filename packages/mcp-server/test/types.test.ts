import { describe, it, expect } from 'vitest'
import type {
  AgentStatus,
  SpecAgentStatus,
  NotificationType,
  NotificationPriority,
  TransportType,
  AthenaNotification,
  InputRequest,
  InputResponse,
  StatusUpdate,
  ErrorReport,
  CompletionReport,
  AgentState,
  AthenaAppState,
  SpaceState,
  PaneState,
  TaskState,
  ServerConfig,
  PluginEvent,
  McpSession,
  PluginManifest,
  OutputEntry,
  OutputReadOptions,
  OutputSinceOptions,
  StreamSubscription,
  AgentListEntry,
} from '../src/types/index.js'

describe('MCP Type Definitions', () => {
  describe('AgentStatus', () => {
    it('accepts all valid status values', () => {
      const statuses: AgentStatus[] = [
        'running',
        'idle',
        'error',
        'waiting',
        'done',
        'blocked',
        'stalled',
      ]
      expect(statuses).toHaveLength(7)
    })
  })

  describe('SpecAgentStatus', () => {
    it('accepts all spec-compliant status values', () => {
      const statuses: SpecAgentStatus[] = [
        'idle',
        'thinking',
        'working',
        'waiting_for_input',
        'completed',
        'error',
        'cancelled',
      ]
      expect(statuses).toHaveLength(7)
    })
  })

  describe('NotificationType', () => {
    it('accepts all valid notification types', () => {
      const types: NotificationType[] = ['info', 'warning', 'error', 'success']
      expect(types).toHaveLength(4)
    })
  })

  describe('NotificationPriority', () => {
    it('accepts all valid priority levels', () => {
      const priorities: NotificationPriority[] = ['low', 'normal', 'high', 'critical']
      expect(priorities).toHaveLength(4)
    })
  })

  describe('TransportType', () => {
    it('accepts stdio, websocket, and tcp', () => {
      const transports: TransportType[] = ['stdio', 'websocket', 'tcp']
      expect(transports).toHaveLength(3)
    })
  })

  describe('AthenaNotification', () => {
    it('constructs a valid notification', () => {
      const n: AthenaNotification = {
        type: 'info',
        title: 'Test',
        message: 'Test message',
        priority: 'normal',
      }
      expect(n.type).toBe('info')
      expect(n.priority).toBe('normal')
      expect(n.agentId).toBeUndefined()
    })

    it('supports optional metadata and actions', () => {
      const n: AthenaNotification = {
        type: 'error',
        title: 'Agent Error',
        message: 'Agent failed',
        priority: 'critical',
        agentId: 'agent-1',
        timestamp: Date.now(),
        metadata: { taskId: 'abc' },
        actions: [{ id: 'retry', label: 'Retry' }],
      }
      expect(n.metadata).toEqual({ taskId: 'abc' })
      expect(n.actions).toHaveLength(1)
    })
  })

  describe('InputRequest', () => {
    it('constructs with required fields only', () => {
      const req: InputRequest = { prompt: 'Enter value:' }
      expect(req.prompt).toBe('Enter value:')
      expect(req.defaultResponse).toBeUndefined()
    })

    it('supports all optional fields', () => {
      const req: InputRequest = {
        prompt: 'Confirm?',
        defaultResponse: 'y',
        timeout: 30000,
        agentId: 'agent-2',
      }
      expect(req.defaultResponse).toBe('y')
      expect(req.timeout).toBe(30000)
    })
  })

  describe('InputResponse', () => {
    it('constructs a successful response', () => {
      const res: InputResponse = { value: 'yes', cancelled: false, timedOut: false }
      expect(res.value).toBe('yes')
    })

    it('constructs a cancelled response', () => {
      const res: InputResponse = { value: '', cancelled: true, timedOut: false }
      expect(res.cancelled).toBe(true)
    })

    it('constructs a timed-out response', () => {
      const res: InputResponse = { value: '', cancelled: false, timedOut: true }
      expect(res.timedOut).toBe(true)
    })
  })

  describe('StatusUpdate', () => {
    it('constructs with required fields', () => {
      const upd: StatusUpdate = { agentId: 'a1', status: 'running' }
      expect(upd.agentId).toBe('a1')
      expect(upd.message).toBeUndefined()
    })

    it('supports optional details', () => {
      const upd: StatusUpdate = {
        agentId: 'a1',
        status: 'error',
        message: 'Crashed',
        progress: 75,
        details: { reason: 'oom' },
      }
      expect(upd.details).toEqual({ reason: 'oom' })
    })
  })

  describe('ErrorReport', () => {
    it('constructs a minimal error report', () => {
      const err: ErrorReport = { agentId: 'a1', error: 'Something broke', recoverable: false }
      expect(err.recoverable).toBe(false)
    })

    it('supports full error details', () => {
      const err: ErrorReport = {
        agentId: 'a1',
        error: 'Fatal',
        stack: 'Error: Fatal\n at ...',
        code: 500,
        recoverable: false,
        context: { attempt: 3 },
      }
      expect(err.code).toBe(500)
      expect(err.context).toEqual({ attempt: 3 })
    })
  })

  describe('CompletionReport', () => {
    it('constructs a minimal completion', () => {
      const comp: CompletionReport = { agentId: 'a1', summary: 'Done' }
      expect(comp.artifacts).toBeUndefined()
    })

    it('supports all optional fields', () => {
      const comp: CompletionReport = {
        agentId: 'a1',
        summary: 'Completed all tasks',
        artifacts: ['output.txt'],
        metrics: { tasksCompleted: 5 },
        duration: 120,
      }
      expect(comp.artifacts).toHaveLength(1)
      expect(comp.duration).toBe(120)
    })
  })

  describe('AgentState', () => {
    it('constructs with required fields', () => {
      const state: AgentState = { id: 'a1', type: 'claude', status: 'idle' }
      expect(state.role).toBeUndefined()
    })
  })

  describe('AthenaAppState', () => {
    it('constructs a full app state', () => {
      const state: AthenaAppState = {
        activeSpaceId: 'space-1',
        spaces: [],
        theme: 'dark',
        activePanel: 'terminal',
        agents: [],
        tasks: [],
      }
      expect(state.activeSpaceId).toBe('space-1')
    })
  })

  describe('ServerConfig', () => {
    it('constructs with required fields', () => {
      const config: ServerConfig = {
        name: 'athena-mcp',
        version: '1.0.0',
        transport: 'stdio',
      }
      expect(config.websocketPort).toBeUndefined()
      expect(config.tcpPort).toBeUndefined()
    })

    it('supports all transport types', () => {
      const configs: ServerConfig[] = [
        { name: 'a', version: '1.0.0', transport: 'stdio' },
        { name: 'a', version: '1.0.0', transport: 'websocket', websocketPort: 8765 },
        { name: 'a', version: '1.0.0', transport: 'tcp', tcpPort: 4545 },
      ]
      expect(configs).toHaveLength(3)
    })
  })

  describe('PluginEvent', () => {
    it('constructs a notification event', () => {
      const event: PluginEvent = {
        id: 'evt-1',
        type: 'notification',
        source: { sessionId: 's1', paneId: null, agentType: 'claude', agentId: null },
        payload: { level: 'info', message: 'Hello' },
        timestamp: Date.now(),
      }
      expect(event.type).toBe('notification')
    })
  })

  describe('McpSession', () => {
    it('constructs a session', () => {
      const session: McpSession = {
        sessionId: 'sess-1',
        token: 'tok-1',
        paneId: 'pane-1',
        agentType: 'claude',
        capabilities: ['notifications', 'status'],
        connectedAt: Date.now(),
        lastActivityAt: Date.now(),
      }
      expect(session.capabilities).toContain('notifications')
    })
  })

  describe('PluginManifest', () => {
    it('constructs a builtin manifest', () => {
      const manifest: PluginManifest = {
        id: 'com.athena.core',
        name: 'Athena Core Plugin',
        version: '1.0.0',
        description: 'Core plugin',
        author: 'Athena',
        minAthenaVersion: '0.1.0',
        capabilities: ['notifications', 'status'],
        tools: [],
        install: { type: 'builtin' },
      }
      expect(manifest.install.type).toBe('builtin')
    })

    it('constructs an mcp_server install manifest', () => {
      const manifest: PluginManifest = {
        id: 'com.example.plugin',
        name: 'Example',
        version: '1.0.0',
        description: 'Example plugin',
        author: 'Example',
        minAthenaVersion: '0.1.0',
        capabilities: ['notifications'],
        tools: [],
        install: { type: 'mcp_server', command: 'node', args: ['server.js'], env: { KEY: 'val' } },
      }
      expect(manifest.install.type).toBe('mcp_server')
    })
  })

  describe('SpaceState, PaneState, TaskState', () => {
    it('constructs SpaceState with panes', () => {
      const pane: PaneState = { id: 'p1', agentType: 'claude', label: 'Agent 1', status: 'running' }
      const space: SpaceState = { id: 's1', name: 'Workspace', cwd: '/home', panes: [pane] }
      expect(space.panes).toHaveLength(1)
    })

    it('constructs TaskState', () => {
      const task: TaskState = {
        id: 't1',
        title: 'Build feature',
        status: 'in_progress',
        description: 'Do the thing',
      }
      expect(task.description).toBe('Do the thing')
      expect(task.spaceId).toBeUndefined()
    })
  })

  describe('OutputEntry', () => {
    it('constructs a full output entry', () => {
      const entry: OutputEntry = {
        timestamp: Date.now(),
        lineNumber: 42,
        content: 'build complete',
        isStderr: false,
      }
      expect(entry.lineNumber).toBe(42)
      expect(entry.isStderr).toBe(false)
    })

    it('constructs a stderr entry', () => {
      const entry: OutputEntry = {
        timestamp: Date.now(),
        lineNumber: 1,
        content: 'error: module not found',
        isStderr: true,
      }
      expect(entry.isStderr).toBe(true)
    })
  })

  describe('OutputReadOptions', () => {
    it('constructs with optional fields', () => {
      const opts: OutputReadOptions = { lines: 50, sinceTimestamp: 1000 }
      expect(opts.lines).toBe(50)
    })

    it('allows empty options', () => {
      const opts: OutputReadOptions = {}
      expect(opts.lines).toBeUndefined()
    })
  })

  describe('OutputSinceOptions', () => {
    it('constructs with sinceLine', () => {
      const opts: OutputSinceOptions = { sinceLine: 100 }
      expect(opts.sinceLine).toBe(100)
    })

    it('constructs with sinceTimestamp', () => {
      const opts: OutputSinceOptions = { sinceTimestamp: Date.now() }
      expect(opts.sinceTimestamp).toBeDefined()
    })
  })

  describe('StreamSubscription', () => {
    it('constructs a subscription', () => {
      const sub: StreamSubscription = {
        id: 'sub-1',
        paneId: 'p1',
        onChunk: () => {},
        active: true,
      }
      expect(sub.active).toBe(true)
    })
  })

  describe('AgentListEntry', () => {
    it('constructs a minimal entry', () => {
      const entry: AgentListEntry = {
        paneId: 'p1',
        agentType: 'claude',
        status: 'idle',
      }
      expect(entry.label).toBeUndefined()
    })

    it('constructs a full entry', () => {
      const entry: AgentListEntry = {
        paneId: 'p1',
        agentType: 'claude',
        status: 'running',
        label: 'Builder',
        lastActivityAt: Date.now(),
      }
      expect(entry.label).toBe('Builder')
    })
  })
})
