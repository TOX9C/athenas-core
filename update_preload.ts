import { readFileSync, writeFileSync } from 'fs'

const path = 'electron/preload.ts'
let content = readFileSync(path, 'utf-8')

content = content.replace(
  "getCwd: (id: string) =>\n      ipcRenderer.invoke('pty:getCwd', id),",
  `getCwd: (id: string) =>\n      ipcRenderer.invoke('pty:getCwd', id),
    ackClosePanes: (data: any) => ipcRenderer.send('athena:close-panes:ack', data),
    ackAgentSpawned: (id: string, data: any) => ipcRenderer.send(\`athena:agent-spawned:ack:\${id}\`, data),
    onAthenaStatus: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('athena:status', handler)
      return () => ipcRenderer.removeListener('athena:status', handler)
    },`
)

writeFileSync(path, content)
