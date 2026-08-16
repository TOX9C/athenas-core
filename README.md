# Athena's Core

<img src="src-tauri/icons/128x128.png?v=2025" alt="Athena's Core" width="128" height="128" />

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Platform](https://img.shields.io/badge/Platform-macOS%20Apple%20Silicon-blue)](https://github.com/TOX9C/athenas-core/releases)
[![Rust](https://img.shields.io/badge/Rust-Tauri%202%20%2B%20Dioxus-orange)](https://www.rust-lang.org/)
[![Stars](https://img.shields.io/github/stars/TOX9C/athenas-core?style=social)](https://github.com/TOX9C/athenas-core)
[![Donate](https://img.shields.io/badge/Donate-NowPayments-purple)](https://nowpayments.io/donation/tox9c)

A next-generation desktop IDE for AI-assisted software development. Athena's Core unifies a multi-pane terminal, AI chat assistant, task management, and multi-agent orchestration into a single native application. The current release scope is macOS on Apple Silicon (arm64), macOS 13.0 or newer; other platforms are not release artifacts yet.

## Preview

A native desktop workspace for AI-assisted software development — multi-pane terminal, AI chat, task management, and multi-agent orchestration in a single ~15MB Rust + Tauri binary.

---

## Table of Contents

- [Overview](#overview)
- [Features](#features)
- [Getting Started](#getting-started)
- [Architecture](#architecture)
- [Documentation](#documentation)
- [Screenshots](#screenshots)
- [Privacy and Support](#privacy-and-support)
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
- **Provider presets**: Pick a provider in Settings → Athena (OpenAI, Anthropic, NVIDIA NIM, LM Studio, or any custom OpenAI-compatible endpoint); the base URL auto-fills and the available models can be fetched from the provider's `/models` endpoint
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
- **Native child webview**: Sites render in a real Tauri WebView rather than an iframe, with docking and sidebar relocation support
- **Live page state**: Clicked links, redirects, document titles, and load status synchronize back to the toolbar
- **Safe URL handling**: Only validated HTTP(S) URLs are accepted; dangerous schemes, credentials, malformed hosts, and empty URLs are rejected. Private IPv4/localhost targets remain available for local development.

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

### Mobile Companion (LAN PWA)

Athena includes an experimental phone companion that mirrors the desktop over a trusted local network. It is a browser/PWA surface, not a separate iOS or Android binary: the desktop remains the source of truth for workspaces, terminals, files, and Athena chat.

To pair a phone:

1. Build and launch Athena's Core.
2. Open **Settings → Mobile Mirror** and enable it.
3. Scan the QR code or copy the private link shown there.
4. Open the link on a phone connected to the same Wi‑Fi and optionally install it to the home screen.

The link acts like a password and grants access while Mobile Mirror is enabled. Do not share it publicly. The relay currently uses HTTP/WebSocket on the trusted LAN and is not intended for internet exposure or untrusted networks. The companion supports workspace viewing, terminal output/commands, Athena chat, and basic file read/save operations.

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
- **Platform dependencies** (macOS only — Windows and Linux are not release targets yet):
  - **macOS 13.0+ on Apple Silicon**: Xcode Command Line Tools for development; public DMGs are signed/notarized release artifacts.

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

| Crate             | Purpose                                             |
| ----------------- | --------------------------------------------------- |
| `src-tauri`       | Tauri binary, 134 IPC commands, app shell           |
| `athena-frontend` | Dioxus web frontend, 85+ components, 15 stores      |
| `athena-core`     | LLM orchestrator, MCP server, agent comms, search   |
| `athena-terminal` | PTY session manager with ANSI/VT100 emulator        |
| `athena-store`    | Persistent key-value and session store              |
| `athena-fs`       | Filesystem utilities with path traversal protection |
| `athena-browser`  | Browser manager for embedded webviews               |
| `athena-plugins`  | Plugin system with manifest validation              |

---

## Documentation

Full documentation lives in [`docs/`](docs/README.md) — architecture, plugin and MCP guides, contributing, and the release scope, privacy, and support records. See the [docs index](docs/README.md) for the complete map and what belongs in `docs/` versus `.plans/`.

---

## Screenshots

Screenshots will be added with the public release.

---

### Keyboard Shortcuts

| Shortcut                              | Action                        |
| ------------------------------------- | ----------------------------- |
| `Cmd+K` / `Cmd+P`                     | Show command palette          |
| `Cmd+J` / `Cmd+\`                     | Toggle right sidebar          |
| `Cmd+B`                               | Toggle left sidebar           |
| `Cmd+T`                               | New workspace                 |
| `Cmd+Shift+A`                         | Add shell pane                |
| `Cmd+Shift+S`                         | Launch swarm                  |
| `Cmd+1` / `Cmd+2` / `Cmd+3` / `Cmd+4` | Switch panels                 |
| `Cmd+,`                               | Open settings                 |
| `Cmd+W`                               | Close active pane             |
| `Escape`                              | Close modals / dismiss popups |

---

## Privacy and Support

Before public distribution, review the [Privacy Notice](docs/release/PRIVACY_NOTICE.md) for provider, plugin, browser, and Mobile Mirror data flows. For troubleshooting and safe diagnostic collection, use the [Support Runbook](docs/release/SUPPORT_RUNBOOK.md). Never send API keys, relay tokens, or unredacted credentials in an issue.

---

## License

Athena's Core is released under the [MIT License](LICENSE). You're free to use, modify, and distribute it.

---

## Support the Developer

Athena's Core is free and open source. If it saves you time or you just want to support a solo developer, donations are appreciated.

**Crypto:**

| Coin | Network | Address |
|------|---------|---------|
| BTC | Bitcoin | `bc1qn8ehwc7rxlpgvljztr5k6npqf307xq00dqatf8` |
| ETH / USDT / USDC | ERC-20 | `0x4260456e1dbdc880d69d75949726953215a93586` |
| USDT | TRC-20 | `TSBUpAreTjmUscbUbf4L1wkX1fvvJvSRGW` |

**Donate online (card or crypto):** https://nowpayments.io/donation/tox9c

**Other ways to help:**
- ⭐ Star the repo on [GitHub](https://github.com/TOX9C/athenas-core)
- Share it with friends or on social media
- Report bugs and suggest features in [Issues](https://github.com/TOX9C/athenas-core/issues)
- Contribute code via Pull Requests
