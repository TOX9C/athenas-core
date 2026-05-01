import { describe, it, expect } from 'vitest'
import * as fs from 'fs'
import * as path from 'path'
import { fileURLToPath } from 'url'

const __filename = fileURLToPath(import.meta.url)
const __dirname = path.dirname(__filename)
const ROOT = path.resolve(__dirname, '..')

const PRELOAD_PATH = path.join(ROOT, 'electron/preload.ts')
const MAIN_PATH = path.join(ROOT, 'electron/main.ts')
const SERVICES_INDEX_PATH = path.join(ROOT, 'electron/services/index.ts')
const MCP_SERVER_PATH = path.join(ROOT, 'electron/mcpServer.ts')

describe('output reading integration wiring', () => {
  describe('preload output-capture namespace', () => {
    let preloadSrc: string

    beforeAll(() => {
      preloadSrc = fs.readFileSync(PRELOAD_PATH, 'utf8')
    })

    it('exposes outputCapture.read IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.invoke('output-capture:read'")
    })

    it('exposes outputCapture.listAgents IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.invoke('output-capture:list-agents')")
    })

    it('exposes outputCapture.subscribe IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.invoke('output-capture:subscribe'")
    })

    it('exposes outputCapture.getInfo IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.invoke('output-capture:getInfo'")
    })

    it('exposes outputCapture.clear IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.invoke('output-capture:clear'")
    })

    it('exposes outputCapture.unsubscribe IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.send('output-capture:unsubscribe'")
    })

    it('exposes outputCapture.registerPane IPC channel', () => {
      expect(preloadSrc).toContain("ipcRenderer.invoke('output-capture:registerPane'")
    })

    it('exposes output-capture:line event listener', () => {
      expect(preloadSrc).toContain("ipcRenderer.on('output-capture:line'")
    })

    it('exposes output-capture:paneRegistered event listener', () => {
      expect(preloadSrc).toContain("ipcRenderer.on('output-capture:paneRegistered'")
    })

    it('exposes output-capture:paneUnregistered event listener', () => {
      expect(preloadSrc).toContain("ipcRenderer.on('output-capture:paneUnregistered'")
    })
  })

  describe('main.ts wiring', () => {
    let mainSrc: string

    beforeAll(() => {
      mainSrc = fs.readFileSync(MAIN_PATH, 'utf8')
    })

    it('imports initOutputCapture from output-capture', () => {
      expect(mainSrc).toContain('initOutputCapture')
    })

    it('calls initOutputCapture with mainWindow', () => {
      expect(mainSrc).toMatch(/initOutputCapture\(mainWindow\)/)
    })

    it('sets output capture hooks on ptyManager', () => {
      expect(mainSrc).toContain('setOutputCaptureHooks')
      expect(mainSrc).toContain('onSpawn')
      expect(mainSrc).toContain('onData')
      expect(mainSrc).toContain('onExit')
    })

    it('calls shutdownOutputCapture on quit', () => {
      expect(mainSrc).toContain('shutdownOutputCapture')
    })
  })

  describe('services/index.ts barrel exports', () => {
    let servicesSrc: string

    beforeAll(() => {
      servicesSrc = fs.readFileSync(SERVICES_INDEX_PATH, 'utf8')
    })

    it('exports output-capture functions', () => {
      expect(servicesSrc).toContain("from './output-capture'")
      expect(servicesSrc).toContain('initOutputCapture')
      expect(servicesSrc).toContain('onPtySpawn')
      expect(servicesSrc).toContain('onPtyData')
      expect(servicesSrc).toContain('onPtyExit')
      expect(servicesSrc).toContain('captureStderr')
      expect(servicesSrc).toContain('shutdownOutputCapture')
    })

    it('exports output-buffer-service functions', () => {
      expect(servicesSrc).toContain("from './output-buffer-service'")
      expect(servicesSrc).toContain('initOutputBufferService')
      expect(servicesSrc).toContain('appendOutput')
      expect(servicesSrc).toContain('registerPane')
      expect(servicesSrc).toContain('unregisterPane')
      expect(servicesSrc).toContain('getOutput')
      expect(servicesSrc).toContain('getAgentList')
      expect(servicesSrc).toContain('shutdownOutputBufferService')
    })

    it('exports OutputLine type', () => {
      expect(servicesSrc).toContain('OutputLine')
    })
  })

  describe('mcpServer.ts output tools', () => {
    let mcpSrc: string

    beforeAll(() => {
      mcpSrc = fs.readFileSync(MCP_SERVER_PATH, 'utf8')
    })

    it('includes get_output tool in TOOLS array', () => {
      expect(mcpSrc).toContain("name: 'get_output'")
    })

    it('includes list_agent_panes tool in TOOLS array', () => {
      expect(mcpSrc).toContain("name: 'list_agent_panes'")
    })

    it('includes athena_forward_output tool in TOOLS array', () => {
      expect(mcpSrc).toContain("name: 'athena_forward_output'")
    })

    it('get_output dispatch imports from output-buffer-service', () => {
      expect(mcpSrc).toMatch(/get_output.*getOutput/s)
    })

    it('list_agent_panes dispatch imports getAgentList', () => {
      expect(mcpSrc).toMatch(/list_agent_panes.*getAgentList/s)
    })

    it('athena_forward_output dispatch sends plugin:event', () => {
      expect(mcpSrc).toContain('output_forwarded')
    })
  })
})
