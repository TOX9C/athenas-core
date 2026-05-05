# Warp vs Athena's Core — UI/UX Comparison

> A comprehensive analysis of UI/UX architectural differences between [Warp](https://github.com/warpdotdev/warp) (Rust/GPU-native terminal) and Athena's Core (Electron/React desktop app).

---

## Table of Contents

1. [Rendering & Performance](#rendering--performance)
2. [Terminal Data Model](#terminal-data-model)
3. [Command/Output Boundaries](#commandoutput-boundaries)
4. [Input Handling](#input-handling)
5. [Rich Content in Terminal](#rich-content-in-terminal)
6. [Block Affordances](#block-affordances)
7. [AI Integration Model](#ai-integration-model)
8. [AI Features](#ai-features)
9. [Command Palette](#command-palette)
10. [Layout & Component Model](#layout--component-model)
11. [Workspace Management](#workspace-management)
12. [Styling & Theming](#styling--theming)
13. [Scrollback & History](#scrollback--history)
14. [UX Philosophy](#ux-philosophy)
15. [What Athena Has That Warp Doesn't](#what-athena-has-that-warp-doesnt)
16. [What Warp Has That Athena Doesn't](#what-warp-has-that-athena-doesnt)
17. [Side-by-Side Summary Table](#side-by-side-summary-table)

---

## Rendering & Performance

| Aspect            | Warp                                                                | Athena's Core                                                     |
| ----------------- | ------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Language          | Rust (native)                                                       | TypeScript/JavaScript                                             |
| Rendering         | GPU-accelerated — Metal (macOS), DirectX (Windows), WGSL (Web/WASM) | Electron DOM + xterm.js with WebGL addon (falls back to Canvas2D) |
| UI Framework      | Custom WarpUI (Flutter-inspired, entity-component-handle pattern)   | React 19 + Tailwind CSS 3.4                                       |
| Performance       | 400+ fps, ~1.9ms average screen redraw (4K capable)                 | ~60fps (Electron/DOM overhead)                                    |
| Shader pipeline   | ~200 lines for 3 primitives: rectangles, images, glyphs             | N/A (DOM-based)                                                   |
| Rendering backend | Pluggable — add a platform by reimplementing ~250-line shader layer | Single platform (Chromium/Electron)                               |

**Key difference**: Warp owns the entire rendering pipeline from GPU shader to pixel. Athena delegates to Chromium's DOM compositor and xterm.js's canvas renderer. Warp can sustain 144Hz+ refresh on 4K; Athena is capped by Electron's frame budget.

---

## Terminal Data Model

| Aspect                | Warp                                                                                                                                                                                                | Athena's Core                                                                                                        |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------- |
| Core abstraction      | **BlockList** — ordered list of typed, self-contained blocks                                                                                                                                        | Single flat grid (xterm.js `Terminal` model)                                                                         |
| Data structure        | **SumTree** — balanced tree aggregating per-block heights at every interior node. O(log n) viewport queries regardless of session length                                                            | xterm.js `Buffer` — rows-of-cells array, linear scan for viewport                                                    |
| Active region storage | **GridStorage** (from Alacritty's `Grid`) — rows-of-cells, mutable, random access, pre-allocated                                                                                                    | xterm.js `IBuffer` — rows-of-cells, mutable                                                                          |
| Scrollback storage    | **FlatStorage** — packed byte buffer + small row index mapping row numbers to byte offsets. Styling in separate interval maps keyed on byte offsets. Only row index rebuilds on resize, not styling | xterm.js scrollback buffer — rows-of-cells (same format as active), memory scales with content                       |
| Memory efficiency     | FlatStorage dramatically reduces memory for blocks with long output (server logs, build output). Styling stored separately from content                                                             | Every row pre-allocated as a cell array, including scrollback. Memory scales with grid dimensions × scrollback lines |
| Block types           | Terminal blocks (command + output), Rich content blocks (AI, agent, UI)                                                                                                                             | No block concept — all output is an undifferentiated character stream                                                |
| Virtualization        | Two-level: (1) BlockList level — only blocks intersecting viewport render, (2) Terminal block level — only rows within viewport render                                                              | xterm.js viewport rendering — only visible rows drawn to canvas                                                      |

**Key difference**: Warp's BlockList is the architectural spine of the entire app. Every feature (scrolling, rendering, search, sharing, AI) flows from it. Athena uses the traditional terminal model where output is a flat undifferentiated stream with no structural awareness.

---

## Command/Output Boundaries

| Aspect                | Warp                                                                                                                                                       | Athena's Core                                                                                         |
| --------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| Boundary detection    | **Shell hooks** — bootstrap scripts (bash, zsh, fish, PowerShell) hook into `precmd`/`preexec` and emit custom **DCS escape sequences** with JSON payloads | None — boundaries are invisible to the renderer                                                       |
| Payload data          | Command text, exit code, working directory, git state, duration                                                                                            | N/A                                                                                                   |
| Communication channel | Bidirectional — Warp can inject commands and instruct the shell to wrap responses in the same escape-sequence markers                                      | Unidirectional PTY stdin/stdout                                                                       |
| Failed command visual | Red background + red sidebar on block                                                                                                                      | Colored pane border (orange=input, red=error, blue=thinking, green=ready) — per-pane, not per-command |

**Key difference**: Warp defines a small contract with the shell. Athena has no shell integration — agent status is tracked via separate IPC, not from shell semantics. Warp knows what you ran and what it produced; Athena only knows that a PTY session is active.

---

## Input Handling

| Aspect            | Warp                                                                                                                                                                                     | Athena's Core                                                                            |
| ----------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------- |
| Input type        | **Full rich-text editor** with multi-cursor, multi-select, CRDT-based (collaboration-ready)                                                                                              | Simple text input (Athena chat) or raw PTY stdin (terminals)                             |
| Data structure    | **SumTree** — custom Rope variant holding generic types, indexing on multiple dimensions. Separate from buffer text for transforms (code folding, annotations) and operation history     | Plain `<textarea>` / `<input>` elements                                                  |
| Undo system       | Operation-based CRDT — designed for real-time collaboration from day one                                                                                                                 | Browser default undo                                                                     |
| Multi-cursor      | Yes (VS Code/Sublime-style)                                                                                                                                                              | No                                                                                       |
| Keybinding system | Action dispatch — keybindings trigger named actions, context-sensitive enable/disable. Supports both legacy shortcuts (up-arrow, ctrl-r, tab) and modern ones (Select All, Move By Word) | Global keyboard shortcuts via `useEffect` listeners in App.tsx                           |
| Input positioning | Pinned to bottom (default), pinned to top, or starting at top                                                                                                                            | AthenaInput always at bottom of chat panel; terminal input is inline (xterm.js readline) |
| Mode detection    | **Natural language detection** (`crates/natural_language_detection/`) — routes input to AI vs shell automatically                                                                        | Manual — user explicitly opens Athena panel for AI, terminal for shell                   |
| Vim mode          | Full vim keybinding layer for the input editor (`crates/vim/`)                                                                                                                           | No                                                                                       |

**Key difference**: Warp's input area is a structural editor rivaling VS Code's. Athena's input is a standard text field. Warp auto-detects whether you're typing a shell command or a natural language query; Athena requires the user to choose the right panel.

---

## Rich Content in Terminal

| Aspect               | Warp                                                                                                                                         | Athena's Core                                                                                               |
| -------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| Approach             | Rich content blocks render **inline alongside terminal blocks** in the same scroll stream                                                    | Rich content lives in the **separate AthenaPanel** (right sidebar). Terminal panes only show raw PTY output |
| Agent conversations  | Appear as rich content blocks in the terminal viewport                                                                                       | Appear in the AthenaPanel chat stream, never in the terminal                                                |
| Agent shell commands | Render as normal terminal blocks within the agent's block sequence                                                                           | Sent to PTY sessions, visible in TerminalPane, but not linked to the AI conversation visually               |
| Diff proposals       | Rendered inline in the block stream                                                                                                          | Not supported — Athena shows AI responses as text in chat                                                   |
| Collapse/expand      | Blocks can be collapsed/expanded. Background agent commands hidden by default; agent-specific content hidden when a different view is active | AthenaPanel sections are not collapsible. Agent output is in a separate AgentInspector panel                |
| Block sharing        | Share a command+output block as a link (web view)                                                                                            | Not supported                                                                                               |
| Sticky headers       | Command name pins at top of visible area when output scrolls out of view                                                                     | No — terminal scrolls as a flat buffer                                                                      |

**Key difference**: Warp merges AI and terminal into one continuous scroll stream. Athena keeps them as separate panels. In Warp, seeing what an agent did means scrolling your terminal. In Athena, it means switching to the Athena panel or Agent Inspector.

---

## Block Affordances

| Aspect                  | Warp                                                             | Athena's Core                                                            |
| ----------------------- | ---------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Navigate between blocks | Cmd+Up/Down to jump between commands                             | No block concept — standard terminal scroll                              |
| Select entire output    | One gesture selects all output for a command                     | Must manually select text in terminal                                    |
| Sticky command header   | Command stays pinned at top when output scrolls                  | No equivalent                                                            |
| Color-coded blocks      | Failed commands (non-zero exit) get red background + red sidebar | Pane border colors indicate agent status, not individual command results |
| Per-block search        | Search within a specific block's output                          | Search within entire terminal buffer (xterm.js find addon)               |
| Block actions           | Run follow-up actions on a whole block at once                   | No equivalent                                                            |

**Key difference**: Warp treats each command+output pair as a first-class interactive object. Athena treats the terminal as a traditional scrollable text surface.

---

## AI Integration Model

| Aspect             | Warp                                                                                                                                   | Athena's Core                                                                                                                    |
| ------------------ | -------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------- |
| AI location        | **Inline in the terminal stream** — AI blocks sit alongside command blocks                                                             | **Separate right-side panel** (AthenaPanel, 400px, expandable to 500px with session list)                                        |
| AI ↔ terminal link | Agent commands appear as terminal blocks in the same viewport. Agent reasoning appears as rich content blocks. The two are interleaved | AI sends commands to PTY sessions but the AI conversation and terminal output live in separate UI regions with no visual linkage |
| AI input           | Type natural language directly in the terminal input editor — auto-detected and routed to AI                                           | Type in a dedicated chat input field with image attachment support                                                               |
| AI responses       | Render as rich content blocks inline                                                                                                   | Render as chat messages in the sidebar panel                                                                                     |
| Multi-agent        | Built-in Warp Agent + external agents (Claude Code, Codex, Gemini CLI, Opencode). ACP (Agent Client Protocol) planned                  | Custom agents (user-defined CLI commands) + built-in Athena agent. Agent output captured via IPC, viewable in AgentInspector     |
| Agent visibility   | Agent activity appears directly in your terminal stream                                                                                | Agent activity tracked by agentStatusStore. Status dots on pane headers. Output viewable in separate AgentInspector panel        |

**Key difference**: Warp dissolves the boundary between "AI" and "terminal." Athena enforces a strict spatial separation. This is the single most consequential UX difference between the two apps.

---

## AI Features

| Warp                                                        | Athena's Core                                                              |
| ----------------------------------------------------------- | -------------------------------------------------------------------------- |
| Natural language detection in input (routes to AI vs shell) | Chat-based Q&A in dedicated panel                                          |
| Inline command suggestions in input editor                  | Image attachments (paste/drag-drop/file picker)                            |
| Command explanation (select → explain)                      | Plan blocks with step progress tracking                                    |
| Voice input for AI queries                                  | Ask-user interactive choice blocks                                         |
| AI-powered tab completions (Fig Completion Specs)           | Evaluation blocks with per-step status and reasoning                       |
| Model selector UI (79KB module)                             | Thinking indicator with rotating labels, pulsing ring, tool activity state |
| Context chips for AI session state                          | Custom agent definitions (name + CLI command)                              |
| Command corrections for failed commands                     | Streaming status log with step dots                                        |
| Computer use capabilities (cloud agents)                    | Session management with create/switch/delete                               |

---

## Command Palette

| Aspect          | Warp                                                                                                                                                      | Athena's Core                                             |
| --------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------- |
| Access          | Cmd+P (macOS) / Ctrl+Shift+P (Windows/Linux)                                                                                                              | **None**                                                  |
| Search scope    | Actions, workflows, prompts, notebooks, env vars, files, sessions, launch configs — prefix-filtered categories (`workflows:`, `prompts:`, `files:`, etc.) | N/A                                                       |
| Architecture    | Unified search interface across all app features                                                                                                          | Navigation is sidebar + tab-based only                    |
| Quick file open | Included (via `files:` prefix)                                                                                                                            | Stub implementation (QuickOpen in editor, non-functional) |

**Key difference**: Warp has a VS Code-style unified search across all app capabilities. Athena has no command palette — users navigate via sidebar clicks and tab interactions only.

---

## Layout & Component Model

### Warp

- **Custom WarpUI framework** — Flutter-inspired entity-component-handle pattern
- Global `App` object owns all views/models. Views hold `ViewHandle<T>` references
- Elements describe visual layout (like Flutter widgets), compose into GPU primitives
- Actions system for event dispatch (keybindings → actions → handlers)
- No DOM, no CSS, no HTML — everything is drawn directly to GPU

### Athena's Core

- **React 19** component tree with **Tailwind CSS 3.4**
- 12 Zustand stores for state (uiStore, athenaStore, workspaceStore, etc.)
- `panelManager.ts` — mediator pattern for exclusive panel activation (athena/browser/editor are mutually exclusive)
- IPC bridge via `window.athena.*` global (strict preload isolation)
- Layout: `react-resizable-panels` (imported but not heavily used) + manual flex/grid
- Component hierarchy: App → Titlebar + Sidebar + MainPanel + OverlayPanels + Modals + EventBuses
- All UI components are **custom-built from scratch** — no component library (no Radix, MUI, shadcn)
- `lucide-react` for icons, pure CSS animations

### Root Layout Comparison

```
Warp:
┌─────────────────────────────────────────┐
│ Tabs                                     │
├──────────┬──────────────────────────────┤
│          │  Block 1: cmd + output        │
│  Sidebar │  Block 2: cmd + output        │
│  (opt)   │  Block 3: AI conversation     │
│          │  Block 4: cmd + output        │
│          │  ─────────────────────────    │
│          │  Input Editor (rich text)     │
└──────────┴──────────────────────────────┘

Athena:
┌──────────────────────────────────────────────────┐
│ Titlebar (tabs + panel switchers + notifications) │
├────────┬──────────────────────┬──────────────────┤
│        │                      │                  │
│Sidebar │  Main Panel          │  Athena Panel    │
│        │  (TerminalGrid /     │  (AI chat -      │
│        │   KanbanBoard /      │   separate       │
│        │   SwarmBoard)        │   right panel)   │
│        │                      │                  │
├────────┴──────────────────────┴──────────────────┤
│ Status bar                                        │
└──────────────────────────────────────────────────┘
```

---

## Workspace Management

| Aspect             | Warp                                                                         | Athena's Core                                                                                                                                                    |
| ------------------ | ---------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Tabs               | Horizontal (default) or vertical (sidebar with metadata, drag-and-drop)      | Workspace tabs with colored dots in titlebar                                                                                                                     |
| Tab configs        | Reusable tab setups defined in TOML files                                    | No tab persistence                                                                                                                                               |
| Split panes        | Divide a tab into multiple rectangular panes, each a unique terminal session | Auto-grid templates (1×1 through 4×4) based on pane count. No manual split                                                                                       |
| Layout persistence | **Launch configurations** — save/restore window/tab/pane layouts             | No layout persistence. Spaces lost on app restart (except terminal session history)                                                                              |
| Multiple spaces    | Single window with tabs + panes                                              | **Named workspaces** ("spaces") each with independent pane grids. `mountedSpaces` pattern keeps TerminalGrids alive when hidden to preserve xterm state          |
| Crash recovery     | Session restoration via `crash_recovery.rs`                                  | No crash recovery                                                                                                                                                |
| New space creation | New tab button                                                               | **NewSpaceModal** — 2-step wizard: choose mode (Terminal/Swarm), configure directory + agent count (up to 16 panes), live xterm preview for directory navigation |

**Key difference**: Warp is tab/pane-centric within a single window. Athena is workspace-centric with multiple named spaces. Warp persists layouts; Athena doesn't. Athena's NewSpaceModal with a live embedded terminal for directory navigation is a unique UX pattern.

---

## Styling & Theming

| Aspect        | Warp                                                                                                                            | Athena's Core                                                                                                                                                                                   |
| ------------- | ------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Theme count   | 21+ built-in                                                                                                                    | **25 built-in** (20 dark, 5 light)                                                                                                                                                              |
| Theme format  | Typed Rust struct (`WarpTheme`)                                                                                                 | TypeScript object with 16 color tokens                                                                                                                                                          |
| Custom themes | Theme creator from uploaded images, manual creation                                                                             | Manual only (edit `themes.ts`)                                                                                                                                                                  |
| Theme preview | **Transient preview** — live update, commit with checkmark, revert with X                                                       | Live apply on selection (no preview/commit flow)                                                                                                                                                |
| OS sync       | Automatic sync with macOS/Windows light/dark mode — select separate themes for each                                             | No OS theme sync                                                                                                                                                                                |
| Font system   | 3 separate font settings: monospace (default: Hack), AI font, UI font (default: Roboto). Font weight + line height configurable | Single configurable font (default: JetBrains Mono). No separate UI/AI font                                                                                                                      |
| CSS variables | N/A (no DOM)                                                                                                                    | `--bg`, `--bgSecondary`, `--bgTertiary`, `--border`, `--text`, `--textMuted`, `--textDim`, `--accent`, `--accentHover`, `--success`, `--error`, `--warning`, `--terminalBg/Fg/Cursor/Selection` |
| Transparency  | `color-mix()` CSS function for semi-transparent variants without separate variables                                             | Same — Athena also uses `color-mix(in srgb, var(--accent) 12%, transparent)`                                                                                                                    |
| Persistence   | Diesel + SQLite                                                                                                                 | `electron-store`                                                                                                                                                                                |

**Key difference**: Warp has a more sophisticated theme workflow (preview/commit, OS sync, image-based creation). Athena has more built-in themes (25 vs 21+) but a simpler application model. Warp separates fonts by context (terminal/AI/UI); Athena uses one monospace font everywhere.

---

## Scrollback & History

| Aspect              | Warp                                                                                                                           | Athena's Core                                                                                                              |
| ------------------- | ------------------------------------------------------------------------------------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------------- |
| Active region       | GridStorage (rows-of-cells, mutable, random access)                                                                            | xterm.js buffer (rows-of-cells, mutable)                                                                                   |
| Scrollback          | FlatStorage (packed byte buffer + row index). Styling in interval maps keyed on byte offsets. Memory-efficient for long output | xterm.js scrollback buffer (rows-of-cells, same format as active). Memory scales with lines × width                        |
| Resize behavior     | Only row index rebuilds, not styling (because styling is keyed on byte offsets, not row/col)                                   | Entire buffer reflows on resize                                                                                            |
| Command history     | Rich metadata (command text, exit code, cwd, duration, git state). Searchable. 38KB module                                     | Raw text in xterm.js. Chat input history (last 100 messages in localStorage). Terminal session history saved by ptyManager |
| Session persistence | Crash recovery + session restoration                                                                                           | Chat sessions persisted via `window.athena.session.*` IPC. Terminal session history saved, but no crash recovery           |

---

## UX Philosophy

### Warp: "Terminal as Application"

Warp fundamentally rejects the "terminal as a dumb text viewport" model. It treats the terminal as a modern application with:

- Typed data structures (blocks, not character grids)
- Interactive elements (selectable blocks, inline AI, rich content)
- Blocks as the core abstraction — everything flows from them (scrolling, rendering, search, sharing, AI)
- Shell integration as a protocol contract, not raw PTY parsing
- Maximum control via custom stack (own UI framework, own editor, own renderer, own GPU pipeline)
- Built for the agentic era — the block model's extensibility (rich content blocks) was the perfect foundation for embedding AI agent conversations alongside terminal commands

### Athena's Core: "Terminal as Component"

Athena treats the terminal as one feature among many in a multi-purpose desktop IDE:

- Terminals coexist with Kanban boards, AI chat, code editors, browsers, and swarm orchestration
- Strict spatial separation between features (sidebar, main panel, overlay panels)
- Pragmatic stack (Electron + React + Tailwind) — leverages web ecosystem over bespoke native rendering
- IPC-first architecture — all system access through `window.athena.*` bridge, strict preload isolation
- Agent-aware terminals — panes know which agent they belong to, but the terminal surface itself is still a traditional character grid
- Workspace-centric — multiple named spaces with independent configurations, rather than Warp's tab/pane model

---

## What Athena Has That Warp Doesn't

| Feature                              | Description                                                                                                                                                                         |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Kanban task board**                | 4-column drag-and-drop board (Todo → In Progress → In Review → Complete) with agent task assignment, inline task creation, "Run task" sends prompt to agent PTY                     |
| **Swarm orchestration UI**           | Multi-agent board with role badges (Coordinator/Builder/Scout/Reviewer), activity feed sidebar, nudge capability for stalled agents, swarm state written to `.ade/swarm-state.json` |
| **Embedded browser panel**           | Electron BrowserView with toolbar (back/forward/reload, URL bar, "Open in system browser")                                                                                          |
| **Monaco code editor**               | File tabs with dirty indicators, auto-save (1s debounce), QuickOpen fuzzy file search, read-only mode                                                                               |
| **File explorer sidebar**            | Tree view of workspace directory                                                                                                                                                    |
| **Plugin dashboard**                 | Search, status filter, enable/disable toggles, plugin cards with name/version/author/capabilities                                                                                   |
| **Agent inspector**                  | Dedicated panel with 3 tabs: Output (line numbers + timestamps), Status (progress bar), Notifications (filterable)                                                                  |
| **Workspace concept**                | Multiple named spaces with independent pane grids, colored dot identifiers                                                                                                          |
| **Notification system**              | Bell icon with unread badge, dropdown with filter tabs (All/Input/Errors/Warnings/Success/Done), toast stack with auto-dismiss, input request modals                                |
| **Image attachments in AI chat**     | Paste, drag-drop, or file picker. Max 5 attachments. Base64 via IPC                                                                                                                 |
| **Custom agent definitions**         | User-defined agents (name + CLI command) in Settings, persisted in electron-store                                                                                                   |
| **NewSpaceModal with live terminal** | 2-step wizard with embedded xterm for directory navigation before workspace creation                                                                                                |
| **Mounted spaces pattern**           | TerminalGrids stay mounted (hidden, not destroyed) when switching tabs to preserve xterm.js state                                                                                   |

---

## What Warp Has That Athena Doesn't

| Feature                           | Description                                                                                             |
| --------------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Block-based terminal model**    | Command+output grouping with structural awareness, not a flat character grid                            |
| **Full rich-text input editor**   | Multi-cursor, multi-select, CRDT-based (collaboration-ready), SumTree-backed, operation-based undo      |
| **Inline AI in terminal stream**  | AI and agent content renders alongside terminal output, not in a separate panel                         |
| **Command palette**               | Unified search across actions, workflows, prompts, notebooks, env vars, files, sessions, launch configs |
| **GPU-native rendering pipeline** | Metal/DirectX/WGSL shaders, 400+ fps, custom WarpUI framework                                           |
| **Shell integration protocol**    | precmd/preexec hooks → DCS escape sequences → JSON payloads for boundary detection                      |
| **Block sharing**                 | Share a command+output block as a link (web viewable)                                                   |
| **Sticky command headers**        | Command pins at top of visible area when output scrolls                                                 |
| **Natural language detection**    | Automatically routes input to AI vs shell based on content                                              |
| **Voice input**                   | Voice-to-text for AI queries (`crates/voice_input/`)                                                    |
| **AI-powered tab completions**    | Fig Completion Specs integration (`command-signatures-v2/`)                                             |
| **Theme creation from images**    | Auto-generates color palettes from uploaded images                                                      |
| **OS theme sync**                 | Automatic light/dark mode sync with separate theme selections per mode                                  |
| **Launch configurations**         | Save and restore window/tab/pane layouts                                                                |
| **Vim mode**                      | Full vim keybinding layer for the input editor                                                          |
| **Notebooks**                     | Runnable, shareable documents (Jupyter-like for the terminal)                                           |
| **Secret detection/masking**      | Identifies and masks sensitive values in terminal output                                                |
| **Command corrections**           | AI-suggested fixes for failed commands                                                                  |
| **Crash recovery**                | Session restoration after crashes                                                                       |
| **Context chips**                 | Visual indicators for AI session state in the terminal                                                  |
| **3 separate font settings**      | Independent monospace, AI, and UI font configuration                                                    |

---

## Side-by-Side Summary Table

| Dimension              | Warp                                                         | Athena's Core                                                                           |
| ---------------------- | ------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| **Stack**              | Rust + GPU (Metal/DX/WGSL) + custom WarpUI                   | Electron + React 19 + Tailwind + xterm.js                                               |
| **Performance**        | 400+ fps, ~1.9ms redraw                                      | ~60fps, DOM-bound                                                                       |
| **Terminal model**     | BlockList (typed blocks, SumTree)                            | Flat grid (xterm.js buffer)                                                             |
| **Input**              | Rich-text CRDT editor, multi-cursor, NL detection            | Plain text input, manual AI/shell switching                                             |
| **AI location**        | Inline in terminal stream                                    | Separate right panel                                                                    |
| **AI ↔ terminal link** | Visually interleaved (blocks)                                | Spatially separated (panels)                                                            |
| **Shell integration**  | Protocol contract (hooks + escape sequences)                 | None                                                                                    |
| **Command palette**    | Yes (prefix-filtered, multi-category)                        | No                                                                                      |
| **Themes**             | 21+, preview/commit, OS sync, image creator                  | 25, live apply, no OS sync, manual only                                                 |
| **Fonts**              | 3 contexts (terminal/AI/UI)                                  | 1 configurable font                                                                     |
| **Layout**             | Tabs + split panes + launch configs                          | Workspaces + auto-grid panes                                                            |
| **Persistence**        | SQLite + TOML configs + crash recovery                       | electron-store + localStorage                                                           |
| **Unique strengths**   | Blocks, inline AI, GPU perf, command palette, shell protocol | Kanban, swarm UI, browser, editor, file explorer, notifications, workspaces, image chat |
| **Philosophy**         | Terminal as Application                                      | Terminal as Component                                                                   |
