import { describe, it, expect, beforeAll, afterAll } from 'vitest'
import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { InMemoryTransport } from '@modelcontextprotocol/sdk/inMemory.js'
import { AthenaMcpServer } from '../packages/mcp-server/src/index.js'

// Drives the real AthenaMcpServer end-to-end over an in-memory transport:
// tool/resource registries, dispatch, and bridge-backed behavior as an MCP
// client actually observes them.

let athena: AthenaMcpServer
let client: Client

beforeAll(async () => {
  athena = new AthenaMcpServer({ transport: 'stdio' })
  const [clientTransport, serverTransport] = InMemoryTransport.createLinkedPair()
  await athena.getServer().connect(serverTransport)
  client = new Client({ name: 'mcp-tools-test', version: '0.0.0' })
  await client.connect(clientTransport)
})

afterAll(async () => {
  await client.close()
  await athena.stop()
})

describe('MCP Tools Integration', () => {
  describe('Tool Registry', () => {
    it('registers the full Athena tool surface', async () => {
      const { tools } = await client.listTools()
      const names = tools.map((t) => t.name)

      for (const expected of [
        'notify',
        'status_update',
        'request_input',
        'athena_notify',
        'athena_request_input',
        'athena_update_status',
        'athena_report_error',
        'athena_report_completion',
        'athena_read_output',
        'athena_stream_output',
        'athena_list_agents',
        'athena_get_output_since',
        'search_files',
      ]) {
        expect(names).toContain(expected)
      }
    })

    it('exposes input schemas for registered tools', async () => {
      const { tools } = await client.listTools()
      for (const tool of tools) {
        expect(tool.inputSchema).toBeDefined()
        expect(tool.inputSchema.type).toBe('object')
      }
    })
  })

  describe('Resource Registry', () => {
    it('lists the static athena:// resources', async () => {
      const { resources } = await client.listResources()
      const uris = resources.map((r) => r.uri)
      expect(uris).toContain('athena://agents')
      expect(uris).toContain('athena://app-state')
    })

    it('exposes the athena://agent/{id} resource template', async () => {
      const { resourceTemplates } = await client.listResourceTemplates()
      const templates = resourceTemplates.map((t) => t.uriTemplate)
      expect(templates).toContain('athena://agent/{id}')
    })
  })

  describe('notify tool', () => {
    it('delivers a notification including metadata and actions', async () => {
      const received: unknown[] = []
      const unsubscribe = athena.getBridge().onEvent((event, data) => {
        if (event === 'notification') received.push(data)
      })
      try {
        const result = await client.callTool({
          name: 'notify',
          arguments: {
            level: 'info',
            message: 'Task finished',
            metadata: { duration: 12 },
            actions: [{ id: 'open', label: 'Open' }],
          },
        })
        expect(result.isError).toBeFalsy()
        expect((result.content as Array<{ text: string }>)[0]!.text).toContain('delivered')

        expect(received).toHaveLength(1)
        const notification = received[0] as {
          message: string
          metadata?: Record<string, unknown>
          actions?: Array<{ id: string; label: string }>
        }
        expect(notification.message).toBe('Task finished')
        expect(notification.metadata).toEqual({ duration: 12 })
        expect(notification.actions).toEqual([{ id: 'open', label: 'Open' }])
      } finally {
        unsubscribe()
      }
    })
  })

  describe('status_update tool', () => {
    it('updates agent state with computed progress', async () => {
      await client.callTool({
        name: 'status_update',
        arguments: {
          status: 'working',
          agentId: 'agent-1',
          progress: { current: 1, total: 4 },
        },
      })
      const state = athena.getBridge().getAgentState('agent-1')
      expect(state?.status).toBe('running')
      expect(state?.progress).toBe(25)
    })

    it('rejects a zero total instead of dividing by it', async () => {
      const result = await client.callTool({
        name: 'status_update',
        arguments: {
          status: 'working',
          agentId: 'agent-1',
          progress: { current: 1, total: 0 },
        },
      })
      expect(result.isError).toBe(true)
    })
  })

  describe('request_input tool', () => {
    it('fails fast when the bridge is not connected', async () => {
      const result = await client.callTool({
        name: 'request_input',
        arguments: { prompt: 'Proceed?', timeoutMs: 1000 },
      })
      // Not connected: the bridge resolves immediately with
      // cancelled+timedOut, and the tool surfaces the timed-out error.
      expect(result.isError).toBe(true)
      const text = (result.content as Array<{ text: string }>)[0]!.text
      const parsed = JSON.parse(text)
      expect(parsed.error).toMatch(/timed out/)
    })

    it('rejects timeoutMs above the cap', async () => {
      const result = await client.callTool({
        name: 'request_input',
        arguments: { prompt: 'Proceed?', timeoutMs: 600_001 },
      })
      expect(result.isError).toBe(true)
    })
  })

  describe('athena_list_agents tool', () => {
    it('lists agents reported through status updates', async () => {
      const result = await client.callTool({ name: 'athena_list_agents', arguments: {} })
      const text = (result.content as Array<{ text: string }>)[0]!.text
      expect(text).toContain('agent-1')
    })
  })

  describe('athena://agents resource', () => {
    it('reads agent states forwarded through the bridge', async () => {
      const result = await client.readResource({ uri: 'athena://agents' })
      const text = result.contents[0]!.text
      const agents = JSON.parse(text) as Array<{ id: string }>
      expect(agents.some((a) => a.id === 'agent-1')).toBe(true)
    })
  })

  describe('athena://agent/{id} resource', () => {
    it('resolves a specific agent without a leading slash in the id', async () => {
      const result = await client.readResource({ uri: 'athena://agent/agent-1' })
      const state = JSON.parse(result.contents[0]!.text as string) as { id: string; error?: string }
      expect(state.id).toBe('agent-1')
      expect(state.error).toBeUndefined()
    })

    it('reports unknown agents', async () => {
      const result = await client.readResource({ uri: 'athena://agent/nope' })
      const body = JSON.parse(result.contents[0]!.text as string) as { error: string }
      expect(body.error).toMatch(/nope not found/)
    })
  })

  describe('athena://app-state resource', () => {
    it('returns the app-state snapshot shape', async () => {
      const result = await client.readResource({ uri: 'athena://app-state' })
      const state = JSON.parse(result.contents[0]!.text as string) as { agents: unknown[] }
      expect(Array.isArray(state.agents)).toBe(true)
    })
  })
})
