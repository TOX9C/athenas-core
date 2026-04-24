# Athena's Core — Bug Fix Prompt

> Read `plan.md` for full architecture context. Fix every issue below. Test each fix before moving on.

---

## Bug 1: Sidebar Cannot Be Reopened After Collapsing

**Problem:** When the sidebar is collapsed, there is no way to bring it back. The toggle button disappears with the sidebar.

**Fix:**
- The sidebar toggle button (`Cmd/Ctrl+\`) must ALWAYS be visible, even when the sidebar is collapsed
- Add a small (28px wide) persistent rail/strip on the left edge when the sidebar is collapsed. This rail should contain:
  - A hamburger menu icon (☰) or chevron-right (→) button at the top that expands the sidebar on click
  - Optionally show small icon-only versions of the sidebar sections (Spaces, Files, Agents) that expand the sidebar when clicked
- The keyboard shortcut `Cmd/Ctrl+\` must toggle the sidebar regardless of current state
- The rail should use `var(--bgSecondary)` background with a `var(--border)` right border
- Animate the sidebar open/close with framer-motion (200ms ease)

---

## Bug 2: Purple Color Scheme — Replace with Dark Theme

**Problem:** The entire app is purple-tinted. It should be a proper dark theme using the `void` theme from `plan.md`.

**Fix:**
- Set the default theme to `void` (bg: `#0a0a0a`, accent: `#6366f1`)
- The `void` theme accent is indigo (`#6366f1`), NOT purple. Make sure:
  - Primary buttons use the accent color (`#6366f1` indigo) — this is subtle, not overwhelming purple
  - Backgrounds are near-black: `#0a0a0a` (primary), `#111111` (secondary), `#1a1a1a` (tertiary)
  - Text is white/gray: `#e4e4e7` (primary), `#a1a1aa` (muted), `#71717a` (dim)
  - Borders are subtle dark: `#27272a`
  - Accent should only appear on: active tab indicators, focused inputs, primary action buttons, selected items. NOT on backgrounds, sidebars, or large surfaces
- Audit every component for hardcoded purple/violet colors and replace with CSS variable references
- The overall feel should be **near-black with subtle gray borders** — accent color used sparingly for interactive elements only
- Terminal background should be pure dark: `#0a0a0a` or `#000000`

---

## Bug 3: Terminal Shells Not Working (Cannot Type)

**Problem:** After creating a workspace with e.g. "Quad" layout, the terminal panes show a cursor but typing does nothing. The shells are not connected to real PTY processes.

**Fix — Check each of these in order:**

1. **PTY spawn is actually being called:** When a Space is created and the grid renders, each `TerminalPane` must call `window.athena.pty.spawn(paneId, workspaceDir, defaultShell)` on mount. Verify this is happening — add a `console.log` in `ptyManager.ts` to confirm spawn requests arrive.

2. **IPC channels are registered:** In `main.ts`, verify ALL `pty:*` channels are registered with `ipcMain.handle` (for request/response like `pty:spawn`) or `ipcMain.on` (for fire-and-forget like `pty:write`, `pty:resize`). Common mistake: using `handle` for write/resize when it should be `on`, or vice versa.

3. **Data flows back to renderer:** After `node-pty` spawns, its `onData` callback must send data to the renderer via `mainWindow.webContents.send('pty:data:' + id, data)`. The renderer must listen for this in `useTerminal.ts` via `window.athena.pty.onData(paneId, callback)` and write received data to the xterm.js instance with `terminal.write(data)`.

4. **Keyboard input flows to PTY:** In `useTerminal.ts`, the xterm.js `onData` event must forward keystrokes to the PTY: `terminal.onData((data) => window.athena.pty.write(paneId, data))`. This is the most commonly missed wiring.

5. **Terminal is properly attached to DOM:** xterm.js `terminal.open(containerElement)` must be called AFTER the container div is mounted in the DOM. Use a `useEffect` with a ref. Then call `fitAddon.fit()`.

6. **Shell binary exists:** Use `platformUtils.ts` to detect the default shell. On macOS: `/bin/zsh`. On Linux: check `$SHELL` or fallback to `/bin/bash`. On Windows: `powershell.exe`.

7. **node-pty is properly loaded:** Since node-pty is a native module, it must be loaded in the main process only. Verify the import works: `import * as pty from 'node-pty'`. If it fails, the native addon may not be rebuilt — run `npx electron-rebuild` after `npm install`.

**Verification:** After fixing, create a Quad workspace. All 4 panes must show a shell prompt. Type `echo hello` and press Enter — output must appear.

---

## Bug 4: Swarm Launch Flow Is Confusing

**Problem:** The swarm can only be launched from within a workspace, but there's no clear entry point. The user expects to choose between "launch terminals" or "launch a swarm" from the beginning.

**Fix:**
- Modify the `NewSpaceModal.tsx` to add a **Step 0 — Mode Selection** before the current steps:
  - Two large clickable cards:
    1. **Terminal Workspace** — icon: terminal icon, description: "Launch multiple terminal panes with AI agents"
    2. **Swarm Mission** — icon: network/swarm icon, description: "Orchestrate a team of agents on a shared goal"
  - Selecting "Terminal Workspace" → proceeds to the existing 3-step flow (name, grid, agents)
  - Selecting "Swarm Mission" → proceeds to a modified flow:
    - Step 1: Name + Directory (same as before)
    - Step 2: Swarm config (goal, team size, role assignment — contents of current `SwarmModal.tsx`)
    - Step 3: Launch
- Remove the separate "Launch Swarm" button from the workspace toolbar (or keep it as a secondary option for launching a swarm within an existing workspace)
- When a Swarm is launched, the workspace view should automatically switch to the SwarmBoard view with the terminal panes visible in a split below

---

## Bug 5: Kanban Board Not Functional

**Problem:** The Kanban board exists but doesn't seem connected to anything.

**Fix:**
- Ensure `taskStore.ts` properly persists tasks per workspace (keyed by `spaceId`)
- The "+ Add Task" button must work: clicking it should show an inline text input at the bottom of the Todo column, typing + Enter creates the task
- Drag and drop between columns must update the task's `status` field and persist immediately
- Each task card must show the agent type badge if assigned
- The "Run Task" button on each card must:
  1. Find the terminal pane assigned to that agent type (or the first available pane)
  2. Write the task as a prompt to that pane's PTY
  3. Move the task to `in_progress`

---

## After All Fixes

Run `npm run dev` and verify:
- [ ] Sidebar collapses and can be reopened via rail button or `Cmd+\`
- [ ] App uses dark near-black theme, accent color used sparingly
- [ ] All terminal panes accept input and show shell output
- [ ] New Space modal offers "Terminal Workspace" vs "Swarm Mission" choice
- [ ] Kanban add/drag/run-task all work
