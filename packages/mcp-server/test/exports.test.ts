import { describe, it, expect } from 'vitest'

describe('Package exports', () => {
  it('exports AthenaMcpServer', async () => {
    const mod = await import('../src/index.js')
    expect(mod.AthenaMcpServer).toBeDefined()
    expect(typeof mod.AthenaMcpServer).toBe('function')
  })

  it('exports AthenaBridge', async () => {
    const mod = await import('../src/index.js')
    expect(mod.AthenaBridge).toBeDefined()
    expect(typeof mod.AthenaBridge).toBe('function')
  })

  it('exports tool functions', async () => {
    const mod = await import('../src/index.js')
    expect(typeof mod.notify).toBe('function')
    expect(typeof mod.statusUpdate).toBe('function')
    expect(typeof mod.requestInput).toBe('function')
    expect(typeof mod.athenaNotify).toBe('function')
    expect(typeof mod.athenaUpdateStatus).toBe('function')
    expect(typeof mod.athenaReportError).toBe('function')
    expect(typeof mod.athenaReportCompletion).toBe('function')
    expect(typeof mod.athenaReadOutput).toBe('function')
    expect(typeof mod.athenaStreamOutput).toBe('function')
    expect(typeof mod.athenaListAgents).toBe('function')
  })

  it('exports tool schemas', async () => {
    const mod = await import('../src/index.js')
    expect(mod.notifySchema).toBeDefined()
    expect(mod.statusUpdateSchema).toBeDefined()
    expect(mod.requestInputSchema).toBeDefined()
    expect(mod.athenaNotifySchema).toBeDefined()
    expect(mod.athenaUpdateStatusSchema).toBeDefined()
    expect(mod.athenaReportErrorSchema).toBeDefined()
    expect(mod.athenaReportCompletionSchema).toBeDefined()
  })

  it('exports transport classes', async () => {
    const mod = await import('../src/index.js')
    expect(mod.WebSocketTransport).toBeDefined()
    expect(mod.TcpTransport).toBeDefined()
    expect(typeof mod.connectStdio).toBe('function')
  })

  it('exports type definitions', async () => {
    const mod = await import('../src/index.js')
    // Type-only exports don't appear at runtime, but the module should load without error
    expect(mod).toBeDefined()
  })

  it('exports types subpath', async () => {
    const mod = await import('../src/types/index.js')
    expect(mod).toBeDefined()
  })

  it('exports tools subpath', async () => {
    const mod = await import('../src/tools/index.js')
    expect(mod.notify).toBeDefined()
    expect(mod.athenaNotify).toBeDefined()
  })

  it('exports transport subpath', async () => {
    const mod = await import('../src/transport/index.js')
    expect(mod.WebSocketTransport).toBeDefined()
    expect(mod.TcpTransport).toBeDefined()
  })
})

describe('ServerConfig type', () => {
  it('allows all transport types', async () => {
    const { AthenaMcpServer } = await import('../src/index.js')
    const s1 = new AthenaMcpServer({ transport: 'stdio' })
    const s2 = new AthenaMcpServer({ transport: 'websocket' })
    const s3 = new AthenaMcpServer({ transport: 'tcp' })
    expect(s1).toBeDefined()
    expect(s2).toBeDefined()
    expect(s3).toBeDefined()
  })

  it('provides access to underlying MCP server and bridge', async () => {
    const { AthenaMcpServer } = await import('../src/index.js')
    const server = new AthenaMcpServer()
    expect(server.getServer()).toBeDefined()
    expect(server.getBridge()).toBeDefined()
  })

  it('provides access to output buffer manager', async () => {
    const { AthenaMcpServer } = await import('../src/index.js')
    const server = new AthenaMcpServer()
    expect(server.getOutputBuffer()).toBeDefined()
  })

  it('exports OutputBufferManager', async () => {
    const mod = await import('../src/index.js')
    expect(mod.OutputBufferManager).toBeDefined()
    expect(typeof mod.OutputBufferManager).toBe('function')
  })

  it('exports output tool functions', async () => {
    const mod = await import('../src/index.js')
    expect(typeof mod.athenaReadOutput).toBe('function')
    expect(typeof mod.athenaStreamOutput).toBe('function')
    expect(typeof mod.athenaListAgents).toBe('function')
    expect(typeof mod.athenaGetOutputSince).toBe('function')
  })

  it('exports output tool schemas', async () => {
    const mod = await import('../src/index.js')
    expect(mod.athenaReadOutputSchema).toBeDefined()
    expect(mod.athenaStreamOutputSchema).toBeDefined()
    expect(mod.athenaListAgentsSchema).toBeDefined()
    expect(mod.athenaGetOutputSinceSchema).toBeDefined()
  })
})
