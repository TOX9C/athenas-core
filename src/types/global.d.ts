declare global {
  interface Window {
    athena: {
      pty: {
        spawn: (id: string, cwd: string, shell: string, agentCmd?: string) => Promise<any>
        write: (id: string, data: string) => void
        resize: (id: string, cols: number, rows: number) => void
        kill: (id: string) => void
        onData: (id: string, cb: (data: string) => void) => () => void
        onExit: (id: string, cb: (code: number) => void) => () => void
      }
      fs: {
        readTree: (dir: string) => Promise<any>
        readFile: (path: string) => Promise<string>
        writeFile: (path: string, content: string) => Promise<any>
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
        readState: (dir: string) => Promise<any>
        writeState: (dir: string, state: any) => Promise<void>
        sendMessage: (dir: string, from: string, to: string, msg: string) => Promise<void>
        readMailbox: (dir: string, agentId: string) => Promise<any[]>
        watchState: (dir: string, cb: (state: any) => void) => () => void
      }
      store: {
        get: (key: string) => Promise<any>
        set: (key: string, value: any) => Promise<void>
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
}

export {}
