import { contextBridge, ipcRenderer } from 'electron'

contextBridge.exposeInMainWorld('athena', {
  orchestrator: {
    chat: (msg: string) => ipcRenderer.invoke('athena:chat', msg),
    chatWithSession: (msg: string, sessionId: string) =>
      ipcRenderer.invoke('athena:chat', msg, sessionId),
    chatWithImages: (msg: string, images: any[], sessionId: string) =>
      ipcRenderer.invoke('athena:chat', msg, sessionId, images),
  },
  pty: {
    spawn: (id: string, cwd: string, shell: string, agentCmd?: string) =>
      ipcRenderer.invoke('pty:spawn', id, cwd, shell, agentCmd),
    spawnAgent: (
      id: string,
      cwd: string,
      shell: string,
      agentCmd: string | undefined,
      agentType: string,
      paneId?: string,
      sessionId?: string,
    ) =>
      ipcRenderer.invoke('pty:spawnAgent', id, cwd, shell, agentCmd, agentType, paneId, sessionId),
    write: (id: string, data: string) => ipcRenderer.send('pty:write', id, data),
    resize: (id: string, cols: number, rows: number) =>
      ipcRenderer.send('pty:resize', id, cols, rows),
    kill: (id: string) => ipcRenderer.send('pty:kill', id),
    getHistory: (id: string) => ipcRenderer.invoke('pty:getHistory', id),
    hasSession: (id: string) => ipcRenderer.invoke('pty:hasSession', id),
    getCwd: (id: string) => ipcRenderer.invoke('pty:getCwd', id),
    ackClosePanes: (data: any) => ipcRenderer.send('athena:close-panes:ack', data),
    ackAgentSpawned: (id: string, data: any) =>
      ipcRenderer.send(`athena:agent-spawned:ack:${id}`, data),
    onAthenaStatus: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('athena:status', handler)
      return () => ipcRenderer.removeListener('athena:status', handler)
    },
    onAskUser: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('athena:askUser', handler)
      return () => ipcRenderer.removeListener('athena:askUser', handler)
    },
    answerUser: (requestId: string, answer: string) => {
      ipcRenderer.send(`athena:userAnswer:${requestId}`, answer)
    },
    onPlanUpdate: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('athena:planUpdate', handler)
      return () => ipcRenderer.removeListener('athena:planUpdate', handler)
    },
    onPlanEvaluated: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('athena:planEvaluated', handler)
      return () => ipcRenderer.removeListener('athena:planEvaluated', handler)
    },
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
    onReady: (id: string, cb: () => void) => {
      const handler = () => cb()
      ipcRenderer.on(`pty:ready:${id}`, handler)
      return () => ipcRenderer.removeListener(`pty:ready:${id}`, handler)
    },
    onExit: (id: string, cb: (code: number) => void) => {
      const handler = (_event: any, code: number) => cb(code)
      ipcRenderer.on(`pty:exit:${id}`, handler)
      return () => ipcRenderer.removeListener(`pty:exit:${id}`, handler)
    },
    onShellIntegration: (id: string, cb: (event: ShellIntegrationEvent) => void) => {
      const handler = (_event: any, data: ShellIntegrationEvent) => cb(data)
      ipcRenderer.on(`shell-integration:${id}`, handler)
      return () => ipcRenderer.removeListener(`shell-integration:${id}`, handler)
    },
    onCwdChanged: (cb: (data: { paneId: string; cwd: string; timestamp: number }) => void) => {
      const handler = (_event: any, data: { paneId: string; cwd: string; timestamp: number }) =>
        cb(data)
      ipcRenderer.on('shell-cwd-changed', handler)
      return () => ipcRenderer.removeListener('shell-cwd-changed', handler)
    },
    onCommandStarted: (
      cb: (data: { paneId: string; command: string; cwd?: string; timestamp: number }) => void,
    ) => {
      const handler = (
        _event: any,
        data: { paneId: string; command: string; cwd?: string; timestamp: number },
      ) => cb(data)
      ipcRenderer.on('shell-command-started', handler)
      return () => ipcRenderer.removeListener('shell-command-started', handler)
    },
    onCommandExited: (
      cb: (data: {
        paneId: string
        command: string
        exitCode: number
        cwd?: string
        duration?: number
        timestamp: number
      }) => void,
    ) => {
      const handler = (
        _event: any,
        data: {
          paneId: string
          command: string
          exitCode: number
          cwd?: string
          duration?: number
          timestamp: number
        },
      ) => cb(data)
      ipcRenderer.on('shell-command-exited', handler)
      return () => ipcRenderer.removeListener('shell-command-exited', handler)
    },
  },
  fs: {
    showOpenDialog: () => ipcRenderer.invoke('fs:showOpenDialog'),
    showImageDialog: () => ipcRenderer.invoke('fs:showImageDialog'),
    readFileAsBase64: (path: string) => ipcRenderer.invoke('fs:readFileAsBase64', path),
    exists: (path: string) => ipcRenderer.invoke('fs:exists', path),
    search: (options: {
      pattern: string
      path: string
      glob?: string
      type?: string
      caseSensitive?: boolean
      maxResults?: number
      contextLines?: number
    }) => ipcRenderer.invoke('fs:search', options),
    searchFiles: (
      directory: string,
      pattern: string,
      options?: { glob?: string; type?: string; maxResults?: number },
    ) => ipcRenderer.invoke('fs:searchFiles', directory, pattern, options),
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
    },
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
    },
  },
  store: {
    get: (key: string) => ipcRenderer.invoke('store:get', key),
    set: (key: string, value: any) => ipcRenderer.invoke('store:set', key, value),
    onUpdateTasks: (cb: () => void) => {
      const handler = () => cb()
      ipcRenderer.on('store:updateTasks', handler)
      return () => ipcRenderer.removeListener('store:updateTasks', handler)
    },
  },
  plugin: {
    list: () => ipcRenderer.invoke('plugin:list'),
    get: (pluginId: string) => ipcRenderer.invoke('plugin:get', pluginId),
    register: (manifest: any) => ipcRenderer.invoke('plugin:register', manifest),
    unregister: (pluginId: string) => ipcRenderer.invoke('plugin:unregister', pluginId),
    enable: (pluginId: string) => ipcRenderer.invoke('plugin:enable', pluginId),
    disable: (pluginId: string) => ipcRenderer.invoke('plugin:disable', pluginId),
    getConfig: (pluginId: string) => ipcRenderer.invoke('plugin:getConfig', pluginId),
    setConfig: (pluginId: string, config: Record<string, unknown>) =>
      ipcRenderer.invoke('plugin:setConfig', pluginId, config),
    setError: (pluginId: string, error: string) =>
      ipcRenderer.invoke('plugin:setError', pluginId, error),
    onRegistryUpdated: (cb: (registry: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:registryUpdated', handler)
      return () => ipcRenderer.removeListener('plugin:registryUpdated', handler)
    },
    onRegistered: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:registered', handler)
      return () => ipcRenderer.removeListener('plugin:registered', handler)
    },
    onEnabled: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:enabled', handler)
      return () => ipcRenderer.removeListener('plugin:enabled', handler)
    },
    onDisabled: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:disabled', handler)
      return () => ipcRenderer.removeListener('plugin:disabled', handler)
    },
    onError: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:error', handler)
      return () => ipcRenderer.removeListener('plugin:error', handler)
    },
    onConfigured: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:configured', handler)
      return () => ipcRenderer.removeListener('plugin:configured', handler)
    },
    onEvent: (cb: (event: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('plugin:event', handler)
      return () => ipcRenderer.removeListener('plugin:event', handler)
    },
    respondToInput: (requestId: string, response: string) => {
      ipcRenderer.send('plugin:respondToInput', requestId, response)
    },
  },
  window: {
    minimize: () => ipcRenderer.send('window:minimize'),
    maximize: () => ipcRenderer.send('window:maximize'),
    close: () => ipcRenderer.send('window:close'),
    isMaximized: () => ipcRenderer.invoke('window:isMaximized'),
    platform: () => ipcRenderer.invoke('window:platform'),
  },
  agents: {
    list: () => ipcRenderer.invoke('agents:list'),
    getStatus: (agentId: string) => ipcRenderer.invoke('agents:getStatus', agentId),
    respondInput: (requestId: string, response: string) =>
      ipcRenderer.invoke('agents:respondInput', requestId, response),
    cancelInput: (requestId: string) => ipcRenderer.invoke('agents:cancelInput', requestId),
    sendMessage: (agentId: string, method: string, params: Record<string, unknown>) =>
      ipcRenderer.invoke('agents:sendMessage', agentId, method, params),
    disconnect: (agentId: string) => ipcRenderer.invoke('agents:disconnect', agentId),
    getToken: () => ipcRenderer.invoke('agents:getToken'),
    getPort: () => ipcRenderer.invoke('agents:getPort'),
    onConnected: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('agents:connected', handler)
      return () => ipcRenderer.removeListener('agents:connected', handler)
    },
    onDisconnected: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('agents:disconnected', handler)
      return () => ipcRenderer.removeListener('agents:disconnected', handler)
    },
    onStatusUpdate: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('agents:statusUpdate', handler)
      return () => ipcRenderer.removeListener('agents:statusUpdate', handler)
    },
    onInputRequested: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('agents:inputRequested', handler)
      return () => ipcRenderer.removeListener('agents:inputRequested', handler)
    },
  },
  notifications: {
    history: (options?: { limit?: number; unreadOnly?: boolean; type?: string; source?: string }) =>
      ipcRenderer.invoke('notifications:history', options),
    getCount: () => ipcRenderer.invoke('notifications:getCount'),
    markRead: (id: string) => ipcRenderer.invoke('notifications:markRead', id),
    markAllRead: () => ipcRenderer.invoke('notifications:markAllRead'),
    dismiss: (id: string) => ipcRenderer.invoke('notifications:dismiss', id),
    clearAll: () => ipcRenderer.invoke('notifications:clearAll'),
    push: (event: any) => ipcRenderer.invoke('notifications:push', event),
    onNew: (cb: (notification: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('notifications:new', handler)
      return () => ipcRenderer.removeListener('notifications:new', handler)
    },
    onUpdated: (cb: (notification: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('notifications:updated', handler)
      return () => ipcRenderer.removeListener('notifications:updated', handler)
    },
    onDismissed: (cb: (data: { id: string }) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('notifications:dismissed', handler)
      return () => ipcRenderer.removeListener('notifications:dismissed', handler)
    },
    onAllRead: (cb: (data: { count: number }) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('notifications:allRead', handler)
      return () => ipcRenderer.removeListener('notifications:allRead', handler)
    },
    onCleared: (cb: (data: { count: number }) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('notifications:cleared', handler)
      return () => ipcRenderer.removeListener('notifications:cleared', handler)
    },
    onClicked: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('notifications:clicked', handler)
      return () => ipcRenderer.removeListener('notifications:clicked', handler)
    },
  },
  pluginHost: {
    listSessions: () => ipcRenderer.invoke('pluginHost:listSessions'),
    getSession: (sessionId: string) => ipcRenderer.invoke('pluginHost:getSession', sessionId),
    emitEvent: (event: any) => ipcRenderer.invoke('pluginHost:emitEvent', event),
    subscribe: (sessionId: string, eventTypes: string[]) =>
      ipcRenderer.invoke('pluginHost:subscribe', sessionId, eventTypes),
    updateStatus: (sessionId: string, status: string, data?: Record<string, unknown>) =>
      ipcRenderer.invoke('pluginHost:updateStatus', sessionId, status, data),
    unregisterSession: (sessionId: string) =>
      ipcRenderer.invoke('pluginHost:unregisterSession', sessionId),
    discoverPlugins: (projectRoot?: string) =>
      ipcRenderer.invoke('pluginHost:discoverPlugins', projectRoot),
    setupPlugin: (
      agentType: string,
      options: { token: string; projectRoot?: string; global?: boolean },
    ) => ipcRenderer.invoke('pluginHost:setupPlugin', agentType, options),
    removePlugin: (agentType: string, projectRoot?: string) =>
      ipcRenderer.invoke('pluginHost:removePlugin', agentType, projectRoot),
    onSessionRegistered: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('pluginHost:sessionRegistered', handler)
      return () => ipcRenderer.removeListener('pluginHost:sessionRegistered', handler)
    },
    onSessionRemoved: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('pluginHost:sessionRemoved', handler)
      return () => ipcRenderer.removeListener('pluginHost:sessionRemoved', handler)
    },
    onSessionStatusUpdate: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('pluginHost:sessionStatusUpdate', handler)
      return () => ipcRenderer.removeListener('pluginHost:sessionStatusUpdate', handler)
    },
    onPluginEvent: (cb: (data: any) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('pluginHost:pluginEvent', handler)
      return () => ipcRenderer.removeListener('pluginHost:pluginEvent', handler)
    },
  },
  outputCapture: {
    read: (
      paneId: string,
      options?: { limit?: number; offset?: number; sinceLine?: number; sinceTime?: number },
    ) => ipcRenderer.invoke('output-capture:read', paneId, options),
    listAgents: () => ipcRenderer.invoke('output-capture:list-agents'),
    getInfo: (paneId: string) => ipcRenderer.invoke('output-capture:getInfo', paneId),
    clear: (paneId: string) => ipcRenderer.invoke('output-capture:clear', paneId),
    subscribe: (paneId: string) => ipcRenderer.invoke('output-capture:subscribe', paneId),
    unsubscribe: (subscriptionId: string) =>
      ipcRenderer.send('output-capture:unsubscribe', subscriptionId),
    registerPane: (paneId: string, agentType?: string) =>
      ipcRenderer.invoke('output-capture:registerPane', paneId, agentType),
    onLine: (cb: (data: { subscriptionId: string; line: any }) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('output-capture:line', handler)
      return () => ipcRenderer.removeListener('output-capture:line', handler)
    },
    onPaneRegistered: (cb: (data: { paneId: string; agentType: string }) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('output-capture:paneRegistered', handler)
      return () => ipcRenderer.removeListener('output-capture:paneRegistered', handler)
    },
    onPaneUnregistered: (cb: (data: { paneId: string }) => void) => {
      const handler = (_event: any, data: any) => cb(data)
      ipcRenderer.on('output-capture:paneUnregistered', handler)
      return () => ipcRenderer.removeListener('output-capture:paneUnregistered', handler)
    },
  },
  session: {
    create: (title?: string) => ipcRenderer.invoke('session:create', title),
    get: (id: string) => ipcRenderer.invoke('session:get', id),
    update: (id: string, updates: any) => ipcRenderer.invoke('session:update', id, updates),
    addMessage: (sessionId: string, message: any) =>
      ipcRenderer.invoke('session:addMessage', sessionId, message),
    delete: (id: string) => ipcRenderer.invoke('session:delete', id),
    list: () => ipcRenderer.invoke('session:list'),
  },
  search: {
    ripgrep: (options: {
      pattern: string
      path: string
      glob?: string
      type?: string
      caseSensitive?: boolean
      maxResults?: number
      contextLines?: number
    }) => ipcRenderer.invoke('search:ripgrep', options),
  },
})
