# Athena Orchestrator Bugfixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the ESM require error in `mcpServer.ts`, implement terminal history scrollback to persist spaces, and disable TUI artifacts in Athena CLI output.

**Architecture:**

1. Use dynamic `import()` for `electron-store` in `mcpServer.ts`.
2. Add a `history` map in `ptyManager.ts` that caches stdout/stderr. Expose `pty:getHistory` over IPC and update frontend `Terminal` to restore it on mount.
3. Inject `CI=1` and `TERM=dumb` specifically for Athena PTY agents to force headless CLI output.

**Tech Stack:** TypeScript, Electron IPC, node-pty, xterm.js.

---

### Task 1: Fix ESM require error in `electron/mcpServer.ts`

**Files:**

- Modify: `electron/mcpServer.ts`

- [ ] **Step 1: Replace static import with dynamic import**
      Remove `import Store from 'electron-store'` and `const store = new Store()`.
      Add a dynamic getter:

```typescript
let storeInstance: any = null
async function getStore() {
  if (!storeInstance) {
    const { default: Store } = await import('electron-store')
    storeInstance = new Store()
  }
  return storeInstance
}
```

- [ ] **Step 2: Update handleToolCall**
      Update `handleToolCall` to await `getStore()`:

```typescript
  try {
    const store = await getStore()
    const tasks: any[] = store.get('tasks') as any[] || []

    if (name === 'create_tasks') {
```

- [ ] **Step 3: Commit**

```bash
git add electron/mcpServer.ts
git commit -m "fix(mcp): resolve electron-store esm require error"
```

---

### Task 2: Implement Terminal Scrollback Buffer

**Files:**

- Modify: `electron/ptyManager.ts`
- Modify: `electron/main.ts`

- [ ] **Step 1: Cache strings in `ptyManager.ts`**
      In `ptyManager.ts`, declare a `history` Map and an exported getter.

```typescript
const history = new Map<string, string>()

export function getHistory(id: string): string {
  return history.get(id) || ''
}
```

Inside `spawn()`, clear the history for the id, and append to it on `onData`:

```typescript
sessions.set(id, ptyProcess)
history.set(id, '')

ptyProcess.onData((data) => {
  const current = history.get(id) || ''
  history.set(id, current + data)
  mainWindow.webContents.send(`pty:data:${id}`, data)
})
```

- [ ] **Step 2: Bridge `getHistory` in `main.ts`**
      In `electron/main.ts` inside `app.whenReady().then(...)`, add an IPC handler:

```typescript
ipcMain.handle('pty:getHistory', async (_event, id: string) => {
  return ptyMgr.getHistory(id)
})
```

- [ ] **Step 3: Commit**

```bash
git add electron/ptyManager.ts electron/main.ts
git commit -m "feat(terminal): implement pty scrollback buffer and ipc getter"
```

---

### Task 3: TUI Stripping & Frontend History Restore

**Files:**

- Modify: `electron/ptyManager.ts`
- Modify: `src/components/Sidebar/Terminal.tsx` (or where the xterm component runs)

- [ ] **Step 1: Inject ENV vars for Athena**
      In `electron/ptyManager.ts`, modify `env` in `pty.spawn`. If `id` includes `__athena__`, pass `CI='1'` and `TERM='dumb'`:

```typescript
const isAthena = id.includes('__athena__')
const customEnv = isAthena
  ? { ...process.env, CI: '1', TERM: 'dumb', FORCE_COLOR: '0', NO_COLOR: '1' }
  : process.env

const ptyProcess = pty.spawn(shellPath, shellArgs, {
  name: isAthena ? 'dumb' : 'xterm-256color',
  cols: 80,
  rows: 24,
  cwd,
  env: customEnv as Record<string, string>,
})
```

- [ ] **Step 2: Fetch history on mount in frontend Terminal**
      Before doing this, let's verify where `xterm` runs. Assuming `src/components/Sidebar/Terminal.tsx`. Add a `window.athena.pty.getHistory(paneId)` fallback or similar.
      Wait, let's look at `preload.ts` to see if `getHistory` exists. The subagent will need to add it to `preload.ts`.

In `electron/preload.ts`, inside the `pty` wrapper:

```typescript
    getHistory: (id: string) => ipcRenderer.invoke('pty:getHistory', id),
```

Then in the React component handling the terminal (`src/components/Settings/Terminal.tsx` or similar - subagent must `find` the exact file, usually `src/components/Terminal/Terminal.tsx`), load history:

```typescript
// Inside useEffect for xterm initialization
window.athena.pty.getHistory(paneId).then((hist) => {
  if (hist) term.write(hist)
})
```

- [ ] **Step 3: Commit**

```bash
git add electron/ptyManager.ts electron/preload.ts src/components/
git commit -m "fix(terminal): restore terminal history on mount and disable athena tui"
```
