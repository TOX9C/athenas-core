# Athena's Core

<img src="src-tauri/icons/128x128.png?v=2025" alt="Athena's Core" width="128" height="128" />

A next-generation desktop IDE for AI-assisted software development. Athena's Core unifies a multi-pane terminal, AI chat assistant, task management, and multi-agent orchestration into a single cross-platform native application.

## Preview

<p align="center">
  <a href="docs/athenas-core-ad.mp4">
    <img src="docs/athenas-core-ad.gif" alt="Athena's Core — Product Showcase" width="900" />
  </a>
</p>

<p align="center">
  <i>75-second product showcase: workspace, AI chat, swarm, kanban, editor, and command palette.<br><a href="docs/athenas-core-ad.mp4">Download the full 1080p MP4 (5 MB)</a></i>
</p>

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Getting Started](#getting-started)
- [Architecture](#architecture)
- [Screenshots](#screenshots)
- [License](#license)

---

## Overview

Athena's Core is a powerful developer workspace that combines multiple tools into a single unified desktop experience. It features a multi-pane terminal grid with full ANSI support, an integrated AI chat assistant that can reason across your workspace, a Kanban task board for project management, a multi-agent swarm system for autonomous task execution, an embedded browser panel, and a plugin system for extensibility.

Built with Rust and Tauri 2, Athena's Core delivers native performance with a modern web-tech frontend, offering a compact ~15MB binary footprint and significantly lower memory usage compared to traditional Electron-based applications.

---

## Features

### Integrated AI Chat Assistant

Athena is your workspace-aware AI assistant, powered by multiple LLM providers. It understands your entire workspace, sees active agents, monitors execution plans, and can interact with your codebase.

- **Multi-provider support**: Connect to Anthropic Claude, OpenAI, NVIDIA NIM, or local models via LM Studio
- **Workspace context**: Every conversation includes a live snapshot of your workspace, agents, and active plans
- **Image support**: Attach screenshots or diagrams directly into chat messages
- **Tool calling**: Athena can spawn agents, run commands, search your codebase, and manage tasks automatically
- **Session persistence**: Chat history is saved and restored across app restarts

### Multi-Pane Terminal Grid

A flexible terminal workspace with full ANSI/VT100 emulation, powered by a custom Rust terminal emulator.

- **Multiple pane layouts**: Automatic grid layouts from 1x1 up to 4x4 as you add terminals agents
- **Shell integration**: Smart shell tracking for zsh, bash, and fish via OSC 633 sequences
- **Command tracking**: Automatically detects prompts, commands, execution, and exit codes
- **ANSI support**: Full 256-color and true-color (24-bit RGB) support
- **xterm.js frontend**: Familiar terminal experience with proper key handling and autoresize
- **Output capture**: Every pane's output is buffered and searchable

### Kanban Task Board

Keep track of your development tasks with a built-in Kanban board.

- **Four columns**: To Do, In Progress, In Review, and Complete
- **Task management**: Create, update, and delete tasks with descriptions and agent assignments
- **Per-workspace**: Each workspace has its own independent task board
- **Agent integration**: Tasks can be assigned to specific AI agents

### Swarm Multi-Agent Coordination

Launch and coordinate teams of AI agents to work on complex tasks autonomously.

- **Agent roles**: Coordinator, Builder, Scout, and Reviewer roles with distinct responsibilities
- **Mailbox system**: Agents communicate with each other through a shared message bus
- **Real-time monitoring**: Watch agent status, activity, and messages in real time
- **State persistence**: Swarm configuration and messages persist on disk
- **Auto-detection**: Automatically detects stalled agents after inactivity

### Plugin System

Extend Athena's Core with custom plugins that integrate seamlessly with the workspace.

- **Manifest-based**: Plugins defined by simple JSON manifests
- **Capability scoping**: Plugins request only the capabilities they need
- **Session management**: Per-agent plugin sessions with lifecycle tracking
- **Event bus**: Subscribe to and emit workspace events
- **Security validation**: Automatic validation of plugin manifests against security rules

### Browser Panel

Browse the web without leaving your workspace.

- **Embedded webview**: Full web browsing capability within a resizable sidebar panel
- **Navigation controls**: Back, forward, reload with full history tracking
- **External links**: Open links in your native browser with one click
- **Safe URL handling**: Dangerous schemes and empty URLs are automatically rejected

### Notification System

Stay informed of everything happening in your workspace.

- **Seven notification types**: Info, Warning, Error, Success, Needs Input, Task Complete, and Task Error
- **Actionable**: Many notifications include action buttons for quick responses
- **Persistent history**: Up to 500 notifications stored in memory
- **Unread tracking**: Badge counts and filtered views to focus on what matters
- **Real-time**: Notifications appear instantly as events occur

### Command Palette

Quick access to all app commands with fuzzy search.

- **Keyboard shortcut**: `Cmd+K` or `Cmd+P` for instant access
- **Fuzzy matching**: Find commands even with partial or imprecise input
- **Recent commands**: Your most-used commands surface automatically
- **Keyboard navigation**: Arrow keys and Enter for fast selection

### Theme System

A rich visual experience with extensive customization.

- **16 built-in themes**: Curated dark and light themes inspired by modern IDEs and cultural aesthetics
- **System auto-detect**: Automatically selects a theme matching your OS preference
- **Live font preview**: Change fonts with instant visual feedback
- **Font size control**: Fine-tune terminal and editor font sizes
- **CSS custom properties**: Full control over the color system

### Workspaces

Organize your work into isolated workspaces.

- **Multiple spaces**: Create and switch between different project environments
- **Workspace tabs**: Quick visual switching between active spaces
- **Per-workspace state**: Each workspace maintains its own agent panes, tasks, and settings
- **Project directory**: Associate each workspace with a specific folder on disk

---

## Getting Started

### Prerequisites

- **Rust 1.82+** - Install via [rustup](https://rustup.rs/)
- **Node.js 18+** - For the Dioxus frontend build tools
- **Tauri CLI** - `cargo install tauri-cli`
- **Dioxus CLI** - `cargo install --git https://github.com/DioxusLabs/dioxus dioxus-cli`
- **Platform dependencies**:
  - **macOS**: Xcode Command Line Tools
  - **Linux**: `webkit2gtk-4.1`, `build-essential`, `curl`, `wget`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
  - **Windows**: WebView2 (included in Windows 10/11), Visual Studio Build Tools

### Quick Start

```bash
# Clone the repository
git clone <repo-url>
cd athenas-core

# Build the frontend assets (release mode)
bash frontend/build-dist.sh

# Run the application in development mode
cargo run --manifest-path src-tauri/Cargo.toml

# Or use the Tauri CLI for hot-reload
cargo tauri dev
```

### Production Build

```bash
# Full release build
bash frontend/build-dist.sh
cargo tauri build
```

### Running Tests

```bash
# All workspace tests
cargo test --workspace

# With output
cargo test --workspace -- --nocapture
```

---

## Architecture

```
+-------------------------------------------------------+
|                    Tauri App Shell                       |
+--------------------------+-----------------------------|
|  Frontend (Dioxus)       |   Backend (Tauri/Rust)       |
|                          |                              |
|  +------------------+    |   +--------------------+     |
|  |  App Component   |    |   |  Tauri Commands    |     |
|  |  - Workspace     |    |   |  - Window/FS       |     |
|  |  - Kanban        |    |   |  - Store/Session   |     |
|  |  - Swarm         |    |   |  - PTY/Athena     |     |
|  |  - xterm.js      |    |   |  - Search/Notify   |     |
|  +------------------+    |   +---------+------------     |
|           ^              |             |                  |
|           +--------------+             |                  |
|                          |   +--------v------------     |
|                          |   |  Workspace Crates  |     |
|                          |   |  athena-core        |     |
|                          |   |  athena-terminal    |     |
|                          |   |  athena-store       |     |
|                          |   |  athena-fs          |     |
|                          |   |  athena-browser     |     |
|                          |   |  athena-plugins     |     |
|                          |   +---------------------     |
+--------------------------+-----------------------------|
```

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `src-tauri` | Tauri binary, 95 IPC commands, app shell |
| `athena-frontend` | Dioxus web frontend, 85+ components, 15 stores |
| `athena-core` | LLM orchestrator, MCP server, agent comms, search |
| `athena-terminal` | PTY session manager with ANSI/VT100 emulator |
| `athena-store` | Persistent key-value and session store |
| `athena-fs` | Filesystem utilities with path traversal protection |
| `athena-browser` | Browser manager for embedded webviews |
| `athena-plugins` | Plugin system with manifest validation |

---

## Screenshots

> Screenshots of the application in action will be added here in the future.
>
> The application features:
> - A macOS-style overlay titlebar with workspace tabs
> - Collapsible left sidebar with Spaces, Files, Agents, and Plugins
> - Center panel with terminal grid or Kanban/Swarm views
> - Resizable right sidebar with Browser, AI Chat, Editor, and Skills panels
> - Status bar showing workspace info and active theme
> - 16 built-in color themes with CSS custom property system
>
> Place your screenshots in `docs/screenshots/` and reference them above.

---

### Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+K` / `Cmd+P` | Show command palette |
| `Cmd+J` / `Cmd+\` | Toggle right sidebar |
| `Cmd+B` | Toggle left sidebar |
| `Cmd+T` | New workspace |
| `Cmd+Shift+A` | Add shell pane |
| `Cmd+Shift+S` | Launch swarm |
| `Cmd+1` / `Cmd+2` / `Cmd+3` / `Cmd+4` | Switch panels |
| `Cmd+,` | Open settings |
| `Cmd+W` | Close active pane |
| `Escape` | Close modals / dismiss popups |

---

## License

Proprietary - All rights reserved.
