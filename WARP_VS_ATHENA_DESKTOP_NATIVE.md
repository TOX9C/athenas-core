# Warp (Rust-native) vs Athena's Core (Electron) — Desktop/Native Architecture Comparison

## Language & Runtime

| Dimension        | Warp                                   | Athena                                      |
| ---------------- | -------------------------------------- | ------------------------------------------- |
| Primary language | Rust (edition 2024)                    | TypeScript                                  |
| Runtime          | No runtime VM — compiled native binary | Chromium V8 + Node.js (Electron 32)         |
| Memory model     | Ownership/borrowing, zero GC           | GC (V8 heap for renderer, Node GC for main) |
| Binary size      | Single native binary per platform      | Bundled Chromium (~150MB+) + app code       |
| Startup overhead | Near-instant (no runtime init)         | Slower (Chromium bootstrap, V8 snapshots)   |

## GPU Rendering Pipeline

| Dimension          | Warp                                                                                                | Athena                                                             |
| ------------------ | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Rendering approach | Custom wgpu renderer — direct GPU pipeline (Metal/Vulkan/DX12/GLES)                                 | Chromium's Skia/Blink renderer (internal to Electron)              |
| Backend selection  | Metal (macOS), Vulkan (Linux/Windows), DX12 (Windows), GLES (fallback), WebGL (WASM)                | Whatever Chromium picks (Metal on macOS, Vulkan/DX11 on Win/Linux) |
| Render pipelines   | 3 explicit pipelines: `rect`, `glyph`, `image` — authored in WGSL shaders                           | Implicit — DOM → layout → paint → compositing (browser pipeline)   |
| Present mode       | `AutoNoVsync` — deliberately non-vsync for input latency                                            | Vsync via Chromium compositor                                      |
| Glyph rendering    | Custom glyph atlas — CPU rasterize (font-kit/macOS, cosmic-text/Linux) → upload to GPU texture      | Browser text rendering (Skia + Core Text/DirectWrite/FreeType)     |
| Frame control      | Explicit: `Frame::new()` → `draw()` → `queue.submit()` → `present()`                                | No frame-level control — browser paints on rAF                     |
| Adapter selection  | Custom multi-stage sort with known-bug workarounds (Nvidia Wayland, Intel UHD, Lavapipe, Parallels) | Handled by Chromium internally, no app control                     |
| Alpha blending     | `PostMultiplied` preferred (Wayland transparency), explicit blend state                             | CSS opacity/compositing                                            |
| Frame capture      | GPU staging buffer copy → BGRA↔RGBA conversion → callback                                           | `webContents.capturePage()` or DevTools protocol                   |
| Surface config     | App-managed swapchain with reconfigure-on-loss                                                      | Chromium-managed                                                   |

### Warp's Rendering Flow (PTY bytes → pixels)

```
Shell stdout
  → PTY leader fd (nonblocking, O_NONBLOCK)
  → mio event loop (fd readable)
  → VTE parser (vte crate, Warp's fork)
  → Terminal grid model (warp_terminal)
  → Scene graph (warpui_core)
  → Frame::new() — walk scene, rasterize glyphs on demand, build draw calls
  → 3x wgpu render passes (rect pipeline, glyph pipeline, image pipeline)
  → queue.submit(encoder.finish())
  → surface_texture.present()
```

The `Renderer` is decoupled from the terminal — it receives a `&Scene` and glyph closures, has no knowledge of terminals. This is how Warp renders both terminal content and its IDE-like UI (blocks, prompts, input fields) through the same GPU pipeline.

### Warp's GPU Adapter Selection

`Resources::new()` → `select_adapter()` sorts adapters through a multi-stage stable sort:

1. **Backend priority** — macOS: Vulkan > Metal > DX12 > GL; Windows: DX12 > Vulkan > GL
2. **Feature support** — deprioritizes adapters with known bugs
3. **Power preference** — LowPower favors integrated; HighPerformance favors discrete
4. **Stability** — sorts into `Supported` / `SupportedWithIssues` / `Unsupported` tiers

Specific stability rules:

- Nvidia < 545 on Wayland → `Unsupported`
- Nvidia >= 572 on Windows (non-DX12) → `SupportedWithIssues`
- Intel UHD 620 Vulkan on Windows → `SupportedWithIssues`
- Lavapipe (llvmpipe) < 24.0.2 → `Unsupported`
- Parallels GL-to-Metal on Windows → `SupportedWithIssues`

Windows DX12 specifics: uses `Dx12SwapchainKind::DxgiFromVisual` (DirectComposition) by default, overridable with `WARP_USE_DIRECT_COMPOSITION=0`.

### Warp's wgpu Instance

A `LazyLock<Mutex<Option<Arc<wgpu::Instance>>>>` singleton initialized once via `init_wgpu_instance()`. On non-WASM, initialization happens in a **separate thread** to parallelize with other app startup. On Linux with X11, `WAYLAND_DISPLAY` is temporarily cleared to prevent wgpu from creating a Wayland instance that would crash with an X11 window handle.

Workspace-level wgpu dependency: version `29.0.1` with features: `dx12`, `gles`, `metal`, `parking_lot`, `std`, `vulkan`, `wgsl`.

## Window Management

| Dimension               | Warp                                                                            | Athena                                                      |
| ----------------------- | ------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Windowing library       | winit (Rust cross-platform) — `app`, `delegate`, `event_loop`, `window` modules | Electron's BrowserWindow (Chromium-level)                   |
| Platform APIs (macOS)   | Direct `cocoa = "=0.26.0"`, `objc`, `core-graphics`, `core-text` bindings       | Chromium's internal Cocoa integration (no direct access)    |
| Platform APIs (Linux)   | `x11rb` (X11), `zbus` (D-Bus), `ashpd` (XDG portals), `fontconfig`              | Chromium's Ozone/X11/Wayland (no direct access)             |
| Platform APIs (Windows) | `windows` crate (Win32: DirectWrite, DWM, Shell, COM, Memory)                   | Chromium's Windows integration                              |
| Custom titlebar         | Full control via winit + custom GPU-rendered chrome                             | `frame: false` + HTML/CSS titlebar                          |
| Multi-window            | Via winit event loop                                                            | Single BrowserWindow + WebContentsView for embedded browser |
| Window state            | Managed in Rust app state                                                       | `isMaximized()` via IPC roundtrip                           |

## Process/PTY Management

| Dimension              | Warp                                                                                                                            | Athena                                                   |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------- |
| PTY creation           | `nix::pty::openpty` directly — adapted from Alacritty                                                                           | `node-pty` (npm native addon wrapping fork/exec)         |
| FD management          | **TerminalServer subprocess** — spawns PTY in a dedicated child to avoid FD leaks from main process; falls back to direct spawn | All PTYs live in Electron main process — no FD isolation |
| Pre-exec hooks         | Full `pre_exec` block: signal reset, sigprocmask, dup2, setsid, TIOCSCTTY, close FDs ≥3, OOM score rebias                       | No control — `node-pty` handles internally               |
| Process isolation      | Linux OOM score +500 for sandbox children (killed before Warp on OOM)                                                           | No OOM tuning                                            |
| Crash reporting        | Cocoa Sentry **uninitialized before fork**, reinitialized after — prevents Mach exception handler leaks                         | No fork-safety concerns (node-pty handles internally)    |
| IUTF8 flag             | Explicitly set on leader fd (Linux/macOS)                                                                                       | Not set (node-pty doesn't expose this)                   |
| Docker sandbox         | `spawn_docker_sandbox()` → `sbx run` with per-sandbox init scripts                                                              | No sandbox support                                       |
| Kill semantics         | Explicitly drops fd first (macOS kernel bug workaround — child can hang in 'E' state) then kills process                        | `pty.kill()` — `node-pty` default kill                   |
| Event loop             | **mio** — PTY fd registered as nonblocking, SIGCHLD via `signal_hook`, `EventedReadWrite` trait                                 | Node.js EventEmitter — `pty.onData()`, `pty.onExit()`    |
| Resize                 | Direct `libc::ioctl(fd, TIOCSWINSZ, &winsize)` with pixel dimensions                                                            | `node-pty.resize(cols, rows)` — no pixel dimensions      |
| VTE parsing            | **vte** crate (Warp's fork at rev `4b399c8`) — in-process, Rust                                                                 | **xterm.js** — in-process, JS/WASM                       |
| Terminal model         | Custom grid model (`warp_terminal`) with block-based output grouping                                                            | xterm.js internal buffer model                           |
| History/scrollback     | Custom `rich_history` in SQLite, command-level grouping                                                                         | 100KB chunk-based ring buffer per PTY session in memory  |
| PTY handle abstraction | `PtyHandle` trait with `DirectPtyHandle` and `ServerOwnedPtyHandle` impls                                                       | Single `node-pty` IPty interface                         |

### Warp's PTY Spawn Flow

1. `PtySpawner::spawn_pty()` — first attempts `spawn_pty_via_server()` (dedicated subprocess to avoid FD leaks). If that fails, falls back to `spawn_pty_directly()`.
2. `PtySpawner::new()` — called "extremely early in application startup" to minimize resources that could leak into forked subprocesses. Creates a `TerminalServer` child process.
3. `spawn()` → `build_host_shell_command()` — constructs `Command` with shell, env vars, home dir.
4. `spawn_command_in_pty()` — shared PTY setup:
   - `make_pty()` → `nix::pty::openpty()` with window size, sets `FD_CLOEXEC` on both fds
   - Sets `IUTF8` input flag on leader fd
   - `pre_exec` hook (runs after fork, must be async-signal-safe):
     - Resets all signal handlers to `SIG_DFL` (signals 1-31, skipping KILL/STOP)
     - Unmasks all signals via `sigprocmask`
     - `dup2(follower, STDIN/STDOUT/STDERR)`
     - `setsid()` — new process group
     - `ioctl(follower, TIOCSCTTY)` — sets controlling terminal
     - Closes all FDs >= 3
     - On Linux: OOM score rebias to +500

### Environment Variables

| Warp                                                   | Athena                                        |
| ------------------------------------------------------ | --------------------------------------------- |
| `TERM=xterm-256color`                                  | `CI=1` (Athena mode)                          |
| `TERM_PROGRAM=WarpTerminal`                            | `TERM=dumb` (Athena mode)                     |
| `COLORTERM=truecolor`                                  | `FORCE_COLOR=0` (Athena mode)                 |
| `WARP_CLIENT_VERSION`                                  | `NO_COLOR=1` (Athena mode)                    |
| `WARP_IS_LOCAL_SHELL_SESSION`                          | `ATHENA_MCP_TOKEN`                            |
| `WARP_HONOR_PS1`                                       | `ATHENA_MCP_PORT` (4545)                      |
| `WARP_USE_SSH_WRAPPER`                                 | `ATHENA_COMMS_TOKEN`                          |
| `WARP_PATH_APPEND`                                     | `ATHENA_COMMS_PORT` (4546)                    |
| Sentinel `HISTFILESIZE=57265949261` (detect first run) | `ATHENA_PANE_ID`, `ATHENA_SESSION_ID`         |
|                                                        | `CLAUDE_MCP_SERVERS` / `OPENCODE_MCP_SERVERS` |

## Communication / IPC

| Dimension      | Warp                                                                                            | Athena                                                                                                                     |
| -------------- | ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| Internal comms | Rust channels (mpsc), trait objects, direct function calls — all in-process, zero-serialization | Electron IPC (~80 channels) — `ipcMain.handle()`/`ipcRenderer.invoke()` — crosses process boundary, requires serialization |
| IPC overhead   | None — same process, same memory                                                                | Serialization + deserialization per call, async Promise overhead                                                           |
| PTY → renderer | mio fd readable → VTE parse → model update → Scene → wgpu render                                | pty stdout → IPC `pty:data:{id}` → xterm `term.write()`                                                                    |
| Input → PTY    | Keyboard event → winit → app state → write to PTY fd                                            | Keyboard → DOM → xterm `onData` → IPC `pty:send` → node-pty                                                                |
| Agent comms    | Protobuf-based `warp_multi_agent_api` + MCP via `rmcp` crate — type-safe, schema-driven         | JSON-RPC over TCP (ports 4545/4546) — hand-rolled, stringly-typed                                                          |
| Cloud/backend  | GraphQL via `cynic` + `graphql-ws-client` (WebSocket subscriptions)                             | No cloud backend — all local                                                                                               |
| AI comms       | In-process Rust (ai crate) with protobuf + GraphQL cloud backend                                | HTTP API calls to Anthropic/OpenAI SDKs from main process                                                                  |

### Warp's IPC: Zero-Copy In-Process

All communication in Warp happens within a single process. Terminal output flows through:

```
PTY fd → mio Poll → VTE parser → Terminal Model → Scene Graph → wgpu Renderer
```

No serialization, no cross-process messaging, no async bridges. State updates propagate via Rust's type system and ownership model.

### Athena's IPC: Cross-Process Serialization

Every byte of terminal output crosses the Electron IPC boundary:

```
node-pty stdout → ipcMain.send('pty:data:{id}', chunk) → ipcRenderer.on('pty:data:{id}') → xterm term.write(chunk)
```

Each keystroke also crosses:

```
DOM keydown → xterm onData → ipcRenderer.send('pty:write', id, data) → ipcMain.on('pty:write') → node-pty write()
```

## Data Persistence

| Dimension       | Warp                                                             | Athena                                                     |
| --------------- | ---------------------------------------------------------------- | ---------------------------------------------------------- |
| Database        | Diesel ORM + SQLite with schema migrations (`diesel_migrations`) | electron-store — JSON key-value file on disk               |
| Data modeling   | Relational, typed, queryable, migrated                           | Flat key-value, no schema, no migrations                   |
| Session storage | SQLite tables with relational integrity                          | JSON files in `userData/athena-sessions/` + image binaries |
| History         | Command-level rich history in SQLite with metadata               | 100KB ring buffer per PTY session in memory                |

## AI/Agent Architecture

| Dimension            | Warp                                                                                                                                              | Athena                                                             |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Agent protocol       | Protobuf (`warp_multi_agent_api`) — strongly typed, compiled from .proto                                                                          | JSON-RPC over TCP — hand-rolled, dynamically typed                 |
| MCP                  | `rmcp` crate — native Rust MCP implementation                                                                                                     | Custom `mcpServer.ts` — hand-rolled JSON-RPC TCP server            |
| Computer use         | Dedicated `computer_use` crate — platform-specific screen capture + input simulation (AppKit/CG on macOS, X11/XDG on Linux, Win32 GDI on Windows) | No computer-use capability                                         |
| Input classification | ML-based: fastText or ONNX (Candle or ORT backends) — classifies natural language vs shell command                                                | No input classification                                            |
| Agent spawning       | Via `cli_agent_sessions/` module + terminal manager + protobuf protocol                                                                           | `ptyManager.spawnAgent()` + env var injection + JSON-RPC handshake |
| Agent status         | Protobuf status messages + cloud GraphQL                                                                                                          | JSON-RPC status + 90s stall detection polling                      |
| LLM integration      | In-process Rust (ai crate)                                                                                                                        | Out-of-process SDK calls (Anthropic/OpenAI npm packages)           |

## Build & Packaging

| Dimension          | Warp                                                                      | Athena                                                                         |
| ------------------ | ------------------------------------------------------------------------- | ------------------------------------------------------------------------------ |
| Build system       | Cargo workspace (40+ crates)                                              | electron-vite (3 targets) + electron-builder                                   |
| Cross-compilation  | Rust cross-compile targets + platform-specific cfg gates                  | electron-builder per-platform builds                                           |
| Native deps        | Compiled from source via Cargo                                            | `npm install` + `postinstall` (electron-builder install-app-deps for node-pty) |
| Shader compilation | WGSL shaders compiled by wgpu at runtime                                  | N/A (CSS/HTML rendering)                                                       |
| Bundle format      | Platform-native (.app, .deb, .msi, AppImage)                              | DMG (Mac), NSIS (Win), deb+AppImage (Linux)                                    |
| WASM target        | Full WASM support (wgpu WebGL, `ws_stream_wasm`, `wasm/` platform module) | None — desktop-only                                                            |

## Font Handling

| Dimension        | Warp                                                                                                      | Athena                                             |
| ---------------- | --------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Rasterization    | macOS: `font-kit` (Core Text); Linux/Windows: `cosmic-text` (Warp's fork) + `owned_ttf_parser` + `fontdb` | Browser text rendering (Skia + platform font APIs) |
| Font discovery   | macOS: Core Text; Linux: `fontconfig` (dlopen); Windows: DirectWrite (`dwrote`, Warp's fork)              | CSS `font-family` — browser resolves               |
| Atlas management | Custom texture atlas — glyphs packed into GPU textures, cached per glyph key                              | Browser glyph cache (internal to Skia)             |

## Security & Isolation

| Dimension          | Warp                                                                                     | Athena                                                               |
| ------------------ | ---------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Process isolation  | TerminalServer subprocess for PTY FD isolation; Docker sandbox mode; OOM score rebias    | All in one Electron main process                                     |
| Context isolation  | N/A — no web renderer, no untrusted code execution surface                               | `contextIsolation: true`, `nodeIntegration: false`, `sandbox: false` |
| Sentry integration | Cocoa Sentry uninitialized during fork (prevents Mach handler leak), reinitialized after | No Sentry integration                                                |
| Auth tokens        | Session tokens for agent comms (protobuf-based)                                          | Random UUID tokens for MCP (port 4545) and Agent Comms (port 4546)   |

## SSH Support

| Dimension    | Warp                                                                                                       | Athena         |
| ------------ | ---------------------------------------------------------------------------------------------------------- | -------------- |
| SSH sessions | Full SSH module — detection, warpification (install Warp on remote), tmux install, root access, 8s timeout | No SSH support |

## Developer Experience

| Dimension     | Warp                                                      | Athena                                                 |
| ------------- | --------------------------------------------------------- | ------------------------------------------------------ |
| Debugging     | Rust debug logging, telemetry, frame capture callback     | Chrome DevTools (full inspector, network, profiler)    |
| Hot reload    | Cargo rebuild (slow)                                      | electron-vite HMR (instant)                            |
| Accessibility | Must implement manually (winit has limited a11y)          | Browser a11y tree (screen reader support via Chromium) |
| Layout system | Custom — manual positioning via Scene graph primitives    | CSS Flexbox/Grid — full web layout engine              |
| Styling       | Programmatic — Rust structs define colors, sizes, spacing | CSS/Tailwind — declarative styling                     |

## Warp's Crate Architecture (40+ crates)

```
warp/
├── app/                          — Main application (terminal, PTY, window)
│   └── src/terminal/
│       ├── local_tty/            — PTY spawning (unix.rs, spawner.rs)
│       ├── remote_tty/           — SSH sessions
│       ├── model/                — Terminal grid data model
│       ├── view/                 — Terminal view rendering
│       ├── input/                — Keyboard handling
│       ├── event_listener/       — mio-based event loop
│       ├── grid_renderer/        — Grid-to-Scene conversion
│       ├── cli_agent_sessions/   — AI agent sessions
│       └── ssh/                  — SSH detection & warpification
├── crates/
│   ├── warpui/                   — UI framework (wgpu renderer + Scene)
│   │   └── src/rendering/
│   │       ├── wgpu/             — GPU renderer (rect/glyph/image pipelines)
│   │       ├── atlas/            — Glyph texture atlas
│   │       └── glyph_cache.rs    — Glyph caching/rasterization
│   ├── ai/                       — AI/LLM integration + MCP + multi-agent
│   ├── persistence/              — Diesel ORM + SQLite
│   ├── computer_use/             — Screen capture + input simulation
│   ├── input_classifier/         — ML-based natural language vs command detection
│   ├── command/                  — Cross-platform process commands
│   ├── vim/                      — Vim emulation mode
│   ├── sum_tree/                 — Custom B-tree with prefix sums (terminal grid)
│   ├── warp_graphql/             — GraphQL cloud client
│   ├── warp_multi_agent_api/     — Protobuf multi-agent protocol
│   ├── http_server/              — Axum-based HTTP server
│   ├── warp_js/                  — JS runtime (rquickjs) for completions
│   ├── repo_metadata/            — Repository indexing + file watching
│   └── websocket/                — WebSocket client (native + WASM)
├── Cargo.toml                    — Workspace root
└── bin/
    └── mcp-proxy.js              — (Athena only — stdin/stdout↔TCP bridge)
```

## Summary

Warp trades development speed and ecosystem convenience for **performance, memory safety, and fine-grained control** — they own the entire rendering pipeline from PTY bytes to GPU pixels, can work around driver bugs and OS quirks directly, and avoid IPC serialization overhead entirely. The cost is a massive Rust codebase (40+ crates), platform-specific code for every OS, and no access to the web ecosystem.

Athena's Core trades performance and control for **developer velocity and ecosystem richness** — React/zustand for UI, npm for dependencies, Chrome DevTools for debugging, web standards for layout/styling. The cost is Chromium's memory footprint, IPC serialization on every PTY byte, and no access to low-level OS/GPU APIs without a native addon bridge.
