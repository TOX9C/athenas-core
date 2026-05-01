# Performance Fix Plan — CPU 100% / RAM 60%

## Overview

You are fixing a series of performance issues in an Electron app that causes CPU usage to spike to 100% and RAM to sit at 60%. The problems span the PTY manager, React frontend, IPC layer, and AI orchestrator.

The relevant files are:

- `electron/ptyManager.ts` — PTY process management and terminal history
- `electron/main.ts` — main process, file watcher, IPC setup
- `electron/athenaOrchestrator.ts` — AI orchestration loop and message history
- `electron/swarmCoordinator.ts` — swarm polling coordinator
- `src/components/Terminal/TerminalPane.tsx` — terminal React component
- `src/hooks/useTerminal.ts` — terminal IPC hook
- `src/App.tsx` — root component, keyboard listeners, space rendering

**Read every file listed above in full before making any changes.** Do not assume variable names, types, or architectural patterns — derive everything from the actual source.

Work through fixes in the order listed below. Checkpoint or commit after each phase so regressions are easy to isolate.

---

## Phase 1 — Terminal & PTY (Do This First — Highest Impact)

### Fix 1.1 — Replace PTY History String Concatenation with a Circular Buffer (`electron/ptyManager.ts`)

**Problem:**
The terminal history buffer is built by continuously concatenating strings (`current + data`) on every single data event from the PTY process. At high output volume (e.g. running commands, AI responses streaming in), this creates a new string in memory on every event, causing extreme garbage collection thrashing and ballooning RAM.

**Fix:**

- Replace the string concatenation buffer with an array-based chunk queue.
- On each PTY data event, push the new chunk into the array instead of concatenating.
- When the total size of chunks exceeds the buffer limit (currently 100KB — find the actual constant in the code), shift old chunks off the front of the array until it fits.
- Only call `.join("")` on the array when the history string is explicitly requested (e.g. when a consumer calls `getHistory()`).
- Do not `.join()` on every data event.

**Pseudocode:**

```typescript
// Instead of:
this.history = (this.history + data).slice(-MAX_HISTORY);

// Do:
this.historyChunks.push(data);
this.historySize += data.length;
while (this.historySize > MAX_HISTORY) {
  const removed = this.historyChunks.shift();
  this.historySize -= removed.length;
}

// Only when history is needed:
getHistory(): string {
  return this.historyChunks.join("");
}
```

---

### Fix 1.2 — Move Ready-State Detection to Main Process, Remove Frontend Polling (`electron/ptyManager.ts` + `src/components/Terminal/TerminalPane.tsx`)

**Problem:**
`TerminalPane.tsx` uses a `setTimeout` loop to repeatedly request the entire terminal history string (100KB) over IPC, then runs a regex on the result in the React render thread to check whether the terminal prompt is ready. This floods the IPC channel with large payloads and blocks the main thread with regex on every tick.

**Fix:**

**In `ptyManager.ts`:**

- On each incoming PTY data chunk, run the ready-state regex against the small incoming chunk only (not the full history).
- When a match is detected, emit a lightweight `pty:ready` IPC event to the renderer with just the session ID. No history payload.
- Do this detection once per chunk — not on a timer.

**In `TerminalPane.tsx`:**

- Remove the `setTimeout` polling loop entirely.
- Instead, listen for the `pty:ready` IPC event and update state when it fires.
- Never request the full history string over IPC for the purpose of ready-state detection.

**Pseudocode (ptyManager.ts):**

```typescript
ptyProcess.onData((chunk) => {
  this.pushChunk(chunk) // circular buffer from Fix 1.1
  if (READY_REGEX.test(chunk)) {
    win.webContents.send('pty:ready', { sessionId: this.id })
  }
  win.webContents.send('pty:data', { sessionId: this.id, data: chunk })
})
```

**Pseudocode (TerminalPane.tsx):**

```typescript
// Remove this entirely:
// const poll = setTimeout(() => { fetchHistory(); checkReady(); }, 200);

// Replace with:
useEffect(() => {
  const unsub = window.athena.pty.onReady((sessionId) => {
    if (sessionId === mySessionId) setIsReady(true)
  })
  return unsub
}, [mySessionId])
```

---

### Fix 1.3 — De-duplicate IPC Listeners (`src/hooks/useTerminal.ts` + `src/components/Terminal/TerminalPane.tsx`)

**Problem:**
The `pty:data` IPC channel is subscribed to in multiple places, causing every incoming data chunk to be deserialized and processed more than once per event.

**Fix:**

- Audit all usages of `window.athena.pty.onData` across the codebase.
- Ensure `useTerminal.ts` is the single subscriber to `pty:data`.
- `TerminalPane.tsx` and any other components should consume terminal data through the hook's return values or a shared context, not by subscribing to IPC directly.
- Remove any duplicate `onData` registrations found outside `useTerminal.ts`.

---

## Phase 2 — React Frontend (Main Thread Blocking)

### Fix 2.1 — Fix `useEffect` Keyboard Listener Dependency Array (`src/App.tsx`)

**Problem:**
A `useEffect` that registers global keyboard listeners has fast-changing values (like `platform` or `spaces`) in its dependency array. Every time a space is added, removed, or updated, the effect tears down and re-registers all global keyboard listeners. This fires far more often than needed.

**Fix:**

- Locate the `useEffect` that registers global `keydown`/`keyup` listeners in `App.tsx`.
- Move any values that change frequently out of the dependency array.
- Use `useRef` to hold a stable reference to the current handler function, and update the ref on each render without re-registering the listener.
- The event listener itself should only be registered once on mount and removed on unmount.

**Pseudocode:**

```typescript
const handlerRef = useRef(null)
handlerRef.current = (e) => handleKeyDown(e, platform, spaces) // always fresh

useEffect(() => {
  const handler = (e) => handlerRef.current(e)
  window.addEventListener('keydown', handler)
  return () => window.removeEventListener('keydown', handler)
}, []) // empty deps — register once only
```

---

### Fix 2.2 — Replace `requestAnimationFrame` Resize Loops with Debounced `ResizeObserver`

**Problem:**
Editor and terminal resize handling uses `requestAnimationFrame` in a loop, which monopolizes the UI thread and fires continuously even when nothing is resizing.

**Fix:**

- Find all `requestAnimationFrame` calls used for resize detection or layout measurement.
- Replace them with a `ResizeObserver` that fires only when the element's size actually changes.
- Debounce the `ResizeObserver` callback with a 50–100ms debounce to avoid firing on every sub-pixel change during a drag.
- Disconnect the observer in the cleanup function.

**Pseudocode:**

```typescript
useEffect(() => {
  const observer = new ResizeObserver(
    debounce((entries) => {
      const { width, height } = entries[0].contentRect
      handleResize(width, height)
    }, 50),
  )
  observer.observe(containerRef.current)
  return () => observer.disconnect()
}, [])
```

---

### Fix 2.3 — Unmount Inactive Spaces Instead of Using `display: none` (`src/App.tsx` / terminal grid)

**Problem:**
Inactive workspaces are hidden with CSS `display: none`, but their full React trees remain mounted. This keeps all xterm.js Canvas instances, WebGL contexts, and DOM nodes alive in memory for every workspace the user has ever opened.

**Fix:**

- Find where inactive spaces are rendered but hidden (look for `display: none`, `visibility: hidden`, or a `hidden` className conditional).
- Replace the hide logic with conditional rendering (`{isActive && <SpaceComponent />}`).
- Before unmounting, serialize any state that needs to be restored when the space becomes active again (scroll position, terminal buffer snapshot, etc.) into a lightweight store.
- On remount, rehydrate from the snapshot.
- If full unmounting is too destructive for terminal sessions (PTY processes must stay alive), at minimum dispose the xterm.js instance and Canvas while keeping the PTY process running in the background. Reconnect a new xterm.js instance on reactivation.

---

### Fix 2.4 — Cap Zustand Store Array Growth (`athenaStore`, `taskStore`)

**Problem:**
Arrays in `athenaStore` and `taskStore` grow indefinitely using spread patterns (`[...prev, newItem]`), and are never pruned.

**Fix:**

- After every push to a bounded array (messages, tasks, logs), slice it to a maximum length.
- Suggested caps: 100 messages, 200 tasks, 500 log entries — find the actual array names in the stores and apply caps that make sense for the data type.
- If the user needs to see older entries, implement pagination or a "load more" pattern rather than keeping everything in memory.

**Pseudocode:**

```typescript
// Instead of:
messages: [...prev.messages, newMessage]

// Do:
messages: [...prev.messages, newMessage].slice(-MAX_MESSAGES)
```

---

## Phase 3 — Main Process & AI Orchestrator

### Fix 3.1 — Replace Recursive File Watcher with `chokidar` (`electron/main.ts`)

**Problem:**
`fs.watch(dir, { recursive: true })` is watching everything including `node_modules`, `.git`, and other massive directories. It opens thousands of file handles immediately on startup and emits IPC events to the renderer on every change without any debouncing.

**Fix:**

- Install `chokidar` if not already present (`npm install chokidar`).
- Replace the `fs.watch` call with a `chokidar.watch()` call.
- Add an `ignored` pattern that excludes `node_modules`, `.git`, `dist`, `out`, `.cache`, and any other non-user directories. Use a regex or glob pattern.
- Add a 300ms debounce to the IPC emit so that bursts of file change events (e.g. a `git checkout` or `npm install`) are collapsed into a single notification.
- Close the watcher properly in the app `before-quit` event.

**Pseudocode:**

```typescript
import chokidar from 'chokidar'

const watcher = chokidar.watch(projectDir, {
  ignored: /(node_modules|\.git|dist|out|\.cache)/,
  persistent: true,
  ignoreInitial: true,
})

const emitChange = debounce((path) => {
  win.webContents.send('fs:changed', { path })
}, 300)

watcher.on('all', (event, path) => emitChange(path))

app.on('before-quit', () => watcher.close())
```

---

### Fix 3.2 — Add Iteration Cap and Context Truncation to AI Orchestrator (`electron/athenaOrchestrator.ts`)

**Problem:**
The `while (true)` orchestration loop has no exit condition for error states or runaway AI behavior. `this.openaiMessages` (and the Anthropic equivalent) grows indefinitely, eventually consuming all available RAM and causing context window overflow errors.

**Important:** This file also has an orchestration loop being added as part of a separate bug fix plan (`plan.md`). Coordinate these changes — do not duplicate the loop or overwrite the bug fix work. Apply the caps and truncation to whichever loop implementation exists in the file at the time you edit it.

**Fix — Iteration Cap:**

- Add an `iteration` counter that increments on every loop cycle.
- Define `MAX_ITERATIONS = 20` (or find if a constant already exists).
- If `iteration > MAX_ITERATIONS`, break the loop, log a warning, and return a message indicating the operation was halted to prevent runaway execution.

```typescript
let iteration = 0
const MAX_ITERATIONS = 20

while (true) {
  if (iteration++ > MAX_ITERATIONS) {
    console.warn('Athena: hit max iterations, halting.')
    return 'Operation halted: too many steps. Please try a more specific command.'
  }
  // ... rest of loop
}
```

**Fix — Context Truncation:**

- After every message push, check if `this.messages.length` exceeds a threshold (e.g. 50 messages).
- If it does, trim from the front of the array while always preserving the system prompt at index 0.
- Keep the most recent N messages so the AI retains short-term context.

```typescript
const MAX_MESSAGES = 50
const KEEP_RECENT = 20

if (this.messages.length > MAX_MESSAGES) {
  const systemPrompt = this.messages[0]
  const recent = this.messages.slice(-KEEP_RECENT)
  this.messages = [systemPrompt, ...recent]
}
```

---

### Fix 3.3 — Replace Swarm `setInterval` with Recursive `setTimeout` (`electron/swarmCoordinator.ts`)

**Problem:**
`setInterval` fires on a fixed clock regardless of whether the previous tick has finished executing. If a task takes longer than the interval period, executions overlap and CPU usage compounds.

**Fix:**

- Remove the `setInterval` call.
- Replace with a recursive `setTimeout` pattern: after each tick completes (including any async work), schedule the next tick with `setTimeout`.
- This guarantees the CPU gets to rest between operations.

**Pseudocode:**

```typescript
// Instead of:
setInterval(async () => {
  await processTasks()
}, INTERVAL_MS)

// Do:
async function tick() {
  try {
    await processTasks()
  } catch (err) {
    console.error('Swarm tick error:', err)
  } finally {
    setTimeout(tick, INTERVAL_MS) // always schedule next, even on error
  }
}
tick() // start the loop
```

---

## Additional Fixes (Apply Alongside the Above)

### Check xterm.js Renderer Type

In the xterm.js initialization code (likely in `TerminalPane.tsx` or a terminal utility file), verify the renderer type being used. If it is set to `"dom"` or unset, change it to `"webgl"` or `"canvas"`. The DOM renderer is significantly more CPU-intensive.

```typescript
const terminal = new Terminal({
  rendererType: 'webgl', // or "canvas" if webgl is unavailable
  // ... other options
})
```

### IPC Payload Size — Send Diffs, Not Snapshots

If any IPC channel is sending full state objects (agent state, terminal state, task lists) on every update, refactor those to send only the changed fields. Full snapshots over IPC are expensive to serialize, transfer, and deserialize on every event.

---

## Implementation Order

| Order | Fix                                        | File(s)                              | Impact      |
| ----- | ------------------------------------------ | ------------------------------------ | ----------- |
| 1     | PTY circular buffer                        | `ptyManager.ts`                      | 🔴 Critical |
| 2     | Move ready-state detection, remove polling | `ptyManager.ts`, `TerminalPane.tsx`  | 🔴 Critical |
| 3     | De-duplicate IPC listeners                 | `useTerminal.ts`, `TerminalPane.tsx` | 🟠 High     |
| 4     | Fix `useEffect` keyboard deps              | `App.tsx`                            | 🟠 High     |
| 5     | ResizeObserver                             | Terminal/Editor components           | 🟠 High     |
| 6     | Unmount inactive spaces                    | `App.tsx`, terminal grid             | 🟠 High     |
| 7     | File watcher → chokidar                    | `main.ts`                            | 🟠 High     |
| 8     | AI iteration cap + context truncation      | `athenaOrchestrator.ts`              | 🟠 High     |
| 9     | Cap Zustand arrays                         | `athenaStore`, `taskStore`           | 🟡 Medium   |
| 10    | Swarm setInterval → setTimeout             | `swarmCoordinator.ts`                | 🟡 Medium   |
| 11    | xterm.js renderer type                     | Terminal init                        | 🟡 Medium   |
| 12    | IPC diff payloads                          | Various                              | 🟡 Medium   |

---

## Testing Checklist

After all fixes are applied, verify the following:

- [ ] CPU stays below 20% at idle with terminals open
- [ ] CPU does not spike above 60% during normal terminal use
- [ ] RAM does not grow continuously over a 10-minute session
- [ ] Terminal ready-state is detected correctly without polling
- [ ] Switching between spaces does not cause memory to spike
- [ ] File changes in the project directory are detected correctly
- [ ] `node_modules` and `.git` changes do NOT trigger file watch events
- [ ] Athena does not loop infinitely on a malformed tool call
- [ ] Swarm coordinator does not show overlapping tick execution in logs
- [ ] No duplicate IPC listener warnings in the console
- [ ] xterm.js is using the WebGL or Canvas renderer (check via terminal init log or devtools)
