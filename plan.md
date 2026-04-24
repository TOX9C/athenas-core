# Athena's Core — Build Plan

> **How to use this file:** This is the master build plan for Athena's Core. Read the relevant phase section before starting work on it. Always re-read the **Shared Context** sections (2, 5, 6) when starting any new phase. After completing each phase, verify the checkpoint before moving on.

---

## 1. WHAT YOU ARE BUILDING

A native desktop **Agent Development Environment** called **Athena's Core** — a single-window app that replaces juggling a terminal, code editor, browser, and task board across separate apps. Inspired by BridgeSpace by BridgeMind.

The app gives builders:
- Multi-pane terminal workspaces with command blocks (grouped command + output)
- One-click AI agent auto-launch (Claude Code, Codex, OpenCode, Gemini CLI, or any custom CLI)
- A multi-agent orchestration system (AthenaSwarm) with roles, file ownership, and a live activity feed
- A full embedded browser panel (Electron WebContentsView)
- An integrated code editor with file tree (Monaco + fs.watch)
- A Kanban task board (Todo → In Progress → In Review → Complete)
- 25+ themes, keyboard shortcuts, workspace tabs with color labels

---

## 2. TECH STACK (SHARED CONTEXT — re-read before every phase)

| Layer | Technology |
|-------|-----------|
| Desktop shell | **Electron** (latest stable, v32+) |
| Renderer framework | **React 19 + TypeScript** |
| Bundler | **electron-vite** (use `npm create electron-vite@latest` to scaffold) |
| Terminal rendering | **xterm.js** with `@xterm/addon-webgl` and `@xterm/addon-fit` |
| Shell processes | **node-pty** (spawns real PTY processes) |
| Code editor | **@monaco-editor/react** |
| State management | **Zustand** (with `immer` middleware) |
| Persistence | **electron-store** (v10, ESM-compatible) |
| Styling | **Tailwind CSS v3** with a custom dark theme config |
| IPC bridge | Electron `contextBridge` + typed preload (`preload.ts`) |
| Drag-resize panels | **react-resizable-panels** |
| Kanban DnD | **@dnd-kit/core** + **@dnd-kit/sortable** |
| Icons | **lucide-react** |
| Animations | **framer-motion** |

**Do NOT use:** create-react-app, CRACO, webpack directly, Tauri, or `BrowserView` (deprecated — use `WebContentsView`).

---

## 3. FILE & FOLDER STRUCTURE

Every file must have correct, working code. No empty stubs.

```
athenas-core/
├── electron/
│   ├── main.ts                  # Electron main process entry
│   ├── preload.ts               # contextBridge API surface
│   ├── ptyManager.ts            # node-pty: spawn, write, resize, kill
│   ├── fileSystem.ts            # read tree, read file, write file, watch dir
│   ├── browserManager.ts        # WebContentsView lifecycle
│   └── swarmCoordinator.ts      # Swarm state file read/write, mailbox IPC
├── src/
│   ├── main.tsx                 # React entry, mounts <App />
│   ├── App.tsx                  # Root layout: sidebar + main area
│   ├── store/
│   │   ├── workspaceStore.ts    # Spaces, tabs, pane layouts
│   │   ├── terminalStore.ts     # Per-pane terminal state
│   │   ├── swarmStore.ts        # Swarm sessions, agent roles, activity feed
│   │   ├── editorStore.ts       # Open files, active tab, cursor position
│   │   ├── taskStore.ts         # Kanban tasks per workspace
│   │   └── uiStore.ts           # Active panels, theme, sidebar state
│   ├── components/
│   │   ├── Sidebar/
│   │   │   ├── Sidebar.tsx
│   │   │   ├── WorkspaceList.tsx
│   │   │   ├── FileExplorer.tsx
│   │   │   ├── FileTreeNode.tsx
│   │   │   └── AgentPanel.tsx
│   │   ├── Terminal/
│   │   │   ├── TerminalGrid.tsx
│   │   │   ├── TerminalPane.tsx
│   │   │   ├── CommandBlock.tsx
│   │   │   ├── CommandBlockList.tsx
│   │   │   └── useTerminal.ts
│   │   ├── Workspace/
│   │   │   ├── WorkspaceTabs.tsx
│   │   │   ├── WorkspaceTab.tsx
│   │   │   ├── NewSpaceModal.tsx
│   │   │   ├── GridTemplateSelector.tsx
│   │   │   └── AgentPicker.tsx
│   │   ├── Swarm/
│   │   │   ├── SwarmLauncher.tsx
│   │   │   ├── SwarmModal.tsx
│   │   │   ├── SwarmBoard.tsx
│   │   │   ├── SwarmActivityFeed.tsx
│   │   │   ├── AgentCard.tsx
│   │   │   └── SwarmRoleBadge.tsx
│   │   ├── Browser/
│   │   │   ├── BrowserPanel.tsx
│   │   │   └── BrowserToolbar.tsx
│   │   ├── Editor/
│   │   │   ├── EditorPanel.tsx
│   │   │   ├── EditorTabs.tsx
│   │   │   ├── QuickOpen.tsx
│   │   │   └── useMonaco.ts
│   │   ├── Kanban/
│   │   │   ├── KanbanBoard.tsx
│   │   │   ├── KanbanColumn.tsx
│   │   │   └── KanbanCard.tsx
│   │   ├── Settings/
│   │   │   ├── SettingsModal.tsx
│   │   │   ├── ThemePicker.tsx
│   │   │   └── ShortcutsRef.tsx
│   │   └── shared/
│   │       ├── ResizablePanel.tsx
│   │       ├── Modal.tsx
│   │       ├── Button.tsx
│   │       ├── Badge.tsx
│   │       ├── Tooltip.tsx
│   │       ├── ContextMenu.tsx
│   │       └── Toast.tsx          # Error/success notification
│   ├── types/
│   │   ├── workspace.ts
│   │   ├── terminal.ts
│   │   ├── swarm.ts
│   │   ├── editor.ts
│   │   ├── task.ts
│   │   └── theme.ts
│   ├── themes/
│   │   └── themes.ts
│   └── utils/
│       ├── commandParser.ts     # Parse command boundaries from PTY stream
│       ├── fileIcons.ts
│       ├── agentCommands.ts
│       ├── fuzzySearch.ts
│       └── platformUtils.ts     # OS detection, default shell, path helpers
├── package.json
├── tsconfig.json
├── tsconfig.node.json
├── electron-builder.yml
├── vite.config.ts
├── tailwind.config.ts
└── plan.md                      # This file
```

---

## 4. ELECTRON MAIN PROCESS

### `electron/main.ts`
- Create the BrowserWindow: `width: 1400, height: 900`, `minWidth: 900`, `minHeight: 600`, `frame: false` (custom titlebar), `titleBarStyle: 'hidden'`, `trafficLightPosition: { x: 12, y: 12 }` (macOS only), `webPreferences: { preload, contextIsolation: true, nodeIntegration: false }`
- Register all IPC handlers (see section 5)
- On `app.ready`: create window, register global shortcuts
- On `window-all-closed`: quit on non-macOS
- Handle `will-navigate` to block navigation away from the renderer URL

### `electron/preload.ts`
Expose `window.athena` via `contextBridge.exposeInMainWorld`:

```typescript
window.athena = {
  pty: {
    spawn: (id, cwd, shell, agentCmd?) => Promise<void>
    write: (id, data) => void
    resize: (id, cols, rows) => void
    kill: (id) => void
    onData: (id, cb) => () => void       // returns unsubscribe fn
    onExit: (id, cb) => () => void
  },
  fs: {
    readTree: (dir) => Promise<FileNode[]>
    readFile: (path) => Promise<string>
    writeFile: (path, content) => Promise<void>
    watchDir: (dir, cb) => () => void
    showOpenDialog: () => Promise<string | null>
    exists: (path) => Promise<boolean>
  },
  browser: {
    show: (bounds) => void
    hide: () => void
    navigate: (url) => void
    back: () => void
    forward: () => void
    reload: () => void
    onTitleChange: (cb) => () => void
    onUrlChange: (cb) => () => void
  },
  swarm: {
    readState: (dir) => Promise<SwarmState>
    writeState: (dir, state) => Promise<void>
    sendMessage: (dir, from, to, msg) => Promise<void>
    readMailbox: (dir, agentId) => Promise<MailboxMessage[]>
    watchState: (dir, cb) => () => void
  },
  store: {
    get: (key) => Promise<any>
    set: (key, value) => Promise<void>
  },
  window: {
    minimize: () => void
    maximize: () => void
    close: () => void
    isMaximized: () => Promise<boolean>
    platform: () => Promise<string>    // 'darwin' | 'win32' | 'linux'
  }
}
```

### `electron/ptyManager.ts`
- Maintain a `Map<string, IPty>` keyed by pane ID
- `spawn(id, cwd, shell, agentCmd?)`: spawn node-pty with the user's shell. If `agentCmd` is provided, send it as the first input after a 500ms delay to let the shell initialize
- `write(id, data)`: write raw data to the PTY
- `resize(id, cols, rows)`: call `pty.resize()`
- `kill(id)`: `pty.kill()`, remove from map
- Stream PTY data back to renderer via `mainWindow.webContents.send('pty:data:${id}', data)`
- **Error handling:** If spawn fails (binary not found, permission denied), send `pty:error:${id}` event with descriptive message

### `electron/fileSystem.ts`
- `readTree(dir)`: recursively read directory, return `FileNode[]` (max depth 6, skip `node_modules`, `.git`, `.next`, `dist`, `build`, `.ade`)
- `readFile(path)`: return UTF-8 string
- `writeFile(path, content)`: write UTF-8
- `watchDir(dir)`: use `fs.watch` recursively; debounce 200ms before emitting to renderer
- **Error handling:** Wrap all fs ops in try/catch, return `{ success: false, error: string }` on failure

### `electron/browserManager.ts`
- Use `WebContentsView` (NOT the deprecated `BrowserView`)
- `show(bounds)`: create a `WebContentsView`, add it to `mainWindow.contentView` via `mainWindow.contentView.addChildView(view)`, set bounds, load URL
- `hide()`: remove from `mainWindow.contentView` via `mainWindow.contentView.removeChildView(view)`
- `navigate(url)`: validate URL (prepend `https://` if missing)
- Forward `did-navigate` → `browser:urlChange` IPC event
- Forward `page-title-updated` → `browser:titleChange` IPC event

### `electron/swarmCoordinator.ts`
- All swarm state stored in `{workspaceDir}/.ade/swarm-state.json`
- All mailbox messages stored in `{workspaceDir}/.ade/mailbox/{agentId}.json`
- Watch the state file with `fs.watch` and emit diffs to renderer on change
- `sendMessage(dir, from, to, msg)`: read full mailbox file, append message, write entire file back (never stream-append JSON)
- **App-side polling:** Poll `swarm-state.json` every 5 seconds. If an agent hasn't updated its task within 90 seconds, mark status as `stalled` and notify renderer

---

## 5. IPC CHANNEL REGISTRY (SHARED CONTEXT)

All `ipcMain.handle` / `ipcMain.on` channels. Use these exact names:

| Channel | Direction | Description |
|---------|-----------|-------------|
| `pty:spawn` | R→M | Spawn a PTY session |
| `pty:write` | R→M | Write data to PTY |
| `pty:resize` | R→M | Resize PTY |
| `pty:kill` | R→M | Kill PTY |
| `pty:data:{id}` | M→R | Streaming PTY output |
| `pty:exit:{id}` | M→R | PTY process exited |
| `pty:error:{id}` | M→R | PTY spawn/runtime error |
| `fs:readTree` | R→M | Read directory tree |
| `fs:readFile` | R→M | Read file contents |
| `fs:writeFile` | R→M | Write file contents |
| `fs:showOpenDialog` | R→M | Open native dir dialog |
| `fs:watchDir` | R→M | Start watching dir |
| `fs:change:{dir}` | M→R | Dir changed event |
| `browser:show` | R→M | Show WebContentsView |
| `browser:hide` | R→M | Hide WebContentsView |
| `browser:navigate` | R→M | Navigate to URL |
| `browser:back` | R→M | Go back |
| `browser:forward` | R→M | Go forward |
| `browser:reload` | R→M | Reload |
| `browser:urlChange` | M→R | URL changed |
| `browser:titleChange` | M→R | Page title changed |
| `swarm:readState` | R→M | Read swarm state |
| `swarm:writeState` | R→M | Write swarm state |
| `swarm:sendMessage` | R→M | Send mailbox message |
| `swarm:readMailbox` | R→M | Read agent mailbox |
| `swarm:stateChange` | M→R | Swarm state updated |
| `store:get` | R→M | Get persisted value |
| `store:set` | R→M | Set persisted value |
| `window:minimize` | R→M | Minimize window |
| `window:maximize` | R→M | Maximize/restore |
| `window:close` | R→M | Close window |
| `window:isMaximized` | R→M | Get maximize state |
| `window:platform` | R→M | Get process.platform |

---

## 6. DATA TYPES (SHARED CONTEXT — `src/types/`)

### `workspace.ts`
```typescript
type AgentType = 'claude' | 'codex' | 'opencode' | 'gemini' | 'custom' | 'shell'

type GridTemplate = '1x1' | '1x2' | '2x2' | '2x3' | '3x3' | '3x4' | '4x4'

interface PaneConfig {
  id: string           // nanoid()
  agentType: AgentType
  customCmd?: string   // only when agentType === 'custom'
  label?: string
}

interface Space {
  id: string
  name: string
  dir: string          // absolute working directory path
  grid: GridTemplate
  panes: PaneConfig[]  // length must match grid cell count
  color: string        // hex color for tab indicator
  createdAt: number
  lastOpenedAt: number
}
```

### `terminal.ts`
```typescript
interface CommandBlock {
  id: string
  command: string
  output: string
  exitCode: number | null   // null = still running
  startedAt: number
  finishedAt: number | null
  collapsed: boolean
}

interface PtySession {
  paneId: string
  pid?: number
  status: 'idle' | 'running' | 'exited' | 'error'
  blocks: CommandBlock[]
  errorMessage?: string    // populated when status === 'error'
}
```

### `swarm.ts`
```typescript
type AgentRole = 'coordinator' | 'builder' | 'scout' | 'reviewer'
type SwarmTaskStatus = 'queued' | 'building' | 'review' | 'done' | 'blocked' | 'stalled'

interface SwarmTask {
  id: string
  title: string
  description: string
  assignedAgentId: string
  ownedFiles: string[]
  status: SwarmTaskStatus
  dependsOn: string[]
  createdAt: number
  completedAt: number | null
  lastUpdatedAt: number     // used for stall detection
}

interface SwarmAgent {
  id: string
  role: AgentRole
  agentType: AgentType
  paneId: string
  status: 'idle' | 'thinking' | 'writing' | 'waiting' | 'done' | 'blocked' | 'stalled'
  currentTask: string | null
  lastAction: string
  lastActionAt: number
}

interface MailboxMessage {
  id: string
  from: string
  to: string
  content: string
  timestamp: number
  read: boolean
}

interface SwarmState {
  id: string
  goal: string
  agents: SwarmAgent[]
  tasks: SwarmTask[]
  messages: MailboxMessage[]
  status: 'active' | 'paused' | 'completed'
  startedAt: number
}
```

### `task.ts`
```typescript
type KanbanStatus = 'todo' | 'in_progress' | 'in_review' | 'complete'

interface KanbanTask {
  id: string
  spaceId: string
  title: string
  description?: string
  assignedAgent?: AgentType
  status: KanbanStatus
  order: number
  createdAt: number
}
```

### `theme.ts`
```typescript
interface ThemeDefinition {
  name: ThemeName
  label: string
  type: 'dark' | 'light'
  colors: {
    bg: string
    bgSecondary: string
    bgTertiary: string
    border: string
    text: string
    textMuted: string
    textDim: string
    accent: string
    accentHover: string
    success: string
    error: string
    warning: string
    terminalBg: string
    terminalFg: string
    terminalCursor: string
    terminalSelection: string
  }
}
```

---

## 7. TERMINAL SYSTEM — COMMAND BLOCKS

Every command and its output should be grouped into a visual "command block."

### Command Block Strategy (Practical Approach)

Instead of relying on fragile OSC 133 shell integration, use a **hybrid approach**:

1. **Command input bar** above the xterm instance (like Warp). When the user types a command and presses Enter, create a `CommandBlock`, write it to the PTY, and capture output.
2. **Prompt detection** via regex on shell prompt patterns (`$`, `%`, `#`, `❯` at line start after output). This marks end of previous block's output.
3. **Fallback**: For AI agent sessions (Claude, Codex, etc.), the entire session is one continuous stream since agents don't emit prompt markers. Show as scrollable output.
4. The raw xterm.js terminal is always available as fallback — command blocks are a **UI layer on top**.

### `src/utils/commandParser.ts`
- Buffer incoming PTY data
- Detect prompt patterns to delimit command boundaries
- Emit `CommandBlock` events when a command completes
- Handle: multi-line commands, commands with no output, long-running processes

### CommandBlock UI (`CommandBlock.tsx`)
Each block renders:
- **Header:** command text (monospace, bright), exit code badge (green ✓ / red ✗), relative timestamp, collapse chevron
- **Body:** terminal output with ANSI color support (mini xterm.js instance, NOT plain text)
- **Collapsed:** body hidden, header shows first output line as preview
- Click header to toggle collapse
- Right-click → Copy command, Copy output, Re-run command

### TerminalPane header (28px)
- Agent type icon + name with colored dot
- Pane label (editable on double-click)
- Status badge (idle/running/error)
- Three dot menu → Restart, Change agent, Split horizontal, Split vertical, Close

---

## 8. WORKSPACE CREATION FLOW (`NewSpaceModal.tsx`)

Three-step modal — make it polished.

### Step 1 — Basic info
- Space name (autofocus text input)
- Working directory (text input + "Browse" → `window.athena.fs.showOpenDialog()`)

### Step 2 — Grid layout
`GridTemplateSelector.tsx`: clickable SVG layout thumbnails:

| Label | Grid | Cells |
|-------|------|-------|
| Solo | 1×1 | 1 |
| Split | 1×2 | 2 |
| Quad | 2×2 | 4 |
| Six | 2×3 | 6 |
| Nine | 3×3 | 9 |
| Twelve | 3×4 | 12 |
| Sixteen | 4×4 | 16 |

### Step 3 — Agent assignment
Per-cell dropdown:

| Option | CLI |
|--------|-----|
| Claude Code | `claude` |
| Codex | `codex` |
| OpenCode | `opencode` |
| Gemini CLI | `gemini` |
| Custom... | (text input) |
| Shell only | (no agent) |

### Tab color picker
8 swatches: `#6366f1`, `#22c55e`, `#f59e0b`, `#ef4444`, `#06b6d4`, `#a855f7`, `#f97316`, `#64748b`

"Launch Space" → create, persist, close modal, spawn all PTYs in parallel.

---

## 9. THEME SYSTEM

### 25 Themes in `src/themes/themes.ts`

Each `ThemeDefinition` uses the interface from section 6. Fill in all colors (bgSecondary, bgTertiary, border, text, etc.) for each theme — not just bg and accent.

**Dark themes:**

| Name | bg | accent |
|------|----|--------|
| `void` (DEFAULT) | `#0a0a0a` | `#6366f1` |
| `ghost` | `#111118` | `#a78bfa` |
| `plasma` | `#0d0d1a` | `#818cf8` |
| `carbon` | `#121212` | `#94a3b8` |
| `hex` | `#0f1117` | `#22d3ee` |
| `neon-tokyo` | `#0d0f1c` | `#f0abfc` |
| `obsidian` | `#13131a` | `#fb923c` |
| `nebula` | `#0c0e1a` | `#c084fc` |
| `storm` | `#0f1520` | `#38bdf8` |
| `infrared` | `#110a0a` | `#f87171` |
| `nova` | `#0a0f14` | `#34d399` |
| `stealth` | `#101010` | `#6b7280` |
| `hologram` | `#071a1a` | `#2dd4bf` |
| `dracula` | `#282a36` | `#bd93f9` |
| `athena` | `#0b0e13` | `#6366f1` |
| `synthwave` | `#1a0533` | `#f92aad` |
| `cybernetics` | `#080c14` | `#00ff9f` |
| `quantum` | `#090d16` | `#67e8f9` |
| `mecha` | `#0d1017` | `#fbbf24` |
| `abyss` | `#040408` | `#4f46e5` |

**Light themes:**

| Name | bg | accent |
|------|----|--------|
| `paper` | `#fafafa` | `#4f46e5` |
| `chalk` | `#f5f5f0` | `#7c3aed` |
| `solar` | `#fdf6e3` | `#b58900` |
| `arctic` | `#f0f4f8` | `#0284c7` |
| `ivory` | `#fffff0` | `#4338ca` |

### Applying themes
CSS custom properties on `<html>` via `document.documentElement.style.setProperty`. Tailwind references these variables. xterm.js theme updated via `terminal.options.theme`.

---

## 10. ATHENASWARM SYSTEM

### Launch flow
1. User clicks "Launch Swarm" in toolbar
2. `SwarmModal.tsx` → Goal textarea, team size slider (2–10), role assignment per slot. Enforce: exactly 1 Coordinator, ≥1 Builder
3. On launch: create `SwarmState`, write to `{dir}/.ade/swarm-state.json`, create mailbox dir, spawn PTY per agent, inject role prompt after 500ms

### Role prompts (injected via PTY write)

**Coordinator:** Break goal into tasks, assign to builders with file ownership, write plan to `.ade/swarm-state.json`, monitor every 30s, send messages via mailbox.

**Builder:** Read task from `swarm-state.json`, check mailbox, only modify `ownedFiles`, update status to `review` when done, report `blocked` if stuck.

**Scout:** Explore codebase, write report to `.ade/scout-report.md`, answer builder questions. Read-only.

**Reviewer:** Monitor for `review` tasks, read `ownedFiles`, approve (→`done`) or reject (→`building` + feedback). Write verdicts to `.ade/reviews/{taskId}.md`.

### App-side orchestration (CRITICAL)
Do NOT rely solely on agents self-reporting. `swarmCoordinator.ts` must:
- Poll `swarm-state.json` every 5 seconds
- Track `lastUpdatedAt` — if no update in 90s, mark `stalled`
- Show "Nudge Agent" button that re-injects task prompt
- Allow manual status overrides via Swarm Board drag-and-drop

### Swarm Board UI
**Left:** Task cards in Kanban columns (Queued → Building → Review → Done). Draggable for manual override.
**Right:** Activity feed — `[HH:MM:SS] [ROLE] Agent — action`. Colors: Coordinator=indigo, Builder=emerald, Scout=amber, Reviewer=violet.
**Toolbar:** Pause All, Resume All, Abort Swarm.

---

## 11. BROWSER PANEL

- Toggle: `Cmd+B` / `Ctrl+B`
- Uses `WebContentsView` (NOT deprecated `BrowserView`)
- `BrowserPanel.tsx` renders placeholder div; `WebContentsView` in main process overlays it
- On mount: `window.athena.browser.show(getBoundingClientRect())`
- On unmount: `window.athena.browser.hide()`
- On resize (ResizeObserver): recalculate bounds
- Toolbar renders ABOVE the WebContentsView: Back, Forward, Reload, editable URL bar (auto-prepends `https://`), open in system browser, close

---

## 12. CODE EDITOR PANEL

- Toggle: `Cmd+E` / `Ctrl+E`
- Right ~40% of main area; terminals shrink via `react-resizable-panels`
- **File Explorer** in sidebar: tree from current Space dir, right-click context menu
- **Monaco** via `@monaco-editor/react`: auto-detect language, tab bar with dirty indicator, auto-save after 1s, file change detection banner
- **Quick Open** (`Cmd+P`): fuzzy search overlay, max 20 results

---

## 13. KANBAN TASK BOARD

Toggle: `Cmd+K` / `Ctrl+K`. Columns: todo → in_progress → in_review → complete.

### KanbanCard
- Editable title, expandable description, agent badge
- **"Run Task"**: find/create pane for agent → focus → inject `{agentCli} "{title}: {description}"` → move to `in_progress`
- Drag between columns changes status
- "+ Add Task" at bottom of Todo, quick-add with Enter

---

## 14. KEYBOARD SHORTCUTS

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl+T` | New workspace tab |
| `Cmd/Ctrl+W` | Close current tab |
| `Cmd/Ctrl+P` | Quick Open |
| `Cmd/Ctrl+F` | Terminal search in active pane |
| `Cmd/Ctrl+D` | Split active terminal |
| `Cmd/Ctrl+B` | Toggle browser |
| `Cmd/Ctrl+E` | Toggle editor |
| `Cmd/Ctrl+K` | Toggle Kanban |
| `Cmd/Ctrl+,` | Settings |
| `Cmd/Ctrl+1–9` | Switch tab by index |
| `Cmd/Ctrl+Shift+S` | Launch Swarm |
| `Cmd/Ctrl+\` | Toggle sidebar |
| `Escape` | Close modal/overlay |

---

## 15. APP LAYOUT

```
┌─────────────────────────────────────────────────────────┐
│  Titlebar (draggable, traffic lights / window controls)  │
├──────────┬──────────────────────────────────────────────┤
│          │  WorkspaceTabs                               │
│          ├──────────────────────────────────────────────┤
│ Sidebar  │  Main Content Area                           │
│  240px   │  (TerminalGrid | SwarmBoard | KanbanBoard)   │
│  - Spaces│                                              │
│  - Files │  ← react-resizable-panels splits with        │
│  - Agents│    EditorPanel when open                     │
│          ├──────────────────────────────────────────────┤
│          │  BrowserToolbar (when browser open)           │
│          │  [WebContentsView overlays below]             │
└──────────┴──────────────────────────────────────────────┘
Status Bar (22px) — space name, shell, terminal count, swarm status, theme
```

- **Sidebar:** default 240px, resizable 180–400px, collapsible
- **Titlebar:** 38px, app name left, tabs center, window controls right (left on macOS). Use `window.athena.window.platform()` to detect
- **WorkspaceTabs:** 36px, color dot per tab, double-click rename, right-click menu

---

## 16. SETTINGS MODAL

`Cmd+,` → full-screen overlay with tab sections:

1. **General**: default shell, launch at login, restore session, font family (JetBrains Mono/Fira Code/Cascadia Code/Menlo/Consolas), font size (10–24, default 14)
2. **Agents**: path override per agent type + "Test" button
3. **Themes**: `ThemePicker.tsx` — 25 swatches (80×60px cards), click to select
4. **Shortcuts**: read-only reference table
5. **About**: app version, electron version

---

## 17. BUILD CONFIGURATION

### `package.json` — ALL dependencies
```json
{
  "name": "athenas-core",
  "productName": "Athena's Core",
  "version": "1.0.0",
  "dependencies": {
    "node-pty": "^1.0.0",
    "electron-store": "^10.0.0"
  },
  "devDependencies": {
    "electron": "^32.0.0",
    "electron-builder": "^24.0.0",
    "electron-vite": "^2.3.0",
    "vite": "^5.0.0",
    "@vitejs/plugin-react": "^4.0.0",
    "react": "^19.0.0",
    "react-dom": "^19.0.0",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "typescript": "^5.5.0",
    "tailwindcss": "^3.4.0",
    "postcss": "^8.4.0",
    "autoprefixer": "^10.4.0",
    "zustand": "^5.0.0",
    "immer": "^10.0.0",
    "@xterm/xterm": "^5.5.0",
    "@xterm/addon-webgl": "^0.18.0",
    "@xterm/addon-fit": "^0.10.0",
    "@monaco-editor/react": "^4.6.0",
    "react-resizable-panels": "^2.0.0",
    "@dnd-kit/core": "^6.1.0",
    "@dnd-kit/sortable": "^8.0.0",
    "lucide-react": "^0.400.0",
    "framer-motion": "^11.0.0",
    "nanoid": "^5.0.0"
  }
}
```

### `electron-builder.yml`
```yaml
appId: com.athenas-core.app
productName: "Athena's Core"
directories:
  output: dist-app
  buildResources: resources
files:
  - dist-electron/**/*
  - dist/**/*
mac:
  target: dmg
  category: public.app-category.developer-tools
win:
  target: nsis
linux:
  target:
    - deb
    - AppImage
nsis:
  oneClick: false
  allowToChangeInstallationDirectory: true
extraResources:
  - from: node_modules/node-pty/build
    to: node-pty/build
```

### `vite.config.ts`
Use `electron-vite` built-in config. Renderer: React plugin + Tailwind. Main/preload: Node.js target with `externalizeDepsPlugin` (keep node-pty, electron-store as externals).

---

## 18. VISUAL DESIGN

- **Font:** System stack for UI. `'JetBrains Mono', 'Fira Code', monospace` for terminals/code.
- **Spacing:** 4px base grid, Tailwind spacing scale
- **Radius:** panels `rounded-lg` (8px), buttons `rounded-md` (6px), badges `rounded-full`
- **Borders:** subtle `1px solid var(--border)`, no heavy shadows
- **Transitions:** 150ms hover, 200ms panel open/close
- **Scrollbars:** 4px width, `var(--bgTertiary)` thumb, transparent track
- **Agent colors:** Claude=`#d97706`, Codex=`#10b981`, OpenCode=`#3b82f6`, Gemini=`#8b5cf6`, Custom=`#6b7280`, Shell=`#64748b`
- **Swarm role colors:** Coordinator=`#6366f1`, Builder=`#22c55e`, Scout=`#f59e0b`, Reviewer=`#a855f7`

---

## 19. ERROR HANDLING

Every IPC handler in `main.ts` must wrap logic in try/catch and return structured errors: `{ success: false, error: string }`.

### Toast notifications (`shared/Toast.tsx`)
Show errors as toasts (bottom-right, auto-dismiss 5s). Specific cases:
- **PTY spawn failure** → "Could not start {shell}. Check Settings → Agents."
- **File permission error** → "Permission denied: {path}"
- **Agent binary not found** → "{agent} not found on PATH. Configure in Settings → Agents."
- **Workspace dir deleted** → banner: "Workspace directory no longer exists", disable panes
- **Swarm agent stalled** → "Agent {name} hasn't responded in 90s" + Nudge button
- **Browser navigation error** → "Could not load {url}" in browser toolbar

---

## 20. BUILD ORDER — IMPLEMENT IN THIS SEQUENCE

Re-read sections 2, 5, and 6 before each phase. Verify checkpoint before proceeding.

### Phase 1 — Electron Shell
1. Scaffold with `electron-vite`. Verify `npm run dev` opens blank window.
2. Implement `main.ts` with correct window options.
3. Implement `preload.ts` with full `window.athena` API (stubs first — `console.log` + `Promise.resolve`).
4. Custom titlebar with working minimize/maximize/close.
5. Basic layout (sidebar + main area) with Tailwind dark theme.
**✓ Checkpoint:** App opens, titlebar buttons work, dark theme visible.

### Phase 2 — PTY Terminals
6. Implement `ptyManager.ts` with node-pty.
7. Register `pty:*` IPC channels.
8. Implement `useTerminal.ts` — xterm.js init, WebGL addon, fit addon, ResizeObserver.
9. Implement `TerminalPane.tsx` + `TerminalGrid.tsx` with hardcoded 2×2.
**✓ Checkpoint:** 4 terminals visible, type in them, real shell output appears.

### Phase 3 — Workspace System
10. Implement `workspaceStore.ts` with Zustand + electron-store persistence.
11. Implement `NewSpaceModal.tsx` — all 3 steps.
12. Implement `GridTemplateSelector.tsx` + `AgentPicker.tsx`.
13. Implement `WorkspaceTabs.tsx`.
**✓ Checkpoint:** Create a Space, pick layout, assign agents, verify auto-launch.

### Phase 4 — Command Blocks
14. Implement `commandParser.ts` with prompt detection.
15. Implement `CommandBlock.tsx` + `CommandBlockList.tsx`.
16. Integrate into `TerminalPane.tsx`.
**✓ Checkpoint:** Run a command, see it as a block with exit code badge.

### Phase 5 — File Explorer + Editor
17. Implement `fileSystem.ts`.
18. Implement `FileExplorer.tsx` + `FileTreeNode.tsx`.
19. Implement `editorStore.ts` + `EditorPanel.tsx` + `EditorTabs.tsx`.
20. Implement `QuickOpen.tsx`.
**✓ Checkpoint:** Open file from tree, edit, auto-save works, Quick Open finds files.

### Phase 6 — Browser
21. Implement `browserManager.ts` with WebContentsView.
22. Implement `BrowserPanel.tsx` + `BrowserToolbar.tsx`.
23. Register `browser:*` IPC channels.
**✓ Checkpoint:** Cmd+B opens browser, URL bar works, navigation works.

### Phase 7 — Kanban
24. Implement `taskStore.ts`.
25. Implement `KanbanBoard.tsx` + `KanbanColumn.tsx` + `KanbanCard.tsx` with @dnd-kit.
26. Implement "Run Task" → PTY inject.
**✓ Checkpoint:** Create tasks, drag between columns, run a task.

### Phase 8 — Swarm
27. Implement `swarmCoordinator.ts` with polling + stall detection.
28. Implement `swarmStore.ts` + `SwarmModal.tsx` + `SwarmLauncher.tsx`.
29. Implement `SwarmBoard.tsx` + `SwarmActivityFeed.tsx`.
30. Implement role prompt injection.
**✓ Checkpoint:** Launch swarm, agents in panes, activity feed updates.

### Phase 9 — Themes + Settings
31. Implement all 25 themes in `themes.ts` (all color properties, not just bg/accent).
32. CSS variable injection + xterm theme sync.
33. Implement `SettingsModal.tsx` with all 5 sections.
**✓ Checkpoint:** Switch themes, app and terminals both update.

### Phase 10 — Polish
34. Register all keyboard shortcuts (section 14).
35. Add status bar.
36. Add framer-motion transitions.
37. Custom scrollbar styles.
38. Window resize handling (WebContentsView bounds, xterm refit).
39. Test cold launch, workspace persistence across restarts.
40. `npm run build` — fix all TS errors and warnings.

---

## 21. COMMON MISTAKES TO AVOID

1. **Never** use `nodeIntegration: true`. All Node.js access through preload contextBridge.
2. **Never** import `electron` in renderer. Only `window.athena.*`.
3. **node-pty** in main process only — native module fails in renderer.
4. **Monaco**: use `@monaco-editor/react`, not raw `monaco-editor`.
5. **WebContentsView bounds**: recalculate on every window AND panel resize (ResizeObserver).
6. **xterm.js**: call `fitAddon.fit()` on container resize (ResizeObserver). Load WebGL addon before DOM attach. Dispose on pane close.
7. **electron-store v10** is ESM. In main process (CJS): `const { default: Store } = await import('electron-store')`.
8. **Swarm mailbox**: read full file → append → write full file. Never stream-append JSON.
9. **File tree**: async `readTree`, respect depth limit. No sync fs calls on main thread.
10. **Platform detection**: use `window.athena.window.platform()` in renderer for OS-specific behavior (titlebar button position, shortcuts).

---

## 22. DEFINITION OF DONE

- [ ] App launches with void theme, custom titlebar, empty sidebar
- [ ] "New Space" modal — all 3 steps work, persists across restarts
- [ ] Terminal grid shows correct layout, all PTYs are real shells
- [ ] Agent CLI auto-launches in assigned pane
- [ ] Commands appear as collapsible blocks with exit code badges
- [ ] File explorer shows directory tree, files open in Monaco
- [ ] Quick Open (Cmd+P) with fuzzy search works
- [ ] Auto-save writes to disk after 1s inactivity
- [ ] Cmd+B opens embedded browser, URL bar navigates
- [ ] Kanban board: 4 columns, drag cards, "Run Task" works
- [ ] "Launch Swarm" spawns agents with role prompts
- [ ] Swarm activity feed updates in real time
- [ ] Stalled agent detection works (90s timeout)
- [ ] All 25 themes render correctly in both app and terminals
- [ ] All keyboard shortcuts work
- [ ] Error toasts show for common failure cases
- [ ] `npm run build` succeeds (no TS errors)
