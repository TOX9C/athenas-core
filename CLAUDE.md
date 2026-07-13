# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

- **Build frontend dist:** `bash frontend/build-dist.sh --debug` (debug) or `bash frontend/build-dist.sh` (release)
- **Run app (debug):** `cargo run --manifest-path src-tauri/Cargo.toml`
- **Run app via Tauri CLI:** `cargo tauri dev` (runs build-dist.sh automatically via beforeDevCommand)
- **Build app (release):** `cargo tauri build`
- **Unit tests:** `cargo test --workspace`
- **E2E tests:** `npm run test:e2e` (see E2E Testing section below)

## High-Level Architecture

"Athena's Core" is a Tauri 2 desktop application with a Dioxus 0.7 WASM frontend. The Rust backend handles all system-level operations (PTY, filesystem, LLM orchestration, MCP, plugins) and the Dioxus/WASM frontend renders the UI in a WKWebView.

### Key Modules

1. **Tauri Backend (`src-tauri/`)**
   - **`src/main.rs`**: App entry point. Registers all Tauri commands, plugins (shell, dialog, log, webdriver-automation in debug), wires AppState.
   - **`src/commands/mod.rs`**: All `#[tauri::command]` handlers organized by domain (window, fs, store, session, pty, athena/LLM, output, notifications, plans, agents, search, MCP, swarm, shell, tools, browser, plugins, security).
   - **`src/state.rs`**: Shared `AppState` with `Arc<Mutex<T>>` and bare `Arc<T>` for internally-synchronized services. Wires event emitters after `set_app_handle`.

2. **Backend Crates (`crates/`)**
   - `athena-core`: Orchestrator, MCP server, swarm coordinator, tool executor, notifications, plan manager, agent comms, shell integration, search
   - `athena-terminal`: PTY session manager with data/ready/exit callbacks
   - `athena-fs`: Filesystem operations
   - `athena-store`: KeyValueStore and SessionStore (SQLite-backed)
   - `athena-browser`: Browser manager for embedded views
   - `athena-plugins`: Plugin manager with host/session lifecycle

3. **Dioxus Frontend (`frontend/`)**
   - **`src/lib.rs`**: Root `App` component with global keybindings, titlebar, sidebar, content panels (Terminal, Editor, Kanban, Swarm, Chat), right sidebar, modals.
   - **`src/tauri_bridge.rs`**: WASM-side bridge for Tauri IPC (`invoke()` calls)
   - **`src/stores/`**: Dioxus signal-based stores (ui, workspace, athena, notification, editor, terminal, layout, session, swarm, task, command, agent_output, agent_status, panel_manager)
   - **`src/components/`**: Feature-organized components mirroring the original React structure
   - **`src/themes/`**: Theme system with CSS variable application
   - **`build-dist.sh`**: Builds Dioxus WASM, copies to `dist/`, replaces index.html with custom version (WASM fetch fix + console capture), auto-detects `wasm/` vs `assets/` directory layout

### Architecture Guidelines

- **IPC Communication**: Frontend MUST use `tauri_bridge.rs` for all backend calls. Never import Rust stdlib or Tauri APIs directly in Dioxus components.
- **Parameter naming**: Tauri `#[tauri::command]` maps JSON keys to Rust param names. Do NOT prefix with `_` — the JSON key will include the underscore and mismatch.
- **WASM loading**: The custom `index.html` uses `__FRONTEND_ENTRY__` placeholder, replaced by `build-dist.sh` with `./wasm/athena-frontend.js` (debug) or `./assets/athena-frontend.js` (release). Never hardcode the path.
- **Debug builds**: Auto-open DevTools via `#[cfg(debug_assertions)]`. The `tauri-plugin-webdriver-automation` is only compiled and registered in debug builds.

## E2E Testing

End-to-end tests use **WebdriverIO** with **tauri-webdriver** (github.com/danielraffel/tauri-webdriver) for automated UI testing against the WKWebView on macOS.

### Prerequisites

1. **Build the frontend dist (release mode)**: `bash frontend/build-dist.sh`
2. **Build the debug binary**: `cargo build --manifest-path src-tauri/Cargo.toml`
3. **Install tauri-wd CLI**: `cargo install tauri-webdriver-automation --vers 0.1.3`
   - Note: `tauri-webdriver-automation` (CLI server) and `tauri-plugin-webdriver-automation` (Tauri plugin) are two separate crates that must share the same version. Both are pinned to `0.1.3`.
4. **Install e2e deps**: `cd e2e-tests && npm install`

### Running E2E Tests

```bash
# Terminal 1: Start the WebDriver server
tauri-wd --port 4444

# Terminal 2: Run the tests
npm run test:e2e
```

Or in a single command (background the server):

```bash
(tauri-wd --port 4444 &) && sleep 2 && npm run test:e2e
```

### How It Works

- `tauri-wd` listens on port 4444 and speaks the W3C WebDriver protocol
- WebdriverIO sends `POST /session` with `tauri:options.binary` pointing at the debug app
- `tauri-wd` launches the app with `TAURI_WEBVIEW_AUTOMATION=true`
- The app's `tauri-plugin-webdriver-automation` plugin starts an HTTP server on a random port
- `tauri-wd` discovers the plugin port from the app's stdout (`[webdriver] listening on port N`)
- All WebDriver commands (find element, click, screenshot, execute script) are proxied through the plugin

### Important Notes

- **Debug binary only**: The webdriver plugin is gated by `#[cfg(debug_assertions)]` and won't exist in release builds
- **Frontend must be built in RELEASE mode**: Run `bash frontend/build-dist.sh` (no `--debug` flag). Debug builds include Dioxus devtools which try to open a WebSocket for hot-reload — WKWebView rejects this with `SecurityError`, causing the WASM to panic at `dioxus-web/src/devtools.rs`. Release builds disable devtools and mount successfully.
- **Screenshots**: SVG-based rendering — cannot capture native title bars or CSS `backdrop-filter` effects
- **macOS only**: tauri-webdriver exists specifically because Apple doesn't provide a WKWebView WebDriver
- **tauri-wd element reference bug**: WDIO's `isElementDisplayed` passes element references as JSON objects, but `Node.contains()` expects a DOM Node. This causes `Argument 1 ('other') to Node.contains must be an instance of Node` errors when using WDIO's `.click()`. Use `browser.execute()` to dispatch click events directly instead.
- **Known WASM runtime issue**: Dioxus 0.7 event handlers can cause `RuntimeError: Unreachable code should not be executed` panics in WKWebView after clicks. The app renders correctly but interactive features may crash the WASM runtime.
- **CI runner cancellation**: GitHub Actions externally cancels the `cargo test` step after all 324 tests pass (conclusion: `cancelled`, not `failure`). This is an account/runner-level kill, not a code or workflow issue — verified across 6+ runs with no newer pushes, no OOM, no test failures. Local `cargo test --workspace` is the reliable verification path until the runner issue is resolved.
