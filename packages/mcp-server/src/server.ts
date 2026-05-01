import { McpServer } from '@modelcontextprotocol/sdk/server/mcp.js'
import { AthenaBridge } from './bridge.js'
import { OutputBufferManager } from './output-buffer.js'
import { registerResources } from './resources/index.js'
import {
  notify,
  notifySchema,
  statusUpdate,
  statusUpdateSchema,
  requestInput,
  requestInputSchema,
  athenaNotify,
  athenaNotifySchema,
  athenaRequestInput,
  athenaRequestInputSchema,
  athenaUpdateStatus,
  athenaUpdateStatusSchema,
  athenaReportError,
  athenaReportErrorSchema,
  athenaReportCompletion,
  athenaReportCompletionSchema,
  controlPause,
  controlPauseSchema,
  controlResume,
  controlResumeSchema,
  controlCancel,
  controlCancelSchema,
  athenaReadOutput,
  athenaReadOutputSchema,
  athenaStreamOutput,
  athenaStreamOutputSchema,
  athenaListAgents,
  athenaListAgentsSchema,
  athenaGetOutputSince,
  athenaGetOutputSinceSchema,
  searchFiles,
  searchFilesSchema,
} from './tools/index.js'
import type { ServerConfig } from './types/index.js'
import { connectStdio } from './transport/stdio-transport.js'
import { WebSocketTransport } from './transport/websocket-transport.js'
import { TcpTransport } from './transport/tcp-transport.js'

const DEFAULT_CONFIG: ServerConfig = {
  name: 'athena-mcp-server',
  version: '1.0.0',
  transport: 'stdio',
  websocketPort: 4546,
  tcpPort: 4545,
  athenaHost: '127.0.0.1',
  athenaPort: 4545,
}

export class AthenaMcpServer {
  private server: McpServer
  private bridge: AthenaBridge
  private outputBuffer: OutputBufferManager
  private wsTransport: WebSocketTransport | null = null
  private tcpTransport: TcpTransport | null = null
  private config: ServerConfig

  constructor(config: Partial<ServerConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config }

    this.server = new McpServer(
      {
        name: this.config.name,
        version: this.config.version,
      },
      {
        capabilities: {
          logging: {},
        },
        instructions: [
          'Athena MCP Server — communication bridge between AI agents and the Athena desktop app.',
          '',
          'PHASE 1 TOOLS (available):',
          ' notify — Send a notification to the user (spec-compliant)',
          ' status_update — Report agent status (spec-compliant)',
          ' request_input — Request user input, blocks until response (spec-compliant)',
          ' athena_notify — Extended notification with priority levels',
          ' athena_update_status — Update agent status with progress %',
          ' athena_report_error — Report an error with recovery info',
          ' athena_report_completion — Report task completion with artifacts',
          '',
          'OUTPUT TOOLS (available):',
          ' athena_read_output — Read full or recent output buffer for a pane',
          ' athena_stream_output — Open a streaming subscription for real-time output',
          ' athena_list_agents — List all active agents with pane IDs and statuses',
          ' athena_get_output_since — Get output since a timestamp or line number',
          '',
          'SEARCH TOOLS (available):',
          ' search_files — Search the codebase using ripgrep with regex, glob, and file type filters',
          '',
          'PHASE 2 TOOLS (defined, not yet available):',
          ' control_pause — Pause an agent pane',
          ' control_resume — Resume a paused agent',
          ' control_cancel — Cancel/terminate an agent',
          '',
          'RESOURCES:',
          ' athena://agents — Current state of all agents',
          ' athena://agent/{id} — State of a specific agent',
          ' athena://app-state — Full application state snapshot',
          '',
          'Best practices:',
          ' - Use notify/status_update for spec-compliant communication',
          ' - Call status_update frequently to keep the dashboard current',
          ' - Use athena_report_completion when your task is fully done',
          ' - Use athena_report_error with recoverable=true if you can continue',
          ' - Use athena_read_output to inspect what another agent has produced',
          ' - Use athena_stream_output for real-time output monitoring',
        ].join('\n'),
      },
    )

    this.bridge = new AthenaBridge({
      athenaHost: this.config.athenaHost!,
      athenaPort: this.config.athenaPort!,
      authToken: this.config.authToken,
    })

    this.outputBuffer = new OutputBufferManager()

    this.registerTools()
    registerResources(this.server, this.bridge)
  }

  private registerTools(): void {
    // ── Spec-compliant Phase 1 tools ──────────────────────────────

    this.server.tool(
      'notify',
      'Send a notification to the Athena UI. Use this to alert the user when a task completes, fails, or requires attention.',
      {
        level: notifySchema.shape.level,
        message: notifySchema.shape.message,
        title: notifySchema.shape.title,
        metadata: notifySchema.shape.metadata,
        actions: notifySchema.shape.actions,
        priority: notifySchema.shape.priority,
        agentId: notifySchema.shape.agentId,
      },
      async (params) => notify(this.bridge, params as any),
    )

    this.server.tool(
      'status_update',
      'Report the current status of this agent. Athena uses this to track agent health and display status in the UI.',
      {
        status: statusUpdateSchema.shape.status,
        message: statusUpdateSchema.shape.message,
        progress: statusUpdateSchema.shape.progress,
        artifacts: statusUpdateSchema.shape.artifacts,
        agentId: statusUpdateSchema.shape.agentId,
      },
      async (params) => statusUpdate(this.bridge, params as any),
    )

    this.server.tool(
      'request_input',
      'Request input from the user. The notification appears in the UI with response options. The tool call blocks until the user responds or a timeout is reached.',
      {
        prompt: requestInputSchema.shape.prompt,
        options: requestInputSchema.shape.options,
        allowFreeText: requestInputSchema.shape.allowFreeText,
        timeoutMs: requestInputSchema.shape.timeoutMs,
        agentId: requestInputSchema.shape.agentId,
      },
      async (params) => requestInput(this.bridge, params as any),
    )

    // ── Extended athena_ prefixed tools ───────────────────────────

    this.server.tool(
      'athena_notify',
      'Send a notification to the user through the Athena UI. Extended version with priority levels and agent identification.',
      {
        type: athenaNotifySchema.shape.type,
        title: athenaNotifySchema.shape.title,
        message: athenaNotifySchema.shape.message,
        priority: athenaNotifySchema.shape.priority,
        agentId: athenaNotifySchema.shape.agentId,
      },
      async (params) => athenaNotify(this.bridge, params as any),
    )

    this.server.tool(
      'athena_request_input',
      'Request input from the user. This blocks until the user responds, dismisses the prompt, or the timeout expires.',
      {
        prompt: athenaRequestInputSchema.shape.prompt,
        defaultResponse: athenaRequestInputSchema.shape.defaultResponse,
        timeout: athenaRequestInputSchema.shape.timeout,
        agentId: athenaRequestInputSchema.shape.agentId,
      },
      async (params) => athenaRequestInput(this.bridge, params as any),
    )

    this.server.tool(
      'athena_update_status',
      'Update the agent status on the Athena dashboard. Call frequently to keep the UI current with your progress.',
      {
        agentId: athenaUpdateStatusSchema.shape.agentId,
        status: athenaUpdateStatusSchema.shape.status,
        message: athenaUpdateStatusSchema.shape.message,
        progress: athenaUpdateStatusSchema.shape.progress,
        details: athenaUpdateStatusSchema.shape.details,
      },
      async (params) => athenaUpdateStatus(this.bridge, params as any),
    )

    this.server.tool(
      'athena_report_error',
      'Report an error to the Athena system. Set recoverable=true if the agent can continue, false if it must stop.',
      {
        agentId: athenaReportErrorSchema.shape.agentId,
        error: athenaReportErrorSchema.shape.error,
        stack: athenaReportErrorSchema.shape.stack,
        code: athenaReportErrorSchema.shape.code,
        recoverable: athenaReportErrorSchema.shape.recoverable,
        context: athenaReportErrorSchema.shape.context,
      },
      async (params) => athenaReportError(this.bridge, params as any),
    )

    this.server.tool(
      'athena_report_completion',
      'Report that a task has been completed. Include a summary of what was accomplished and any artifacts created.',
      {
        agentId: athenaReportCompletionSchema.shape.agentId,
        summary: athenaReportCompletionSchema.shape.summary,
        artifacts: athenaReportCompletionSchema.shape.artifacts,
        metrics: athenaReportCompletionSchema.shape.metrics,
        duration: athenaReportCompletionSchema.shape.duration,
      },
      async (params) => athenaReportCompletion(this.bridge, params as any),
    )

    // ── Phase 2 stubs ─────────────────────────────────────────────

    this.server.tool(
      'control_pause',
      'Pause the execution of a specific agent pane. (Phase 2 — not yet available)',
      {
        paneId: controlPauseSchema.shape.paneId,
        reason: controlPauseSchema.shape.reason,
      },
      async (params) => controlPause(params as any),
    )

    this.server.tool(
      'control_resume',
      'Resume a paused agent pane. (Phase 2 — not yet available)',
      {
        paneId: controlResumeSchema.shape.paneId,
      },
      async (params) => controlResume(params as any),
    )

    this.server.tool(
      'control_cancel',
      'Cancel and terminate an agent pane. (Phase 2 — not yet available)',
      {
        paneId: controlCancelSchema.shape.paneId,
        force: controlCancelSchema.shape.force,
      },
      async (params) => controlCancel(params as any),
    )

    // ── Output tools ────────────────────────────────────────────

    this.server.tool(
      'athena_read_output',
      'Read the full or recent output buffer for a given pane ID. Returns line-numbered entries, optionally filtered by recency or timestamp.',
      {
        paneId: athenaReadOutputSchema.shape.paneId,
        lines: athenaReadOutputSchema.shape.lines,
        sinceTimestamp: athenaReadOutputSchema.shape.sinceTimestamp,
      },
      async (params) => athenaReadOutput(this.outputBuffer, params as any),
    )

    this.server.tool(
      'athena_stream_output',
      'Open a streaming subscription to receive real-time output chunks for a pane. Returns a snapshot of recent lines, then collects new output for up to 60 seconds or 100 lines.',
      {
        paneId: athenaStreamOutputSchema.shape.paneId,
      },
      async (params) => athenaStreamOutput(this.outputBuffer, params as any),
    )

    this.server.tool(
      'athena_list_agents',
      'List all active agents with their pane IDs, types, and statuses. No parameters required.',
      {},
      async (params) => athenaListAgents(this.bridge, params as any),
    )

    this.server.tool(
      'athena_get_output_since',
      'Get output for a pane since a given timestamp or line number. At least one of sinceTimestamp or sinceLine must be provided.',
      {
        paneId: athenaGetOutputSinceSchema.shape.paneId,
        sinceTimestamp: athenaGetOutputSinceSchema.shape.sinceTimestamp,
        sinceLine: athenaGetOutputSinceSchema.shape.sinceLine,
      },
      async (params) => athenaGetOutputSince(this.outputBuffer, params as any),
    )

    // ── Search tools ─────────────────────────────────────────────

    this.server.tool(
      'search_files',
      'Search the codebase for a pattern using ripgrep. Returns matching file paths, line numbers, and surrounding context. Supports regex, file type filtering, and glob patterns.',
      {
        pattern: searchFilesSchema.shape.pattern,
        path: searchFilesSchema.shape.path,
        glob: searchFilesSchema.shape.glob,
        type: searchFilesSchema.shape.type,
        case_sensitive: searchFilesSchema.shape.case_sensitive,
        max_results: searchFilesSchema.shape.max_results,
        context_lines: searchFilesSchema.shape.context_lines,
      },
      async (params) => searchFiles(this.bridge, params as any),
    )
  }

  async start(): Promise<void> {
    switch (this.config.transport) {
      case 'tcp':
        await this.startTcp()
        break
      case 'websocket':
        await this.startWebSocket()
        break
      case 'stdio':
      default:
        await this.startStdio()
        break
    }
  }

  private async startStdio(): Promise<void> {
    await connectStdio(this.server)
  }

  private async startWebSocket(): Promise<void> {
    const port = this.config.websocketPort ?? 4546
    this.wsTransport = new WebSocketTransport(port, '127.0.0.1')

    this.wsTransport.onMessage(async (message, sessionId) => {
      this.wsTransport?.send(sessionId, {
        jsonrpc: '2.0',
        id: (message as any)?.id,
        result: { status: 'received' },
      })
    })

    await this.wsTransport.start()
  }

  private async startTcp(): Promise<void> {
    const port = this.config.tcpPort ?? 4545
    this.tcpTransport = new TcpTransport(port, '127.0.0.1')

    this.tcpTransport.onMessage(async (message, sessionId) => {
      this.tcpTransport?.send(sessionId, {
        jsonrpc: '2.0',
        id: (message as any)?.id,
        result: { status: 'received' },
      })
    })

    await this.tcpTransport.start()
  }

  async connectToAthena(): Promise<void> {
    await this.bridge.connect()
  }

  getBridge(): AthenaBridge {
    return this.bridge
  }

  getOutputBuffer(): OutputBufferManager {
    return this.outputBuffer
  }

  getServer(): McpServer {
    return this.server
  }

  getTcpTransport(): TcpTransport | null {
    return this.tcpTransport
  }

  getWsTransport(): WebSocketTransport | null {
    return this.wsTransport
  }

  async stop(): Promise<void> {
    await this.bridge.disconnect()
    if (this.wsTransport) {
      await this.wsTransport.stop()
    }
    if (this.tcpTransport) {
      await this.tcpTransport.stop()
    }
  }
}
