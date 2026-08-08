# Athena's Core — Beta Release v0.3.0

Thank you for testing Athena's Core! This is a private beta — please don't share this build publicly.

---

## Installation (macOS, Apple Silicon)

1. Open `Athena's Core_0.3.0_aarch64.dmg`
2. Drag **Athena's Core.app** into your **Applications** folder
3. Launch from Applications (or Spotlight)

> **First launch gate:** macOS may block the app because it's unsigned. Right-click → **Open** → confirm **Open anyway**. This is normal for beta/apps not distributed through the App Store.

---

## What You're Testing

Athena's Core is a desktop IDE for AI-assisted development. It combines:

- **Multi-pane terminal** — multiple shell sessions side by side
- **AI chat assistant** — talk to Claude or other LLMs inline
- **Swarm orchestration** — launch multiple AI agents on a shared goal
- **Kanban task board** — track tasks across columns
- **Plugin system** — extensible AI agent plugins
- **In-app browser** — surf docs without leaving the workspace
- **16 color themes** — customize the look

---

## New in This Build: Agent Activity Detection

Athena's Core now **sees which AI agents are running inside each pane** and keeps you in the loop without staring at the terminal:

- **Per-pane status dot** — every pane header shows a live dot: gold = agent working, pulsing gold = thinking/warming up, amber (pulsing) = finished / waiting for input / errored.
- **Space tab badges** — each space tab shows `[Working] [Total] [Attention]`: how many agents are actively working (left of the total, as requested), how many agents are in the space, and how many need your attention (amber).
- **Notifications** — when an agent finishes its work, asks a question, or errors, you get an in-app notification **and** a macOS notification with sound (per type), plus a dock badge with the unread count. Toggle each notification type in **Settings → Agents → Agent Notifications**.
- **Detected agents** — Claude Code, Codex, OpenCode, Gemini CLI, Qwen, Aider, Cursor CLI, Freebuff, and OMP (oh my pi). Task titles are scraped from agent session files where formats exist (Claude/Codex/Qwen/Aider).

## What to Focus On

Please test these areas and report **anything** that feels broken, confusing, or crashes:

1. **Workspace** — create new spaces, add terminal panes, switch between panels
2. **AI Chat** — send a message, verify the response streams back
3. **Swarm** — click the swarm icon, type a goal, hit Launch, verify New Space modal opens
4. **Kanban** — add/edit/move tasks between columns
5. **Settings** — change theme, adjust terminal font size, toggle agents
6. **Plugins** — browse the plugin list, verify cards render

---

## Known Issues

### ⚠️ App may freeze after clicking around (WASM crash)

This is the **#1 issue we're tracking**. The app uses a web-based UI layer (Dioxus/WASM) that can crash inside macOS's WebView. You'll see the app window freeze or go blank — the backend keeps running, but the UI stops responding.

**If it happens:** quit the app (⌘Q) and relaunch. Your workspace state isn't lost — it recenters on the last open directory.

**What helps us:** note what you clicked right before the freeze. Was it a button? A tab switch? A modal? That helps us narrow down the trigger.

### macOS only

This build is for Apple Silicon Macs only. Windows and Linux builds are planned but not yet available.

---

## How to Report Feedback

For each issue you find, please share:

1. **What you did** — steps to reproduce (be specific: "clicked the swarm icon, typed 'fix all bugs', hit Launch")
2. **What happened** — freeze? error message? wrong behavior? nothing?
3. **What you expected** — what should have happened instead

Screenshots or screen recordings are hugely helpful if the app is in a broken state.

---

## Version

- **Release:** v0.3.0 (beta)
- **Platform:** macOS (Apple Silicon / arm64)
- **Date:** July 2025

---

Thanks again for helping us ship. Every bug you find before public launch is one fewer bug our users hit.
