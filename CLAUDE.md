# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- **Start Development Server:** `npm run dev` (Starts the electron-vite dev server)
- **Build Application:** `npm run build`
- **Preview Application:** `npm run preview`
- **Post-install native dependencies:** `npm run postinstall` (crucial to run after installing packages to re-compile native deps like `node-pty` for Electron)

## High-Level Architecture

"Athena's Core" is an Electron desktop application built with React 19, TypeScript, and Vite (`electron-vite`). It utilizes `zustand` for state management, `@monaco-editor/react` for the built-in editor, and `@xterm/xterm` + `node-pty` for localized terminal instances.

### Key Modules and Communication

1. **Main Process (`electron/`)**
   - **`main.ts`**: The entry point. Handles window creation and registers IPC handlers. This connects all system-level modules to the renderer process.
   - **`ptyManager.ts`**: Handles underlying terminal pseudo-teletype process orchestration using native `node-pty`. Manages spawning, resizing, and writing, and saves terminal session history.
   - **`athenaOrchestrator.ts`**: Manages the conversational context and communication with LLMs (Claude/OpenAI). Interacts heavily with `ptyManager` when an AI action involves creating/controlling terminals/agents.
   - **`mcpServer.ts` / `swarmCoordinator.ts`**: Implements the Model Context Protocol (MCP) and agent coordination for "Swarm", connecting the app's internal interfaces to the AI agents.
   - **`fileSystem.ts` / `storeUtil.ts` / `browserManager.ts`**: Handlers for respective native operations (reading/writing files, persistent `electron-store` configurations, and managing embedded browser views).

2. **Preload Script (`electron/preload.ts`)**
   - Employs context isolation, bridging functionality from the node backend to the frontend safely. System APIs are exposed under the global `window.athena` object with targeted namespaces such as `athena.window`, `athena.fs`, `athena.pty`, and `athena.store`.

3. **Renderer Process (`src/`)**
   - **`App.tsx`**: Forms the root application layout using `react-resizable-panels`. It initializes platform variables, global key bindings, and fetches user configurations heavily relying on polling state managers for updates.
   - **State Management (`src/store/`)**: State relies strictly on slice-specific `zustand` stores (`workspaceStore`, `athenaStore`, `uiStore`, `editorStore`, etc.).
   - **Feature Components (`src/components/`)**: Code is primarily organized by major feature slices:
     - `Athena/`: Chat UI & integration views.
     - `Terminal/`: Terminal GUI rendering layer utilizing `xterm`.
     - `Workspace/`: Space/tab managers.
     - `Editor/` & `Kanban/`: Secondary active panels depending on user workflow.

### Architecture Guidelines

- **IPC Communication is Strict**: Renderer components MUST NOT import Node.js APIs (e.g., `fs`, `path`) directly. Rely strictly on what is exposed in `window.athena` via `preload.ts`. If new system features are needed, they must be implemented in the `electron/` directory and explicitly bridged.
- **Agent Interactivity**: Terminals launched across workspaces may be linked to specific agents. `ptyManager.ts` captures stdout which `mcpServer.ts`/`athenaOrchestrator.ts` ingest. Changes in Terminal behavior typically require tandem updates on both ends.
