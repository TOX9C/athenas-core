import { contextBridge, ipcRenderer } from 'electron'

contextBridge.exposeInMainWorld('athena', {
  orchestrator: {
    chat: (msg: string) => ipcRenderer.invoke('athena:chat', msg)
  },
  pty: {
    spawn: (id: string, cwd: string, shell: string, agentCmd?: string) =>
      ipcRenderer.invoke('pty:spawn', id, cwd, shell, agentCmd),
    write: (id: string, data: string) =>
      ipcRenderer.send('pty:write', id, data),
    resize: (id: string, cols: number, rows: number) =>
      ipcRenderer.send('pty:resize', id, cols, rows),
    kill: (id: string) =>
      ipcRenderer.send('pty:kill', id),
    getHistory: (id: string) =>
      ipcRenderer.invoke('pty:getHistory', id),
    hasSession: (id: string) =>
      ipcRenderer.invoke('pty:hasSession', id),
        onAthenaClosePanes: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on(`athena:close-panes`, handler)
      return () => ipcRenderer.removeListener(`athena:close-panes`, handler)
    },
    onAthenaSpawn: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on(`athena:agent-spawned`, handler)
      return () => ipcRenderer.removeListener(`athena:agent-spawned`, handler)
    },
    onData: (id: string, cb: (data: string) => void) => {
      const handler = (_event: any, data: string) => cb(data)
      ipcRenderer.on(`pty:data:${id}`, handler)
      return () => ipcRenderer.removeListener(`pty:data:${id}`, handler)
    },
    onExit: (id: string, cb: (code: number) => void) => {
      const handler = (_event: any, code: number) => cb(code)
      ipcRenderer.on(`pty:exit:${id}`, handler)
      return () => ipcRenderer.removeListener(`pty:exit:${id}`, handler)
    }
  },
  fs: {
    readTree: (dir: string) => ipcRenderer.invoke('fs:readTree', dir),
    readFile: (path: string) => ipcRenderer.invoke('fs:readFile', path),
    writeFile: (path: string, content: string) => ipcRenderer.invoke('fs:writeFile', path, content),
    watchDir: (dir: string, cb: () => void) => {
      ipcRenderer.send('fs:watchDir', dir)
      const handler = () => cb()
      ipcRenderer.on(`fs:change:${dir}`, handler)
      return () => {
        ipcRenderer.removeListener(`fs:change:${dir}`, handler)
        ipcRenderer.send('fs:unwatchDir', dir)
      }
    },
    showOpenDialog: () => ipcRenderer.invoke('fs:showOpenDialog'),
    exists: (path: string) => ipcRenderer.invoke('fs:exists', path)
  },
  browser: {
    show: (bounds: { x: number; y: number; width: number; height: number }) =>
      ipcRenderer.send('browser:show', bounds),
    hide: () => ipcRenderer.send('browser:hide'),
    navigate: (url: string) => ipcRenderer.send('browser:navigate', url),
    back: () => ipcRenderer.send('browser:back'),
    forward: () => ipcRenderer.send('browser:forward'),
    reload: () => ipcRenderer.send('browser:reload'),
    onTitleChange: (cb: (title: string) => void) => {
      const handler = (_event: any, title: string) => cb(title)
      ipcRenderer.on('browser:titleChange', handler)
      return () => ipcRenderer.removeListener('browser:titleChange', handler)
    },
    onUrlChange: (cb: (url: string) => void) => {
      const handler = (_event: any, url: string) => cb(url)
      ipcRenderer.on('browser:urlChange', handler)
      return () => ipcRenderer.removeListener('browser:urlChange', handler)
    }
  },
  swarm: {
    readState: (dir: string) => ipcRenderer.invoke('swarm:readState', dir),
    writeState: (dir: string, state: any) => ipcRenderer.invoke('swarm:writeState', dir, state),
    sendMessage: (dir: string, from: string, to: string, msg: string) =>
      ipcRenderer.invoke('swarm:sendMessage', dir, from, to, msg),
    readMailbox: (dir: string, agentId: string) =>
      ipcRenderer.invoke('swarm:readMailbox', dir, agentId),
    watchState: (dir: string, cb: (state: any) => void) => {
      const handler = (_event: any, state: any) => cb(state)
      ipcRenderer.on('swarm:stateChange', handler)
      return () => ipcRenderer.removeListener('swarm:stateChange', handler)
    }
  },
  store: {
    get: (key: string) => ipcRenderer.invoke('store:get', key),
    set: (key: string, value: any) => ipcRenderer.invoke('store:set', key, value)
  },
  window: {
    minimize: () => ipcRenderer.send('window:minimize'),
    maximize: () => ipcRenderer.send('window:maximize'),
    close: () => ipcRenderer.send('window:close'),
    isMaximized: () => ipcRenderer.invoke('window:isMaximized'),
    platform: () => ipcRenderer.invoke('window:platform')
  }
})
