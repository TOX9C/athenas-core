# Athena's Core — Marketing Posts

Ready-to-post drafts. Copy-paste each one. Post Show HN first, then Reddit the same day.

---

## 1. Hacker News — Show HN

**Title:** Show HN: I built an AI-assisted IDE in Rust that's 15MB — no Electron

**Text:**

I'm a solo developer in Iraq, and I've spent the last several months building Athena's Core — a native desktop IDE for AI-assisted software development.

It started because I was tired of Electron. Every AI coding tool I tried was either a VS Code extension ( bloated, limited to one editor) or a full Electron app eating 2GB of RAM. I wanted something that felt like a native terminal multiplexer with an AI brain attached — so I built one.

**What it is:**
- Rust + Tauri 2 backend, Dioxus WASM frontend
- ~15MB binary, a fraction of Electron memory usage
- Multi-pane terminal grid (1×1 up to 4×4) with full ANSI/VT100 emulation and OSC 633 shell integration
- AI chat with workspace context — sees your active agents, plans, and files. Supports Claude, OpenAI, NVIDIA NIM, and local models via LM Studio
- Agent swarm system — launch Coordinator, Builder, Scout, Reviewer agents that communicate via a mailbox bus and work on tasks autonomously
- Kanban task board with agent assignments — drop a task on a card and the agent picks it up
- Embedded browser panel, plugin system, command palette, 16 themes
- Mobile companion via LAN PWA — mirror your desktop to your phone

**Tech choices:**
- Tauri 2 over Electron: native webview, no bundled Chromium
- Dioxus (Rust → WASM) over React: smaller bundle, type-safe, no JS dependency graph
- Custom PTY + ANSI emulator in Rust (athena-terminal crate) instead of depending on node-pty
- SQLite for persistence (athena-store) — no external database

**What's next:**
- Linux and Windows release builds (currently macOS Apple Silicon only)
- More LLM providers
- Plugin marketplace

The app is free and MIT-licensed. If you build from source, it's all there. I'd love feedback on the architecture, the agent swarm model, and whether the Rust + Tauri + Dioxus stack is viable for serious desktop apps.

GitHub: https://github.com/TOX9C/athenas-core
Website: https://tox9c.github.io/athenas-core/

---

## 2. Reddit — r/rust

**Title:** I built an AI-assisted IDE in Rust + Tauri 2 + Dioxus — 15MB binary, no Electron

**Body:**

I've been working on Athena's Core for several months and just released it as open source (MIT).

It's a native desktop IDE built entirely in Rust:

- **Backend:** Tauri 2 with 134 IPC commands
- **Frontend:** Dioxus 0.7 compiled to WASM, 85+ components, 15 stores
- **Terminal:** Custom PTY session manager with full ANSI/VT100 emulation (athena-terminal crate)
- **AI:** LLM orchestrator with multi-provider support (Claude, OpenAI, NIM, local models)
- **Agents:** Multi-agent swarm system with Coordinator/Builder/Scout/Reviewer roles and a mailbox bus
- **Storage:** SQLite via athena-store

The whole thing compiles to a ~15MB binary. No Electron, no bundled Chromium, no Node runtime.

Crate structure:

| Crate | Purpose |
|-------|---------|
| `src-tauri` | Tauri binary, 134 IPC commands |
| `athena-frontend` | Dioxus WASM frontend |
| `athena-core` | LLM orchestrator, MCP server, agent comms |
| `athena-terminal` | PTY + ANSI/VT100 emulator |
| `athena-store` | Persistent KV + session store (SQLite) |
| `athena-fs` | Filesystem with path traversal protection |
| `athena-browser` | Embedded webview manager |
| `athena-plugins` | Plugin system with manifest validation |

Currently macOS Apple Silicon only — working on Linux and Windows.

I'd love feedback from the Rust community on:
1. The Tauri 2 + Dioxus stack for a complex desktop app (is anyone else pushing Dioxus this hard?)
2. The ANSI/VT100 terminal emulator implementation
3. The agent swarm architecture
4. Whether I should ditch Tauri for pure native (or is this the right sweet spot?)

GitHub: https://github.com/TOX9C/athenas-core
Website: https://tox9c.github.io/athenas-core/

---

## 3. Reddit — r/programming

**Title:** A solo dev built a full AI-assisted IDE in Rust that's 15MB — no Electron, no bundled Chromium

**Body:**

The Electron fatigue is real. A solo developer built Athena's Core — an AI-assisted IDE with multi-pane terminal, AI chat, agent swarm, Kanban task board, and plugin system — as a 15MB Rust + Tauri 2 binary.

For comparison:
- VS Code (Electron): ~350MB installed, ~200-400MB RAM
- Cursor (Electron fork): similar
- Athena's Core (Rust + Tauri 2 + Dioxus WASM): ~15MB binary, a fraction of the memory

Key technical choices:
- **Tauri 2** instead of Electron — uses the platform's native WebView, no bundled Chromium
- **Dioxus** (Rust → WASM) instead of React/TypeScript — type-safe, smaller bundle, no JS toolchain
- **Custom ANSI/VT100 terminal emulator** in Rust instead of depending on node-pty
- **SQLite** for persistence instead of an external database
- **Multi-agent swarm** — launch AI agents (Coordinator, Builder, Scout, Reviewer) that communicate via a mailbox bus and execute tasks autonomously
- **Multi-provider AI** — works with Claude, OpenAI, NVIDIA NIM, and local models
- **Plugin system** with capability scoping and security validation

MIT licensed and free to use.

GitHub: https://github.com/TOX9C/athenas-core
Website: https://tox9c.github.io/athenas-core/

---

## 4. Reddit — r/selfhosted

**Title:** Athena's Core — a 15MB self-hosted AI coding IDE with multi-agent swarm (no Electron, no cloud required)

**Body:**

If you self-host your tools and want to keep your AI coding workspace local, Athena's Core runs entirely on your machine — no cloud dependency required.

**What makes it interesting for self-hosters:**

- **Local LLM support** — connect to LM Studio or any OpenAI-compatible local endpoint. Your code never leaves your machine.
- **~15MB binary** — Rust + Tauri 2, no Electron, no bundled Chromium.
- **Multi-agent swarm** — launch autonomous AI agents (Coordinator, Builder, Scout, Reviewer) that work on tasks while you do something else. They communicate via a mailbox bus and persist state to disk.
- **Multi-pane terminal** — full ANSI/VT100 emulation, grid up to 4×4, OSC 633 shell integration.
- **Mobile companion** — mirror your desktop to your phone over your LAN (PWA, no app store needed). Terminal commands, AI chat, file read/save from your phone.
- **Plugin system** — extend it with JSON manifests. Capability scoping, security validation, per-agent plugin sessions.
- **Embeds a browser** — native Tauri webview panel, not an iframe.

**Self-hosting options:**
- Use local models exclusively (LM Studio, Ollama, any OpenAI-compatible endpoint)
- All data stays in SQLite on your disk
- No telemetry, no cloud sync — everything is local-first

MIT licensed, free and open source.

GitHub: https://github.com/TOX9C/athenas-core
Website: https://tox9c.github.io/athenas-core/

---

## Posting Schedule

1. **Show HN** — post early morning US time (8-9am EST / 3-4pm Baghdad time)
2. **r/rust** — same day, shortly after Show HN
3. **r/programming** — 2-3 hours after r/rust (don't post all at once, let each gain traction)
4. **r/selfhosted** — next day

## Other places to post

- [ ] dev.to — write a longer-form "How I built X" article
- [ ] r/programmingtools
- [ ] r/tauri
- [ ] r/rustgamedev (for the Tauri/WASM angle, not game-specific but community is technical)
- [ ] Twitter/X — thread with screenshots/demo video
- [ ] Lobsters (lobste.rs) — similar to HN but more technical, needs invite
- [ ] Product Hunt — once you have a pre-built binary for download
- [ ] Hacker News comments — engage with feedback on the Show HN post
- [ ] Discord servers — Rust community, Tauri community, self-hosting communities
