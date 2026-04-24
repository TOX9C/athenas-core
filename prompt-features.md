# Athena's Core — Feature Prompt: Athena AI Assistant + Notifications

> Read `plan.md` for full architecture context. Implement these features AFTER all bugs in `prompt-bugfixes.md` are resolved.

---

## Feature 1: Athena AI Assistant Panel

### Concept
Every workspace has a built-in AI assistant called **Athena**. Athena is a chat interface where the user can give natural language commands. Athena can:
- Launch agent instances (Claude Code, Codex, etc.) in terminal panes
- Give each agent a specific prompt/task
- Communicate with agents individually
- Orchestrate work across multiple agents

### Architecture

**Backend — How Athena works:**
- Athena is powered by a configurable AI CLI. Default: `claude` (Claude Code). Configurable in Settings → Athena.
- Athena runs in its own PTY session (hidden from the terminal grid, managed separately)
- Athena's PTY is spawned when a workspace opens, with a system prompt injected:

```
You are Athena, the AI orchestrator for this workspace. The user will give you instructions.

You have access to the terminal in this workspace directory: {workspaceDir}

When the user asks you to launch agents, you can do so by telling the user what commands to run, or by directly executing them.

Available agent CLIs:
- claude --dangerously-skip-permissions (Claude Code, bypass mode)
- codex (Codex CLI)
- opencode (OpenCode CLI)
- gemini (Gemini CLI)

Your capabilities:
1. When asked to launch N agents, tell the orchestration system to spawn N terminal panes with the specified agent type
2. When asked to give agents tasks, you compose prompts and the system injects them into each agent's terminal
3. You can read the Kanban board state to understand what tasks exist
4. You monitor agent completion notifications and can report status

Keep responses concise. You are a command center, not a chatbot.
```

- However, Athena doesn't directly control the app. Instead, the chat interface parses Athena's responses and the **app** performs the actions (spawning panes, injecting prompts). This is a UI-driven orchestration layer.

**Frontend — Chat UI:**

### New files to create:
```
src/components/Athena/
├── AthenaPanel.tsx        # Main chat panel (right sidebar or overlay)
├── AthenaChatMessage.tsx  # Single message bubble
├── AthenaInput.tsx        # Text input + send button
└── useAthena.ts           # Hook: manages Athena PTY, message history, parsing
```

### `AthenaPanel.tsx`
- A panel that slides in from the right side (320px wide) or opens as an overlay
- Toggle via a button in the top toolbar — use an owl/wisdom icon or a Greek helmet icon from lucide-react (or use `Brain` icon)
- Also toggle via keyboard shortcut: `Cmd/Ctrl+J`
- **Header:** "Athena" label + model indicator (e.g., "Claude Code") + settings gear icon + close button
- **Chat area:** scrollable list of `AthenaChatMessage` bubbles
  - User messages: aligned right, accent-colored background
  - Athena messages: aligned left, `var(--bgSecondary)` background
  - Support markdown rendering in Athena's responses (use a simple markdown-to-HTML renderer or just preserve code blocks and line breaks)
- **Input area:** text input at the bottom with a send button (arrow-up icon). Enter to send, Shift+Enter for newline.

### `useAthena.ts` Hook
- Spawns a dedicated PTY for Athena on workspace open (using the configured CLI)
- Launches the CLI in bypass mode: `claude --dangerously-skip-permissions` (for Claude Code)
- Maintains a message history array in the Athena Zustand store
- When user sends a message:
  1. Add to message history as `{ role: 'user', content: text }`
  2. Write the message text to Athena's PTY
  3. Collect PTY output until the next prompt marker (same approach as commandParser.ts)
  4. Add collected output as `{ role: 'athena', content: output }`
- Parse Athena's responses for actionable commands (future enhancement — for now, just display the chat)

### Add to Zustand store:
```typescript
// src/store/athenaStore.ts
interface AthenaMessage {
  id: string
  role: 'user' | 'athena'
  content: string
  timestamp: number
}

interface AthenaState {
  messages: AthenaMessage[]
  isOpen: boolean
  isPtyReady: boolean
  model: string          // 'claude' | 'codex' | 'gemini' | custom
  bypassMode: boolean    // default true for claude
}
```

### Settings → Athena (new settings section)
Add a 6th tab in `SettingsModal.tsx`:
- **Model**: dropdown — Claude Code, Codex, OpenCode, Gemini CLI, Custom
- **Custom command**: text input (shown when Custom selected)
- **Bypass permissions mode**: toggle (default ON for Claude Code). When on, appends `--dangerously-skip-permissions` to the claude command
- **Auto-launch Athena on workspace open**: toggle (default ON)

---

## Feature 2: Agent Launch in Bypass Permission Mode

When launching AI agents (Claude Code specifically) in terminal panes:
- Update `src/utils/agentCommands.ts` to support bypass mode:

```typescript
export function getAgentCommand(type: AgentType, options?: { bypass?: boolean }): string {
  switch (type) {
    case 'claude':
      return options?.bypass ? 'claude --dangerously-skip-permissions' : 'claude'
    case 'codex':
      return 'codex'
    case 'opencode':
      return 'opencode'
    case 'gemini':
      return 'gemini'
    case 'custom':
      return '' // user provides
    case 'shell':
      return '' // no agent
  }
}
```

- Add a **"Bypass permissions"** toggle in the `AgentPicker.tsx` (Step 3 of New Space modal). Default ON for Claude Code.
- Store the bypass preference per pane in `PaneConfig`:
```typescript
interface PaneConfig {
  id: string
  agentType: AgentType
  customCmd?: string
  label?: string
  bypassMode?: boolean   // NEW — default true for claude
}
```

---

## Feature 3: Completion Sound + Notification System

### Sound on agent/command completion

When a terminal pane finishes executing a command or an agent completes a task:

1. **Generate a notification sound:**
   - Create a short, pleasant "ding" sound using the Web Audio API (no external sound files needed)
   - In `src/utils/notificationSound.ts`:
   ```typescript
   export function playDing() {
     const ctx = new AudioContext()
     const oscillator = ctx.createOscillator()
     const gainNode = ctx.createGain()
     oscillator.connect(gainNode)
     gainNode.connect(ctx.destination)
     oscillator.frequency.value = 880  // A5 note
     oscillator.type = 'sine'
     gainNode.gain.setValueAtTime(0.3, ctx.currentTime)
     gainNode.gain.exponentialRampToValueAtTime(0.001, ctx.currentTime + 0.5)
     oscillator.start(ctx.currentTime)
     oscillator.stop(ctx.currentTime + 0.5)
   }
   ```

2. **Trigger conditions:**
   - When a PTY process exits (agent finished)
   - When a command block completes (exit code received)
   - Do NOT play sound for the user's own shell commands (only for agent CLIs)
   - Add a "Mute notifications" toggle in Settings → General

3. **Detection of agent completion:**
   - Listen for `pty:exit:{id}` events — if the pane's agent type is not `shell`, play ding
   - For long-running agents (Claude Code), detect when they return to an idle/prompt state

### Notification Center (top right)

**New files:**
```
src/components/Notifications/
├── NotificationBell.tsx     # Bell icon + badge in top toolbar
├── NotificationDropdown.tsx # Dropdown list of notifications
└── NotificationItem.tsx     # Single notification entry
```

**Add to store:**
```typescript
// src/store/notificationStore.ts
interface Notification {
  id: string
  paneId: string
  paneName: string        // e.g., "Claude Code - Pane 3"
  agentType: AgentType
  message: string         // e.g., "Task completed" or "Agent exited"
  timestamp: number
  read: boolean
  spaceId: string
}
```

### `NotificationBell.tsx`
- Position: top-right of the titlebar/toolbar area
- Shows a bell icon (lucide `Bell`)
- When there are unread notifications, show a red badge with the count
- Click to open `NotificationDropdown.tsx`

### `NotificationDropdown.tsx`
- Slides down from the bell icon (or appears as a popover)
- List of `NotificationItem` entries, newest first
- Each item shows:
  - Agent type icon + colored dot
  - Pane name (e.g., "Claude Code — Pane 2")
  - Message (e.g., "Finished task" / "Process exited")
  - Relative timestamp ("2m ago")
  - Click → navigates to that pane (switches to the correct workspace tab if needed, focuses the pane)
- "Mark all as read" button at the top
- "Clear all" button
- Max 50 notifications stored, oldest auto-pruned

### Integration
- When a ding plays, also create a `Notification` entry in the store
- The `NotificationBell` badge count updates reactively via Zustand
- Clicking a notification:
  1. Switch to the workspace containing that pane (if not already active)
  2. Focus/highlight that terminal pane (scroll to it, add a brief glow animation)
  3. Mark the notification as read

---

## Feature 4: Kanban Board Visible to Agents

Each agent instance in a workspace should be able to see the Kanban board state. When an agent is launched with a task from the Kanban:

- The task's title and description are injected as a prompt into the agent's terminal
- The agent's PTY pane header shows which Kanban task it's working on (task title as a small badge)
- When an agent completes (PTY exits or returns to idle), the associated Kanban task should be automatically moved to `in_review`

This is implemented in the app layer — the app manages the task lifecycle, not the agents.

---

## Keyboard Shortcut Additions

Add these to the existing shortcut registry:

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl+J` | Toggle Athena panel |
| `Cmd/Ctrl+Shift+N` | Focus notification dropdown |

---

## After Implementation

Verify:
- [ ] Athena panel opens/closes with `Cmd+J` or toolbar button
- [ ] Can type messages to Athena, responses appear in chat
- [ ] Athena's PTY uses configured model with bypass mode
- [ ] Settings → Athena section exists with model/bypass/auto-launch options
- [ ] Claude Code agents launch with `--dangerously-skip-permissions` by default
- [ ] Ding sound plays when an agent finishes (not for regular shell commands)
- [ ] Notification bell appears top-right with unread badge count
- [ ] Clicking a notification navigates to the correct pane
- [ ] Sound can be muted from Settings → General
