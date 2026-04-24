# Athena Settings Persistence & Lifecycle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ensure Athena AI settings (model, bypass mode, custom command, auto-launch) are saved to disk, loaded on app startup, and trigger an immediate terminal restart when the active model is changed.

**Architecture:** We will wire up `electron-store` persisting keys in `src/components/Settings/SettingsModal.tsx` and hydrating them in `src/App.tsx`. In `src/components/Athena/useAthena.ts`, we will introduce a `useEffect` that reacts to changes in `model`, `bypassMode`, or `customCommand` by automatically re-spawning the Athena background process (`node-pty`) if the session is currently active.

**Tech Stack:** React 18, Zustand, Electron (`electron-store`, Inter-Process Communication).

---

### Task 1: Persist Settings on Change

**Files:**
- Modify: `src/components/Settings/SettingsModal.tsx`

- [ ] **Step 1: Write explicit handlers for settings changes**

Replace the inline `onChange` functions inside the `tab === 'athena'` render block with explicit handler functions near the top of the component (e.g., right under `handleThemeChange`). Update the JSX controls to use these new handlers.

```tsx
  const handleThemeChange = (name: ThemeName) => {
    setTheme(name)
    applyTheme(themes[name])
    window.athena.store.set('theme', name)
  }

  const handleModelChange = (val: string) => {
    setModel(val)
    window.athena.store.set('athena-model', val)
  }

  const handleBypassChange = (val: boolean) => {
    setBypassMode(val)
    window.athena.store.set('athena-bypassMode', val)
  }

  const handleAutoLaunchChange = (val: boolean) => {
    setAutoLaunch(val)
    window.athena.store.set('athena-autoLaunch', val)
  }

  const handleCustomCmdChange = (val: string) => {
    setCustomCommand(val)
    window.athena.store.set('athena-customCommand', val)
  }
```

- [ ] **Step 2: Update the JSX elements to call the new handlers**

Replace `(e) => setModel(e.target.value)` with `(e) => handleModelChange(e.target.value)`.
Replace `(e) => setCustomCommand(e.target.value)` with `(e) => handleCustomCmdChange(e.target.value)`.
Replace `() => setBypassMode(!bypassMode)` with `() => handleBypassChange(!bypassMode)`.
Replace `() => setAutoLaunch(!autoLaunch)` with `() => handleAutoLaunchChange(!autoLaunch)`.

- [ ] **Step 3: Commit**

```bash
git add src/components/Settings/SettingsModal.tsx
git commit -m "feat(settings): persist athena model and launch preferences to electron-store"
```

---

### Task 2: Hydrate Settings on Startup

**Files:**
- Modify: `src/App.tsx`

- [ ] **Step 1: Load stored values into Zustand store in the App mount hook**

In the `useEffect` block that fetches `tasks` and `theme` via `window.athena.store.get()`, add the corresponding fetches for the new Athena keys.

```tsx
  useEffect(() => {
    window.athena.store.get('theme').then((saved: ThemeName | undefined) => {
      if (saved && themes[saved]) {
        useUIStore.getState().setTheme(saved)
        applyTheme(themes[saved])
      }
    })
    window.athena.store.get('spaces').then((saved: any) => {
      if (saved && Array.isArray(saved) && saved.length > 0) {
        useWorkspaceStore.getState().setSpaces(saved)
        useWorkspaceStore.getState().setActiveSpace(saved[saved.length - 1].id)
      }
    })
    window.athena.store.get('tasks').then((saved: any) => {
      if (saved && Array.isArray(saved)) {
        useTaskStore.getState().setTasks(saved)
      }
    })
    // Add Athena settings hydration
    window.athena.store.get('athena-model').then((saved: any) => {
      if (typeof saved === 'string') useAthenaStore.getState().setModel(saved)
    })
    window.athena.store.get('athena-bypassMode').then((saved: any) => {
      if (typeof saved === 'boolean') useAthenaStore.getState().setBypassMode(saved)
    })
    window.athena.store.get('athena-autoLaunch').then((saved: any) => {
      if (typeof saved === 'boolean') useAthenaStore.getState().setAutoLaunch(saved)
    })
    window.athena.store.get('athena-customCommand').then((saved: any) => {
      if (typeof saved === 'string') useAthenaStore.getState().setCustomCommand(saved)
    })
  }, [])
```

- [ ] **Step 2: Commit**

```bash
git add src/App.tsx
git commit -m "feat(app): hydrate athena persistence keys to memory on startup"
```

---

### Task 3: Manage PTY Lifecycle on Model Change

**Files:**
- Modify: `src/components/Athena/useAthena.ts`

- [ ] **Step 1: Add a watcher for model configuration changes**

Append a new `useEffect` hook near the bottom of `useAthena.ts`. If the settings change while `isPtyReady` is true, we immediately re-invoke `spawnAthena()`, which routes to `ptyManager.ts`'s `.kill()` automatically before spawning the new sub-process.

```tsx
  // Re-spawn the underlying agent PTY if the user modifies CLI parameters while it's running
  useEffect(() => {
    if (isPtyReady && activeSpace) {
      // The backend ptyManager.ts explicitly kills older sessions sharing the same ID during spawn
      spawnAthena()
    }
    // We intentionally only listen to configuration dependencies
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [model, bypassMode, customCommand])
```

- [ ] **Step 2: Commit**

```bash
git add src/components/Athena/useAthena.ts
git commit -m "fix(athena): automatically reboot AI PTY shell when model config changes"
```
