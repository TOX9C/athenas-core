# Codebase Hardening Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix 15 identified issues across security, stability, type safety, and cleanup — organized into 4 phases by severity.

**Architecture:** Extract a shared tool executor from the duplicated orchestrator, cap terminal history, add MCP auth token, introduce a panel exclusivity manager to replace cross-store coupling, add ErrorBoundary, type all `any` usages, and clean up scratch files.

**Tech Stack:** TypeScript, Electron IPC, React 19, Zustand, node-pty, @anthropic-ai/sdk, openai

---

## Phase 1 — Critical Security & Stability

### Task 1: Extract Shared Tool Executor from Orchestrator

**Files:**

- Create: `electron/toolExecutor.ts`
- Modify: `electron/athenaOrchestrator.ts:1-349`

The Anthropic path (lines 260-337) and NVIDIA/OpenAI path (lines 192-227) duplicate ~90% of tool-handling logic. Extract a single `executeToolCall` function both paths call into.

- [ ] **Step 1: Create `electron/toolExecutor.ts` with shared tool definitions and executor**

```typescript
import { BrowserWindow } from 'electron'
import { randomUUID } from 'node:crypto'
import { write as ptyWrite } from './ptyManager'

export interface ToolInput {
  task_prompt?: string
  agent_count?: number
  command?: string
  pane_ids?: string[]
}

export interface ToolCallResult {
  text: string
}

export const ORCHESTRATOR_TOOLS = [
  {
    name: 'close_terminals',
    description:
      'Close, remove, or replace terminal panes/agents from the UI entirely (using pane IDs). Use this tool whenever the user asks to close, exit, completely remove, or replace an existing running terminal/agent.',
    input_schema: {
      type: 'object' as const,
      properties: {
        pane_ids: {
          type: 'array',
          items: { type: 'string' },
          description: 'Array of string IDs of the panes to drop/remove.',
        },
      },
      required: ['pane_ids'],
    },
  },
  {
    name: 'launch_claude_cli',
    description:
      "Launch a standard background Claude Code agent. If the user doesn't specify a task, you MUST leave task_prompt empty to launch an interactive Claude shell.",
    input_schema: {
      type: 'object' as const,
      properties: {
        task_prompt: {
          type: 'string',
          description:
            'Optional. The prompt to start the background agent with. Leave entirely empty or omit it to open a blank terminal.',
        },
        agent_count: {
          type: 'number',
          description: 'The number of agents to spawn.',
        },
      },
      required: ['agent_count'],
    },
  },
  {
    name: 'run_command_in_terminals',
    description: 'Run a CLI command inside one or more ALREADY OPEN shell/terminal panes.',
    input_schema: {
      type: 'object' as const,
      properties: {
        pane_ids: {
          type: 'array',
          items: { type: 'string' },
          description:
            'Array of string IDs of the panes (from the Currently Running Terminals list).',
        },
        command: {
          type: 'string',
          description: 'The command string to execute in the shells.',
        },
      },
      required: ['pane_ids', 'command'],
    },
  },
  {
    name: 'launch_custom_agent',
    description: "Launch one of the user's custom-defined agents using a direct CLI command.",
    input_schema: {
      type: 'object' as const,
      properties: {
        command: {
          type: 'string',
          description: 'The exact CLI command of the custom agent to launch.',
        },
        agent_count: {
          type: 'number',
          description: 'The number of custom agents to spawn.',
        },
      },
      required: ['command', 'agent_count'],
    },
  },
]

export function toOpenAITools() {
  return ORCHESTRATOR_TOOLS.map((t) => ({
    type: 'function' as const,
    function: {
      name: t.name,
      description: t.description,
      parameters: {
        type: 'object',
        properties: t.input_schema.properties,
        required: t.input_schema.required,
      },
    },
  }))
}

function getWindow(): BrowserWindow | null {
  return BrowserWindow.getAllWindows()[0] ?? null
}

function buildAgentCommand(taskPrompt?: string): string {
  if (!taskPrompt) return 'claude'
  return `claude -p ${shellEscape(taskPrompt)}`
}

function shellEscape(arg: string): string {
  return "'" + arg.replace(/'/g, "'\\''") + "'"
}

export function executeToolCall(name: string, args: ToolInput): ToolCallResult {
  const win = getWindow()
  if (!win) {
    return { text: 'Failed: No window available.' }
  }

  switch (name) {
    case 'launch_claude_cli': {
      const { task_prompt, agent_count = 1 } = args
      const agentCommand = buildAgentCommand(task_prompt)
      for (let i = 0; i < agent_count; i++) {
        const id = `agent-${randomUUID()}`
        win.webContents.send('athena:agent-spawned', {
          id,
          agentType: 'claude',
          agentCmd: agentCommand,
        })
      }
      return { text: `Done, launched ${agent_count} standard agents.` }
    }

    case 'launch_custom_agent': {
      const { command, agent_count = 1 } = args
      for (let i = 0; i < agent_count; i++) {
        const id = `custom-agent-${randomUUID()}`
        win.webContents.send('athena:agent-spawned', { id, agentType: 'custom', agentCmd: command })
      }
      return { text: `Done, launched ${agent_count} custom agents.` }
    }

    case 'close_terminals': {
      const { pane_ids } = args
      if (Array.isArray(pane_ids)) {
        win.webContents.send('athena:close-panes', pane_ids)
      }
      return { text: `Closed ${pane_ids?.length ?? 0} terminal(s).` }
    }

    case 'run_command_in_terminals': {
      const { pane_ids, command } = args
      if (Array.isArray(pane_ids) && command) {
        pane_ids.forEach((id) => {
          ptyWrite(id, command)
          setTimeout(() => ptyWrite(id, '\r'), 150)
        })
      }
      return { text: `Sent command to ${pane_ids?.length ?? 0} terminal(s).` }
    }

    default:
      return { text: `Unknown tool: ${name}` }
  }
}
```

- [ ] **Step 2: Rewrite `electron/athenaOrchestrator.ts` to use the shared executor**

```typescript
import { BrowserWindow } from 'electron'
import Anthropic from '@anthropic-ai/sdk'
import OpenAI from 'openai'
import { getStore } from './storeUtil'
import { ORCHESTRATOR_TOOLS, toOpenAITools, executeToolCall, type ToolInput } from './toolExecutor'

function buildSystemPrompt(
  spaces: any[],
  tasks: any[],
  customAgents: any[],
  activePanes: any[],
): string {
  return `You are the Athena Orchestrator, an AI assistant built into an Electron IDE. Your primary job is to manage, delegate, and launch background agents. You can also chat and answer questions normally. 

CRITICAL: If the user asks to launch an agent or terminal WITHOUT giving a task, you MUST execute the tool with an EMPTY task_prompt. Do NOT complain or ask for a prompt.

Project Context:
- Active Workspaces: ${JSON.stringify(spaces.map((s: any) => ({ name: s.name, dir: s.dir })))}
- Current Project Tasks/Todos: ${JSON.stringify(tasks)}
- Custom Agents Available: ${JSON.stringify(customAgents)}
- Currently Running Terminals/Panes: ${JSON.stringify(activePanes)}

INSTRUCTIONS:
- You have context to the current files (via workspaces), tasks, and custom agents. Use this knowledge to help the user.
- If the user explicitly asks to launch a basic agent, use 'launch_claude_cli'. Omit the task_prompt completely if they just want an empty terminal.
- If the user asks to launch a CUSTOM agent, use 'launch_custom_agent' and provide the command text associated with the custom agent from the context list.
- If the user asks you to interact with or run a command inside ALREADY OPEN "free shell terminals" or existing panes, use 'run_command_in_terminals' and pass the array of terminal IDs and the command.

When you successfully use a tool, briefly confirm it (e.g., 'Done, launched 1 agent').`
}

export class AthenaOrchestrator {
  private anthropic?: Anthropic
  private openai?: OpenAI
  private messages: Anthropic.MessageParam[] = []
  private openaiMessages: OpenAI.ChatCompletionMessageParam[] = []

  async sendMessage(userText: string): Promise<string> {
    const store = await getStore()
    const provider = (store.get('athena.provider') || 'anthropic') as string
    const apiKey = store.get('athena.apiKey') as string | undefined
    const model = (store.get('athena-model') ||
      (provider === 'nvidia_nim'
        ? 'minimaxai/minimax-text-01'
        : 'claude-sonnet-4-20250514')) as string

    if (!apiKey) {
      return 'Error: API Key is required. Please set it in Settings.'
    }

    const customAgents = (store.get('athena-customAgents') || []) as any[]
    const spaces = (store.get('spaces') || []) as any[]
    const tasks = (store.get('tasks') || []) as any[]

    const activePanes = spaces.flatMap(
      (s: any) =>
        s.panes?.map((p: any) => ({
          id: p.id,
          type: p.agentType,
          isShell: p.agentType === 'shell',
        })) || [],
    )

    const systemPrompt = buildSystemPrompt(spaces, tasks, customAgents, activePanes)

    if (provider === 'nvidia_nim') {
      return this.sendOpenAI(
        apiKey,
        model,
        systemPrompt,
        userText,
        'https://integrate.api.nvidia.com/v1',
      )
    }
    return this.sendAnthropic(apiKey, model, systemPrompt, userText)
  }

  private async sendOpenAI(
    apiKey: string,
    model: string,
    systemPrompt: string,
    userText: string,
    baseURL: string,
  ): Promise<string> {
    if (!this.openai) {
      this.openai = new OpenAI({ apiKey, baseURL })
    }

    if (this.openaiMessages.length === 0 || this.openaiMessages[0].role !== 'system') {
      this.openaiMessages = [{ role: 'system', content: systemPrompt }, ...this.openaiMessages]
    } else {
      ;(this.openaiMessages[0] as OpenAI.ChatCompletionSystemMessageParam).content = systemPrompt
    }

    this.openaiMessages.push({ role: 'user', content: userText })

    try {
      const response = await this.openai.chat.completions.create({
        model,
        max_tokens: 4096,
        messages: this.openaiMessages,
        tools: toOpenAITools(),
      })

      const choice = response.choices[0]
      if (!choice.message) return ''

      this.openaiMessages.push(choice.message)
      let responseText = choice.message.content || ''

      if (choice.message.tool_calls?.length) {
        for (const toolCall of choice.message.tool_calls) {
          const args: ToolInput = JSON.parse(toolCall.function.arguments)
          const result = executeToolCall(toolCall.function.name, args)
          responseText += '\n' + result.text

          this.openaiMessages.push({
            role: 'tool',
            tool_call_id: toolCall.id,
            content: result.text,
          })
        }
      }

      return responseText.trim()
    } catch (error: unknown) {
      this.openaiMessages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      return `Error calling provider: ${msg}`
    }
  }

  private async sendAnthropic(
    apiKey: string,
    model: string,
    systemPrompt: string,
    userText: string,
  ): Promise<string> {
    if (!this.anthropic) {
      this.anthropic = new Anthropic({ apiKey })
    }

    this.messages.push({ role: 'user', content: userText })

    try {
      const response = await this.anthropic.messages.create({
        model,
        max_tokens: 4096,
        system: systemPrompt,
        messages: this.messages,
        tools: ORCHESTRATOR_TOOLS as any,
      })

      this.messages.push({ role: 'assistant', content: response.content })

      let responseText = ''

      for (const block of response.content) {
        if (block.type === 'text') {
          responseText += block.text
        } else if (block.type === 'tool_use') {
          const result = executeToolCall(block.name, block.input as ToolInput)
          responseText += '\n' + result.text

          this.messages.push({
            role: 'user',
            content: [{ type: 'tool_result', tool_use_id: block.id, content: result.text }],
          })
        }
      }

      return responseText.trim()
    } catch (error: unknown) {
      this.messages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      return `Error calling Anthropic: ${msg}`
    }
  }
}

export const athenaOrchestrator = new AthenaOrchestrator()
```

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds with no errors

- [ ] **Step 4: Commit**

```bash
git add electron/toolExecutor.ts electron/athenaOrchestrator.ts
git commit -m "refactor: extract shared tool executor from orchestrator, fix command injection"
```

**Note:** This task also fixes Issue 2 (command injection) — the `shellEscape` function in `toolExecutor.ts` uses proper POSIX single-quote escaping (`'\\''`) instead of the vulnerable `replace(/'/g, "\\'")`. It also fixes Issue 6 (stale model) by updating the default to `claude-sonnet-4-20250514`.

---

### Task 2: Cap Terminal History (Unbounded Memory)

**Files:**

- Modify: `electron/ptyManager.ts:50-53`

The `history` Map concatenates every byte of terminal output forever. Cap it at 100KB per session.

- [ ] **Step 1: Add history size cap constant and apply in onData handler**

In `electron/ptyManager.ts`, add the constant after line 6 and modify the `onData` handler:

```typescript
// After line 6: const history = new Map<string, string>()
const MAX_HISTORY_BYTES = 100_000
```

Replace lines 50-53 (inside the `ptyProcess.onData` callback):

```typescript
ptyProcess.onData((data) => {
  const current = history.get(id) || ''
  const updated = current + data
  history.set(id, updated.length > MAX_HISTORY_BYTES ? updated.slice(-MAX_HISTORY_BYTES) : updated)
  mainWindow.webContents.send(`pty:data:${id}`, data)
})
```

- [ ] **Step 2: Clean up history on session exit**

In the `onExit` handler (line 56-59), also delete history:

```typescript
ptyProcess.onExit(({ exitCode }) => {
  sessions.delete(id)
  history.delete(id)
  mainWindow.webContents.send(`pty:exit:${id}`, exitCode)
})
```

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add electron/ptyManager.ts
git commit -m "fix: cap terminal history at 100KB to prevent memory leaks"
```

---

### Task 3: Add MCP Server Token Authentication

**Files:**

- Modify: `electron/mcpServer.ts:67-88`

The MCP server on `127.0.0.1:4545` has zero authentication. Add a per-session random token that clients must provide in the `initialize` request.

- [ ] **Step 1: Generate a session token and validate on connection**

In `electron/mcpServer.ts`, add a token after the imports (after line 4):

```typescript
const SESSION_TOKEN = randomUUID()
```

Add a function to expose the token (so `spawn_agents` in the same process can pass it to child agents):

```typescript
export function getMcpToken(): string {
  return SESSION_TOKEN
}
```

Modify `handleRequest` (line 90) to validate the token on `initialize`:

```typescript
async function handleRequest(socket: net.Socket, mainWindow: BrowserWindow, req: any) {
  const send = (res: any) => socket.write(JSON.stringify(res) + '\n')

  if (req.method === 'initialize') {
    if (req.params?.token !== SESSION_TOKEN) {
      send({
        jsonrpc: '2.0',
        id: req.id,
        error: { code: -32600, message: 'Invalid or missing auth token' },
      })
      socket.end()
      return
    }
    send({
      jsonrpc: '2.0',
      id: req.id,
      result: {
        protocolVersion: '2024-11-05',
        capabilities: { tools: {} },
        serverInfo: { name: 'athena-orchestrator', version: '1.0.0' },
      },
    })
  } else if (req.method === 'notifications/initialized') {
    // no-op
  } else if (req.method === 'tools/list') {
    send({ jsonrpc: '2.0', id: req.id, result: { tools: TOOLS } })
  } else if (req.method === 'tools/call') {
    send({
      jsonrpc: '2.0',
      id: req.id,
      result: await handleToolCall(mainWindow, req.params.name, req.params.arguments),
    })
  }
}
```

- [ ] **Step 2: Update `spawn_agents` handler to pass the token to child agents**

In the `spawn_agents` handler (line 151-163), update the MCP config to include the token:

```typescript
if (name === 'spawn_agents') {
  const shell = process.platform === 'win32' ? 'powershell.exe' : process.env.SHELL || '/bin/zsh'
  const instruction =
    args.instruction ||
    'You are an Athena Swarm Worker. Use the get_next_task MCP tool to pull work and update_task_status to complete it.'

  const proxyPath = require('path').join(__dirname, '../../bin/mcp-proxy.js')
  const mcpConfig = JSON.stringify({
    athena: { command: 'node', args: [proxyPath], env: { ATHENA_MCP_TOKEN: SESSION_TOKEN } },
  })
  const mcpEnv = `export CLAUDE_MCP_SERVERS='${mcpConfig.replace(/'/g, "'\\''")}';`
  const agentCmd = `${mcpEnv} claude -p "${instruction}"`

  for (let i = 0; i < args.count; i++) {
    const ptyId = `worker-${Date.now()}-${i}`
    ptyMgr.spawn(ptyId, args.cwd, shell, agentCmd, mainWindow)
  }
  return { content: [{ type: 'text', text: `Spawned ${args.count} workers successfully.` }] }
}
```

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add electron/mcpServer.ts
git commit -m "fix: add session token auth to MCP server"
```

---

## Phase 2 — High Priority

### Task 4: Delete Scratch Files & Add .gitignore

**Files:**

- Delete: All `patch*.js`, `test-*.js`, `update_*.*`, `Oops.rej`, `*.orig` in project root
- Create: `.gitignore`

- [ ] **Step 1: Delete all scratch files from the project root**

```bash
cd /Users/apollo/Documents/athenas-core
rm -f patch.js patch2.js patch_app.js patch_array_command.js patch_athenastore.js \
      patch_labels_and_custom.js patch_orchestrator.js patch_orchestrator3.js \
      patch_submit.js patch_workspaceStore.js \
      test-ansi.js test-colors.js test-orchestrator.js test-pattern.js \
      test-pty.js test-regex.js test-space.js \
      update_ansi.patch update_app.patch update_app.py update_global.patch \
      update_orchestrator.patch update_orchestrator.py update_preload.patch \
      update_pty.patch update_settings.patch update_sidebar.patch \
      update_terminal_pane.patch update_useterminal.patch update_workspace_store.patch \
      Oops.rej
rm -f electron/ptyManager.ts.orig
```

- [ ] **Step 2: Create `.gitignore`**

```gitignore
# Dependencies
node_modules/
.pnp
.pnp.js

# Build output
out/
dist/
build/

# Environment
.env
.env.local
.env.*.local

# IDE
.vscode/
.idea/
*.swp
*.swo

# OS
.DS_Store
Thumbs.db

# Scratch / dev artifacts
*.patch
*.rej
*.orig
patch*.js
test-*.js
update_*.py
```

- [ ] **Step 3: Commit**

```bash
git add -A .gitignore
git add -u  # stages the deletions
git commit -m "chore: remove scratch files, add .gitignore"
```

---

### Task 5: Fix Cross-Store Coupling

**Files:**

- Create: `src/store/panelManager.ts`
- Modify: `src/store/uiStore.ts:45-60`
- Modify: `src/store/athenaStore.ts:50-62`

Replace bidirectional store calls with a single `panelManager` that both stores delegate to.

- [ ] **Step 1: Create `src/store/panelManager.ts`**

```typescript
import { useUIStore } from './uiStore'
import { useAthenaStore } from './athenaStore'

export type ExclusivePanel = 'athena' | 'browser' | 'editor' | null

export function activatePanel(panel: ExclusivePanel): void {
  const uiState = useUIStore.getState()
  const athenaState = useAthenaStore.getState()

  const browserOpen = panel === 'browser'
  const editorOpen = panel === 'editor'
  const athenaOpen = panel === 'athena'

  useUIStore.setState({
    browserOpen,
    editorOpen: editorOpen && !browserOpen,
  })
  athenaState._setOpenDirect(athenaOpen)
}

export function togglePanel(panel: ExclusivePanel): void {
  const uiState = useUIStore.getState()
  const athenaState = useAthenaStore.getState()

  const isCurrentlyOpen =
    panel === 'browser'
      ? uiState.browserOpen
      : panel === 'editor'
        ? uiState.editorOpen
        : panel === 'athena'
          ? athenaState.isOpen
          : false

  activatePanel(isCurrentlyOpen ? null : panel)
}
```

- [ ] **Step 2: Update `uiStore.ts` to remove cross-store calls**

Replace `toggleBrowser` and `toggleEditor` (lines 45-60):

```typescript
  toggleBrowser: () => {
    const { togglePanel } = require('./panelManager')
    togglePanel('browser')
  },
  toggleEditor: () => {
    const { togglePanel } = require('./panelManager')
    togglePanel('editor')
  },
```

- [ ] **Step 3: Update `athenaStore.ts` to remove cross-store calls**

Add `_setOpenDirect` (internal, no side effects) and update `setOpen` and `toggleOpen`:

```typescript
  _setOpenDirect: (open: boolean) => set({ isOpen: open }),
  setOpen: (open) => {
    const { activatePanel } = require('../store/panelManager')
    activatePanel(open ? 'athena' : null)
  },
  toggleOpen: () => {
    const { togglePanel } = require('../store/panelManager')
    togglePanel('athena')
  },
```

Add `_setOpenDirect` to the `AthenaState` interface:

```typescript
  _setOpenDirect: (open: boolean) => void
```

- [ ] **Step 4: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 5: Commit**

```bash
git add src/store/panelManager.ts src/store/uiStore.ts src/store/athenaStore.ts
git commit -m "refactor: decouple uiStore/athenaStore with panelManager"
```

---

### Task 6: Type the `any` Usages

**Files:**

- Modify: `src/types/global.d.ts`
- Modify: `electron/mcpServer.ts:6`

- [ ] **Step 1: Type `global.d.ts` — replace `any` with proper types**

```typescript
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
        onAthenaClosePanes: (cb: (data: string[]) => void) => () => void
        onAthenaSpawn: (
          cb: (data: { id: string; agentType: string; agentCmd?: string }) => void,
        ) => () => void
        onData: (id: string, cb: (data: string) => void) => () => void
        onExit: (id: string, cb: (code: number) => void) => () => void
      }
      fs: {
        readTree: (
          dir: string,
        ) => Promise<
          | { name: string; path: string; isDir: boolean; children?: any[] }
          | { success: false; error: string }
        >
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
        chat: (msg: string) => Promise<string>
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
```

- [ ] **Step 2: Type `mcpServer.ts` storeInstance**

Replace line 6:

```typescript
let storeInstance: import('electron-store').default | null = null
```

- [ ] **Step 3: Verify the build compiles and fix any type errors**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -30`
Expected: Build succeeds (may need minor adjustments in consuming files)

- [ ] **Step 4: Commit**

```bash
git add src/types/global.d.ts electron/mcpServer.ts
git commit -m "refactor: replace any types with proper interfaces"
```

---

## Phase 3 — Medium Priority

### Task 7: Fix useTerminal Race Condition & Hardcoded Shell

**Files:**

- Modify: `src/components/Terminal/useTerminal.ts:85-101`

- [ ] **Step 1: Fix the spawnedRef race condition and remove hardcoded shell**

Replace lines 85-101:

```typescript
if (!spawnedRef.current) {
  spawnedRef.current = true
  window.athena.pty
    .hasSession(paneId)
    .then((exists) => {
      if (!exists) {
        window.athena.pty
          .spawn(paneId, cwd, '', agentCmd || undefined)
          .then((res) => {
            if (res && !res.success) {
              spawnedRef.current = false
            }
          })
          .catch(() => {
            spawnedRef.current = false
          })
      }
    })
    .catch(() => {
      spawnedRef.current = false
    })
}
```

Passing empty string `''` for shell makes `ptyManager.ts:32` use `getDefaultShell()` which is already platform-aware.

- [ ] **Step 2: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 3: Commit**

```bash
git add src/components/Terminal/useTerminal.ts
git commit -m "fix: remove hardcoded /bin/zsh, use platform-aware default shell"
```

---

### Task 8: Implement fs:watchDir Handler in main.ts

**Files:**

- Modify: `electron/main.ts` (add after line 95)

The preload declares `watchDir` that listens for `fs:change:${dir}` events, but `main.ts` never emits them.

- [ ] **Step 1: Add the watchDir IPC handler using chokidar/fs.watch**

Add after the `fs:exists` handler (after line 95):

```typescript
const activeWatchers = new Map<string, import('fs').FSWatcher>()

ipcMain.on('fs:watchDir', (_event, dir: string) => {
  if (activeWatchers.has(dir)) return
  try {
    const { watch } = require('fs')
    const watcher = watch(dir, { recursive: true }, () => {
      mainWindow?.webContents.send(`fs:change:${dir}`)
    })
    activeWatchers.set(dir, watcher)
  } catch {
    // directory doesn't exist or can't be watched
  }
})

ipcMain.on('fs:unwatchDir', (_event, dir: string) => {
  const watcher = activeWatchers.get(dir)
  if (watcher) {
    watcher.close()
    activeWatchers.delete(dir)
  }
})
```

- [ ] **Step 2: Update the preload to send the watch request**

In `electron/preload.ts`, update the `watchDir` implementation (line 45-48) to also send the IPC:

```typescript
    watchDir: (dir: string, cb: () => void) => {
      ipcRenderer.send('fs:watchDir', dir)
      const handler = () => cb()
      ipcRenderer.on(`fs:change:${dir}`, handler)
      return () => {
        ipcRenderer.removeListener(`fs:change:${dir}`, handler)
        ipcRenderer.send('fs:unwatchDir', dir)
      }
    },
```

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add electron/main.ts electron/preload.ts
git commit -m "fix: implement fs:watchDir handler in main process"
```

---

### Task 9: Add React ErrorBoundary

**Files:**

- Create: `src/components/shared/ErrorBoundary.tsx`
- Modify: `src/App.tsx:194` (wrap root content)

- [ ] **Step 1: Create the ErrorBoundary component**

```typescript
import { Component, type ReactNode } from 'react'

interface ErrorBoundaryProps {
  children: ReactNode
  fallback?: ReactNode
}

interface ErrorBoundaryState {
  hasError: boolean
  error: Error | null
}

export class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { hasError: false, error: null }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error }
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) return this.props.fallback

      return (
        <div style={{
          display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center',
          height: '100%', padding: 32, color: 'var(--textMuted)', background: 'var(--bg)',
        }}>
          <h2 style={{ color: 'var(--error)', marginBottom: 8 }}>Something went wrong</h2>
          <pre style={{ fontSize: 12, maxWidth: 600, overflow: 'auto', color: 'var(--textDim)' }}>
            {this.state.error?.message}
          </pre>
          <button
            onClick={() => this.setState({ hasError: false, error: null })}
            style={{
              marginTop: 16, padding: '8px 16px', borderRadius: 6,
              background: 'var(--accent)', color: '#fff', border: 'none', cursor: 'pointer',
            }}
          >
            Try again
          </button>
        </div>
      )
    }

    return this.props.children
  }
}
```

- [ ] **Step 2: Wrap the App root content in an ErrorBoundary**

In `src/App.tsx`, add import:

```typescript
import { ErrorBoundary } from './components/shared/ErrorBoundary'
```

Wrap the return JSX (line 194) — wrap the outermost `<div>` children:

```tsx
return (
  <ErrorBoundary>
    <div
      className="h-screen w-screen flex flex-col overflow-hidden"
      style={{ background: 'var(--bg)' }}
    >
      {/* ... existing content unchanged ... */}
    </div>
  </ErrorBoundary>
)
```

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add src/components/shared/ErrorBoundary.tsx src/App.tsx
git commit -m "feat: add ErrorBoundary to catch React render crashes"
```

---

### Task 10: Derive ThemeName from themes record

**Files:**

- Modify: `src/types/theme.ts`
- Modify: `src/themes/themes.ts` (add export for deriving)

- [ ] **Step 1: Export theme keys from themes.ts and derive ThemeName**

In `src/themes/themes.ts`, add after the `themes` record definition:

```typescript
export const themeNames = Object.keys(themes) as ThemeName[]
```

In `src/types/theme.ts`, replace the manual union. Since the themes record depends on `ThemeName`, deriving creates a cycle. Instead, keep the union but add a compile-time exhaustiveness check:

At the bottom of `src/themes/themes.ts`, add:

```typescript
// Compile-time check: if a theme key is missing from ThemeName, this errors
const _exhaustiveCheck: Record<ThemeName, ThemeDefinition> = themes
```

This is already satisfied by the existing `Record<ThemeName, ThemeDefinition>` type on `themes`. The existing setup is actually correct — `themes` already enforces the union. No change needed here. Skip this task.

- [ ] **Step 2: Commit (skip if no changes needed)**

---

## Phase 4 — Low Priority Cleanup

### Task 11: Remove Unused framer-motion Dependency

**Files:**

- Modify: `package.json` (remove framer-motion)

- [ ] **Step 1: Remove framer-motion**

```bash
cd /Users/apollo/Documents/athenas-core && npm uninstall framer-motion
```

- [ ] **Step 2: Verify no imports exist**

```bash
grep -r "framer-motion\|from 'motion'" src/ electron/ || echo "Clean — no imports"
```

Expected: "Clean — no imports"

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add package.json package-lock.json
git commit -m "chore: remove unused framer-motion dependency"
```

---

### Task 12: Add Atomic Writes to Swarm Mailbox

**Files:**

- Modify: `electron/swarmCoordinator.ts:40-58`

- [ ] **Step 1: Use write-to-temp-then-rename for atomic file operations**

Replace the `sendMessage` handler body (lines 35-63) to use atomic writes:

```typescript
ipcMain.handle(
  'swarm:sendMessage',
  async (_event, dir: string, from: string, to: string, msg: string) => {
    try {
      const mailboxDir = join(dir, '.ade', 'mailbox')
      try {
        await access(mailboxDir)
      } catch {
        await mkdir(mailboxDir, { recursive: true })
      }

      const mailboxPath = join(mailboxDir, `${to}.json`)
      const tmpPath = mailboxPath + `.tmp.${Date.now()}`
      let messages: any[] = []
      try {
        const content = await readFile(mailboxPath, 'utf-8')
        messages = JSON.parse(content)
      } catch {
        // file doesn't exist yet
      }

      messages.push({
        id: `msg-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`,
        from,
        to,
        content: msg,
        timestamp: Date.now(),
        read: false,
      })

      await writeFile(tmpPath, JSON.stringify(messages, null, 2), 'utf-8')
      const { rename } = await import('fs/promises')
      await rename(tmpPath, mailboxPath)
      return { success: true }
    } catch (err: any) {
      return { success: false, error: err.message }
    }
  },
)
```

Also apply the same pattern to `swarm:writeState` (lines 23-33):

```typescript
ipcMain.handle('swarm:writeState', async (_event, dir: string, state: any) => {
  try {
    const adeDir = join(dir, '.ade')
    try {
      await access(adeDir)
    } catch {
      await mkdir(adeDir, { recursive: true })
    }
    const statePath = join(adeDir, 'swarm-state.json')
    const tmpPath = statePath + `.tmp.${Date.now()}`
    await writeFile(tmpPath, JSON.stringify(state, null, 2), 'utf-8')
    const { rename } = await import('fs/promises')
    await rename(tmpPath, statePath)
    return { success: true }
  } catch (err: any) {
    return { success: false, error: err.message }
  }
})
```

And the poll interval write at line 107:

```typescript
if (modified) {
  const tmpPath = statePath + `.tmp.${Date.now()}`
  await writeFile(tmpPath, JSON.stringify(state, null, 2), 'utf-8')
  const { rename } = await import('fs/promises')
  await rename(tmpPath, statePath)
}
```

- [ ] **Step 2: Add `rename` to the imports at the top of the file**

Update line 2:

```typescript
import { readFile, writeFile, mkdir, access, rename } from 'fs/promises'
```

Then replace inline `await import('fs/promises')` calls with the top-level import.

- [ ] **Step 3: Verify the build compiles**

Run: `cd /Users/apollo/Documents/athenas-core && npm run build 2>&1 | tail -20`
Expected: Build succeeds

- [ ] **Step 4: Commit**

```bash
git add electron/swarmCoordinator.ts
git commit -m "fix: use atomic rename for swarm mailbox writes"
```
