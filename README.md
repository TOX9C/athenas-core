# Athena's Core

A cross-platform desktop IDE for AI-assisted development, rebuilt in Rust with Tauri 2 and Dioxus.

## Overview

Athena's Core is a next-generation development environment that combines a terminal, AI chat assistant, task management (Kanban), multi-agent orchestration (Swarm), and a plugin system into a single desktop application.

This workspace contains the **Rust/Tauri migration** — a complete rewrite of the original Electron/Node.js application. The migration delivers:

- **~10x smaller binary** (~15MB vs ~150MB Electron)
- **~50% less memory** at idle
- **Native performance** for PTY, file I/O, and LLM orchestration
- **Full feature parity** with the Electron app
- **Same data directory** — zero-config migration for existing users

## Architecture

```
┌─────────────────────────────────────────────────────┐
│                   Tauri App Shell                    │
├──────────────────────┬──────────────────────────────┤
│  Frontend (Dioxus)   │   Backend (Tauri/Rust)       │
│                      │                              │
│  ┌────────────────┐  │   ┌──────────────────────┐   │
│  │  App Component  │  │   │  src-tauri (binary)  │   │
│  │  + 78 commands  │◄─┼──►│  + 78 Tauri commands │   │
│  │  + 15 stores    │  │   │  + AppState          │   │
│  └────────────────┘  │   └──────────┬───────────┘   │
│                      │              │               │
│  ┌────────────────┐  │   ┌──────────▼───────────┐   │
│  │  xterm.js       │  │   │  Workspace Crates     │   │
│  │  Terminal Pane  │◄─┼──►│  athena-terminal      │   │
│  └────────────────┘  │   │  athena-core          │   │
│                      │   │  athena-store         │   │
│  ┌────────────────┐  │   │  athena-fs            │   │
│  │  Athena Chat    │◄─┼──►│  athena-browser       │   │
│  └────────────────┘  │   │  athena-plugins       │   │
│                      │   └──────────────────────┘   │
│  ┌────────────────┐  │                              │
│  │  Kanban/Swarm   │  │   ┌──────────────────────┐   │
│  │  Plugins/Editor │  │   │  External Services    │   │
│  └────────────────┘  │   │  MCP Server :4545      │   │
│                      │   │  Agent Comms :4546     │   │
│                      │   │  LLM APIs (HTTP)       │   │
└──────────────────────┴───┴──────────────────────┘
```

### Crate Structure

| Crate | Purpose |
|-------|---------|
| `src-tauri` | Tauri binary — app entry point, 78 IPC commands, graceful shutdown |
| `athena-frontend` | Dioxus web frontend — 85+ components, 15 stores, xterm.js integration |
| `athena-core` | LLM orchestrator, MCP server, agent comms, search, notifications, plans, swarm |
| `athena-terminal` | PTY session manager — spawn, write, resize, kill, output capture |
| `athena-store` | Persistent key-value store, session management, image storage |
| `athena-fs` | File system utilities and watchers |
| `athena-browser` | Browser manager for embedded webviews |
| `athena-plugins` | Plugin system — manifest, event bus, session management |

## Prerequisites

- **Rust 1.82+** — install via [rustup](https://rustup.rs/)
- **Node.js 18+** — for Dioxus frontend build (`dx` CLI)
- **Tauri CLI** — `cargo install tauri-cli`
- **Dioxus CLI** — `cargo install --git https://github.com/DioxusLabs/dioxus dioxus-cli`
- **Platform dependencies:**
  - **macOS:** Xcode Command Line Tools
  - **Linux:** `webkit2gtk-4.1`, `build-essential`, `curl`, `wget`, `libssl-dev`, `libgtk-3-dev`, `libayatana-appindicator3-dev`, `librsvg2-dev`
  - **Windows:** WebView2 (included in Windows 10/11), Visual Studio Build Tools

## Quick Start

```bash
# Clone and enter the migration workspace
cd rust-migration

# Check compilation
cargo check --workspace

# Development (two-step build)
./dev.sh
# Or manually:
dx build --package athena-frontend
cp -r target/dx/athena-frontend/debug/web/public/* frontend/dist/
cargo tauri dev

# Production build
cargo tauri build
```

## Development Workflow

### Frontend Development

The Dioxus frontend compiles to WASM and is served as static files to Tauri's WebView:

```bash
# Build frontend only
dx build --package athena-frontend

# Copy to dist (where Tauri expects it)
cp -r target/dx/athena-frontend/debug/web/public/* frontend/dist/
```

### Backend Development

```bash
# Watch-mode compilation
cargo watch -x "check --workspace"

# Run Tauri dev (hot-reload for Rust)
cargo tauri dev
```

### Running Tests

```bash
# All workspace tests
cargo test --workspace

# Single crate
cargo test -p athena-core

# With output
cargo test --workspace -- --nocapture
```

### Code Quality

```bash
# Format
cargo fmt

# Lint
cargo clippy --all-targets --all-features -- -D warnings

# Type check
cargo check --workspace
```

## Testing

The project uses Rust's built-in test framework. Tests are organized by crate:

- **`athena-core`** — orchestrator message formatting, MCP request parsing, agent comms session lifecycle, search result formatting
- **`athena-terminal`** — PTY spawn/write/resize/kill, concurrent sessions, history trimming
- **`athena-store`** — CRUD operations, persistence, orphan cleanup

Run the full suite:

```bash
cargo test --workspace
```

## Migration Status

| Phase | Description | Status |
|-------|-------------|--------|
| 0. Critical Bugs | Compiles, MCP works, agents can connect | ✅ Complete |
| 1. Backend Completeness | All events wired, no stubs, no blocking I/O | ✅ Complete |
| 2. Frontend Integration | Right sidebar, file tree, themes, shortcuts | In Progress |
| 3. Stub Components | Editor syntax highlighting, browser iframe | TODO |
| 4. Testing & Polish | 80%+ tests, security, data migration | TODO |
| 5. Build & Release | CI/CD, code signing, auto-update | TODO |

### What Works

- Terminal with xterm.js (PTY, data streaming, resize, ANSI colors)
- Athena AI chat panel (multi-provider: Anthropic, OpenAI, NVIDIA NIM, LM Studio)
- MCP server on port 4545 (14 tool handlers)
- Agent communications on port 4546 (initialize, notify, status, input request, heartbeat)
- Kanban board with MCP task management
- Swarm multi-agent coordination
- Plugin system with event bus
- Notification system (bell, panel, toast)
- Command palette
- Settings with theme picker
- File tree and workspace tabs
- 78 Tauri commands registered
- 15 Zustand-equivalent stores
- Graceful shutdown (PTY kill, MCP shutdown, agent comms)

### Known Gaps

- Editor panel needs syntax highlighting
- Browser panel needs iframe embedding
- Theme CSS variable switching needs completion
- Additional keyboard shortcuts (Cmd+W, Cmd+P, Cmd+E, Cmd+\)
- File tree auto-refresh on FS events
- Terminal ready/exit event UI updates

## Contributing

See [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) for development guidelines.

## License

Proprietary — All rights reserved.
