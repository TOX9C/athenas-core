declare global {
  interface Window {
    athena: {
      pty: {
        spawn: (id: string, cwd: string, shell: string, agentCmd?: string) => Promise<{ success: boolean; error?: string }>
        write: (id: string, data: string) => void
        resize: (id: string, cols: number, rows: number) => void
        kill: (id: string) => void
        getHistory: (id: string) => Promise<string>
        hasSession: (id: string) => Promise<boolean>
        onAthenaClosePanes: (cb: (data: string[]) => void) => () => void
        onAthenaSpawn: (cb: (data: { id: string; agentType: string; agentCmd?: string }) => void) => () => void
        onData: (id: string, cb: (data: string) => void) => () => void
        onExit: (id: string, cb: (code: number) => void) => () => void
      }
      fs: {
        readTree: (dir: string) => Promise<FileTreeNode | { success: false; error: string }>
        readFile: (path: string) => Promise<string>
        writeFile: (path: string, content: string) => Promise<{ success: boolean; error?: string }>
        watchDir: (dir: string, cb: () => void) => () => void
        showOpenDialog: () => Promise<string | null>
        exists: (path: string) => Promise<boolean>
      }
      browser: {
        show: (bounds: { x: number; y: number; width: number; height: number }) => void
        hide: () => void
        navigate: (url: string) => void
        back: () => void
        forward: () => void
        reload: () => void
        onTitleChange: (cb: (title: string) => void) => () => void
        onUrlChange: (cb: (url: string) => void) => () => void
      }
      swarm: {
        readState: (dir: string) => Promise<SwarmState | null>
        writeState: (dir: string, state: SwarmState) => Promise<void>
        sendMessage: (dir: string, from: string, to: string, msg: string) => Promise<void>
        readMailbox: (dir: string, agentId: string) => Promise<SwarmMessage[]>
        watchState: (dir: string, cb: (state: SwarmState) => void) => () => void
      }
      store: {
        get: (key: string) => Promise<unknown>
        set: (key: string, value: unknown) => Promise<void>
      }
      orchestrator: {
        chat: (msg: string, activeSpaceId?: string) => Promise<string>
      }
      window: {
        minimize: () => void
        maximize: () => void
        close: () => void
        isMaximized: () => Promise<boolean>
        platform: () => Promise<string>
      }
    }
  }

  interface FileTreeNode {
    name: string
    path: string
    isDir: boolean
    children?: FileTreeNode[]
  }

  interface SwarmState {
    agents: SwarmAgent[]
    [key: string]: unknown
  }

  interface SwarmAgent {
    id: string
    status: string
    lastActionAt?: number
    [key: string]: unknown
  }

  interface SwarmMessage {
    id: string
    from: string
    to: string
    content: string
    timestamp: number
    read: boolean
  }
}

export {}
