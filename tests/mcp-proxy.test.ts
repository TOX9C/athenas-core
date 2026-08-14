import { once } from 'node:events'
import { createServer, type Socket } from 'node:net'
import { spawn, type ChildProcessWithoutNullStreams } from 'node:child_process'
import { fileURLToPath } from 'node:url'
import path from 'node:path'
import { describe, expect, it } from 'vitest'

const proxyPath = fileURLToPath(new URL('../bin/mcp-proxy.js', import.meta.url))

function readLine(stream: NodeJS.ReadableStream): Promise<string> {
  return new Promise((resolve, reject) => {
    let buffer = ''
    const onData = (chunk: Buffer | string) => {
      buffer += chunk.toString()
      const newline = buffer.indexOf('\n')
      if (newline < 0) return
      cleanup()
      resolve(buffer.slice(0, newline))
    }
    const onError = (error: Error) => {
      cleanup()
      reject(error)
    }
    const onEnd = () => {
      cleanup()
      reject(new Error('stream ended before a complete line was received'))
    }
    const cleanup = () => {
      stream.off('data', onData)
      stream.off('error', onError)
      stream.off('end', onEnd)
      stream.off('close', onEnd)
    }
    stream.on('data', onData)
    stream.on('error', onError)
    stream.on('end', onEnd)
    stream.on('close', onEnd)
  })
}

async function waitForProxyConnection(server: ReturnType<typeof createServer>): Promise<Socket> {
  const [socket] = (await once(server, 'connection')) as [Socket]
  return socket
}

describe('mcp-proxy TCP bridge', () => {
  it('forwards line-delimited requests and responses unchanged', async () => {
    const server = createServer()
    server.listen(0, '127.0.0.1')
    await once(server, 'listening')
    const address = server.address()
    if (!address || typeof address === 'string') {
      throw new Error('test server did not expose an ephemeral port')
    }

    let proxy: ChildProcessWithoutNullStreams | undefined
    let socket: Socket | undefined
    try {
      proxy = spawn(process.execPath, [proxyPath], {
        cwd: path.dirname(proxyPath),
        env: {
          ...process.env,
          ATHENA_MCP_HOST: '127.0.0.1',
          ATHENA_MCP_PORT: String(address.port),
        },
        stdio: ['pipe', 'pipe', 'pipe'],
      })
      const proxyExit = once(proxy, 'exit')
      const proxyError = once(proxy, 'error')
      socket = await waitForProxyConnection(server)
      const request = '{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}\n'
      proxy.stdin.write(request)
      expect(await readLine(socket)).toBe(request.trim())

      const response = '{"jsonrpc":"2.0","id":1,"result":{"tools":[]}}\n'
      socket.write(response)
      expect(await readLine(proxy.stdout)).toBe(response.trim())

      socket.end()
      proxy.stdin.end()
      await Promise.race([proxyExit, proxyError])
    } finally {
      socket?.destroy()
      if (proxy && proxy.exitCode === null) {
        proxy.kill('SIGTERM')
        await once(proxy, 'exit').catch(() => undefined)
      }
      await new Promise<void>((resolve) => server.close(() => resolve()))
    }
  })

  it('exits nonzero when the configured MCP server is unreachable', async () => {
    const unavailable = createServer()
    unavailable.listen(0, '127.0.0.1')
    await once(unavailable, 'listening')
    const address = unavailable.address()
    if (!address || typeof address === 'string') {
      throw new Error('failure fixture did not expose an ephemeral port')
    }
    await new Promise<void>((resolve) => unavailable.close(() => resolve()))

    const proxy = spawn(process.execPath, [proxyPath], {
      env: {
        ...process.env,
        ATHENA_MCP_HOST: '127.0.0.1',
        ATHENA_MCP_PORT: String(address.port),
      },
      stdio: ['pipe', 'pipe', 'pipe'],
    })
    let stderr = ''
    proxy.stderr.on('data', (chunk) => {
      stderr += chunk.toString()
    })
    const [code] = (await once(proxy, 'exit')) as [number | null]
    expect(code).not.toBe(0)
    expect(stderr).toContain('MCP Proxy connection error:')
  })
})
