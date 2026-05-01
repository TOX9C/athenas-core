import type { SwarmState, MailboxMessage } from './swarm'
import type { PluginEvent, PluginManifest } from './plugin'
import type {
  ShellIntegrationEvent,
  ShellCwdChangedEvent,
  ShellCommandStartedEvent,
  ShellCommandExitedEvent,
} from './terminal'

declare global {
  interface Window {
    athena: {
      pty: {
        spawn: (
          id: string,
          cwd: string,
          shell: string,
          agentCmd?: string,
        ) => Promise<{ success: boolean; error?: string }>
        write: (id: string, data: string) => void
        resize: (id: string, cols: number, rows: number) => void
        kill: (id: string) => void
        getHistory: (id: string) => Promise<string>
        hasSession: (id: string) => Promise<boolean>
        getCwd: (id: string) => Promise<string | null>
        onAthenaClosePanes: (cb: (data: string[]) => void) => () => void
        onAthenaSpawn: (
          cb: (data: { id: string; agentType: string; agentCmd?: string }) => void,
        ) => () => void
        onData: (id: string, cb: (data: string) => void) => () => void
        onExit: (id: string, cb: (code: number) => void) => () => void
        ackClosePanes: (data: { success: boolean; error?: string }) => void
        ackAgentSpawned: (id: string) => void
        onAthenaStatus: (cb: (data: StatusLogItem) => void) => () => void
        onReady: (id: string, cb: () => void) => () => void
        onShellIntegration: (id: string, cb: (event: ShellIntegrationEvent) => void) => () => void
        onCwdChanged: (cb: (data: ShellCwdChangedEvent) => void) => () => void
        onCommandStarted: (cb: (data: ShellCommandStartedEvent) => void) => () => void
        onCommandExited: (cb: (data: ShellCommandExitedEvent) => void) => () => void
      }
      fs: {
        readTree: (dir: string) => Promise<FileTreeNode[]>
        readFile: (path: string) => Promise<string>
        writeFile: (path: string, content: string) => Promise<{ success: boolean; error?: string }>
        watchDir: (dir: string, cb: () => void) => () => void
        showOpenDialog: () => Promise<string | null>
        showImageDialog: () => Promise<string[] | null>
        readFileAsBase64: (
          path: string,
        ) => Promise<{ data: string | null; mediaType: string | null; error?: string }>
        exists: (path: string) => Promise<boolean>
        getDirectories: (dir: string) => Promise<string[]>
        search: (
          options: CodeSearchOptions,
        ) => Promise<CodeSearchResult | { success: false; error: string }>
        searchFiles: (
          directory: string,
          pattern: string,
          options?: { glob?: string; type?: string; maxResults?: number },
        ) => Promise<string[] | { success: false; error: string }>
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
        writeState: (
          dir: string,
          state: SwarmState,
        ) => Promise<{ success: boolean; error?: string }>
        sendMessage: (
          dir: string,
          from: string,
          to: string,
          msg: string,
        ) => Promise<{ success: boolean; error?: string }>
        readMailbox: (dir: string, agentId: string) => Promise<MailboxMessage[]>
        watchState: (dir: string, cb: (state: SwarmState) => void) => () => void
      }
      store: {
        get: (key: string) => Promise<unknown>
        set: (key: string, value: unknown) => Promise<void>
        onUpdateTasks: (cb: () => void) => () => void
      }
      orchestrator: {
        chat: (msg: string, spaceId?: string) => Promise<string>
        chatWithSession: (msg: string, sessionId: string) => Promise<string>
        chatWithImages: (
          msg: string,
          images: Array<{ base64: string; mediaType: string }>,
          sessionId: string,
        ) => Promise<string>
      }
      plugin: {
        onEvent: (cb: (event: PluginEvent) => void) => () => void
        respondToInput: (requestId: string, response: string) => void
        list: () => Promise<Record<string, PluginRegistryEntry>>
        get: (pluginId: string) => Promise<any | null>
        register: (manifest: any) => Promise<{ success: boolean; id?: string; error?: string }>
        unregister: (pluginId: string) => Promise<{ success: boolean; error?: string }>
        enable: (pluginId: string) => Promise<{ success: boolean; error?: string }>
        disable: (pluginId: string) => Promise<{ success: boolean; error?: string }>
        getConfig: (pluginId: string) => Promise<Record<string, unknown> | null>
        setConfig: (
          pluginId: string,
          config: Record<string, unknown>,
        ) => Promise<{ success: boolean; error?: string }>
        setError: (pluginId: string, error: string) => Promise<{ success: boolean; error?: string }>
        onRegistryUpdated: (
          cb: (registry: Record<string, PluginRegistryEntry>) => void,
        ) => () => void
        onRegistered: (cb: (data: any) => void) => () => void
        onEnabled: (cb: (data: any) => void) => () => void
        onDisabled: (cb: (data: any) => void) => () => void
        onError: (cb: (data: any) => void) => () => void
        onConfigured: (cb: (data: any) => void) => () => void
      }
      agents: {
        list: () => Promise<AgentSessionInfo[]>
        getStatus: (agentId: string) => Promise<AgentStatusInfo | null>
        respondInput: (
          requestId: string,
          response: string,
        ) => Promise<{ success: boolean; error?: string }>
        cancelInput: (requestId: string) => Promise<{ success: boolean; error?: string }>
        sendMessage: (
          agentId: string,
          method: string,
          params: Record<string, unknown>,
        ) => Promise<{ success: boolean; messageId?: string; error?: string }>
        disconnect: (agentId: string) => Promise<{ success: boolean; error?: string }>
        getToken: () => Promise<string>
        getPort: () => Promise<number>
        onConnected: (cb: (data: AgentConnectedEvent) => void) => () => void
        onDisconnected: (cb: (data: AgentDisconnectedEvent) => void) => () => void
        onStatusUpdate: (cb: (data: AgentStatusUpdateEvent) => void) => () => void
        onInputRequested: (cb: (data: AgentInputRequestEvent) => void) => () => void
      }
      notifications: {
        history: (options?: NotificationHistoryOptions) => Promise<NotificationRecord[]>
        getCount: () => Promise<NotificationCountInfo>
        markRead: (id: string) => Promise<{ success: boolean; error?: string }>
        markAllRead: () => Promise<{ success: boolean; count: number }>
        dismiss: (id: string) => Promise<{ success: boolean; error?: string }>
        clearAll: () => Promise<{ success: boolean; count: number }>
        push: (event: NotificationEventInput) => Promise<{ success: boolean; id: string }>
        onNew: (cb: (notification: NotificationRecord) => void) => () => void
        onUpdated: (cb: (notification: NotificationRecord) => void) => () => void
        onDismissed: (cb: (data: { id: string }) => void) => () => void
        onAllRead: (cb: (data: { count: number }) => void) => () => void
        onCleared: (cb: (data: { count: number }) => void) => () => void
        onClicked: (cb: (data: any) => void) => () => void
      }
      outputCapture: {
        read: (paneId: string, options?: OutputCaptureReadOptions) => Promise<OutputCaptureLine[]>
        listAgents: () => Promise<OutputCaptureAgentInfo[]>
        getInfo: (paneId: string) => Promise<OutputCapturePaneInfo | null>
        clear: (paneId: string) => Promise<boolean>
        subscribe: (paneId: string) => Promise<{ subscriptionId: string }>
        unsubscribe: (subscriptionId: string) => void
        registerPane: (paneId: string, agentType?: string) => Promise<{ success: boolean }>
        onLine: (
          cb: (data: { subscriptionId: string; line: OutputCaptureLine }) => void,
        ) => () => void
        onPaneRegistered: (cb: (data: { paneId: string; agentType: string }) => void) => () => void
        onPaneUnregistered: (cb: (data: { paneId: string }) => void) => () => void
      }
      window: {
        minimize: () => void
        maximize: () => void
        close: () => void
        isMaximized: () => Promise<boolean>
        platform: () => Promise<string>
      }
      session: {
        create: (title?: string) => Promise<ChatSession>
        get: (id: string) => Promise<ChatSession | null>
        update: (
          id: string,
          updates: { title?: string; messages?: ChatSessionMessage[] },
        ) => Promise<ChatSession | null>
        addMessage: (sessionId: string, message: ChatSessionMessage) => Promise<ChatSession | null>
        delete: (id: string) => Promise<boolean>
        list: () => Promise<SessionListItem[]>
      }
    }
  }

  interface FileTreeNode {
    name: string
    path: string
    isDirectory: boolean
    children?: FileTreeNode[]
  }

  interface StatusLogItem {
    id: string
    status: string
    message?: string
    [key: string]: any
  }

  type PluginStatus = 'installed' | 'enabled' | 'disabled' | 'error'

  interface PluginRegistryEntry {
    name: string
    version: string
    status: PluginStatus
    description: string
    author: string
    config: Record<string, unknown>
    error?: string
  }

  interface AgentSessionInfo {
    id: string
    pluginId: string
    agentId: string
    connectedAt: number
    lastActivityAt: number
    status: 'active' | 'idle' | 'waiting_input' | 'disconnected'
  }

  interface AgentStatusInfo {
    id: string
    pluginId: string
    agentId: string
    status: 'active' | 'idle' | 'waiting_input' | 'disconnected'
    connectedAt: number
    lastActivityAt: number
  }

  interface AgentConnectedEvent {
    sessionId: string
    pluginId: string
    agentId: string
    connectedAt: number
  }

  interface AgentDisconnectedEvent {
    sessionId: string
    agentId: string
    pluginId: string
  }

  interface AgentStatusUpdateEvent {
    sessionId: string
    agentId: string
    status: string
    data?: Record<string, unknown>
  }

  interface AgentInputRequestEvent {
    sessionId: string
    agentId: string
    requestId: string
    prompt: string
  }

  type NotificationType = 'info' | 'warning' | 'error' | 'success' | 'needs_input'

  interface NotificationEventInput {
    type: NotificationType
    title: string
    message: string
    source: string
    agentId?: string
    data?: Record<string, unknown>
    metadata?: Record<string, unknown>
    actions?: Array<{ id: string; label: string }>
    requestId?: string
    timestamp: number
  }

  interface NotificationRecord extends NotificationEventInput {
    id: string
    read: boolean
    dismissedAt?: number
  }

  interface NotificationHistoryOptions {
    limit?: number
    unreadOnly?: boolean
    type?: NotificationType
    source?: string
  }

  interface NotificationCountInfo {
    total: number
    unread: number
    byType: Record<NotificationType, number>
  }

  interface OutputCaptureLine {
    paneId: string
    lineNum: number
    timestamp: number
    text: string
  }

  interface OutputCaptureReadOptions {
    limit?: number
    offset?: number
    sinceLine?: number
    sinceTime?: number
  }

  interface OutputCaptureAgentInfo {
    paneId: string
    agentType: string
    lineCount: number
    createdAt: number
    lastActivityAt: number
  }

  interface OutputCapturePaneInfo {
    paneId: string
    agentType: string
    lineCount: number
    totalLines: number
    totalBytes: number
    createdAt: number
    lastActivityAt: number
  }

  interface IPCImageData {
    base64: string
    mediaType: 'image/jpeg' | 'image/png' | 'image/gif' | 'image/webp'
  }

  interface ImageAttachment {
    id: string
    base64: string
    mediaType: 'image/jpeg' | 'image/png' | 'image/gif' | 'image/webp'
    name?: string
  }

  interface ChatSessionMessage {
    id: string
    role: 'user' | 'athena'
    content: string
    timestamp: number
    isError?: boolean
    images?: ImageAttachment[]
  }

  interface ChatSession {
    id: string
    title: string
    createdAt: number
    updatedAt: number
    messages: ChatSessionMessage[]
  }

  interface SessionListItem {
    id: string
    title: string
    createdAt: number
    updatedAt: number
    messageCount: number
    lastMessagePreview: string
  }

  interface CodeSearchOptions {
    pattern: string
    path: string
    glob?: string
    type?: string
    caseSensitive?: boolean
    maxResults?: number
    contextLines?: number
  }

  interface CodeSearchMatch {
    filePath: string
    lineNumber: number
    column: number
    lineText: string
    matchText: string
    contextBefore: string[]
    contextAfter: string[]
  }

  interface CodeSearchResult {
    matches: CodeSearchMatch[]
    truncated: boolean
    stats: {
      filesMatched: number
      totalMatches: number
    }
  }
}

declare module '@xterm/xterm/css/xterm.css' {
  const content: Record<string, string>
  export default content
}

export {}
