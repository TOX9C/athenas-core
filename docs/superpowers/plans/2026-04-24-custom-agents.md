# Custom Agents Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Allow users to manage multiple named custom agent CLI commands and render them dynamically in dropdowns.

**Architecture:** We will replace the single `customCommand` state with a `customAgents` array in the Zustand store and electron-store. `SettingsModal` will get a management UI in the Agents tab, and `useAthena.ts` will dynamically fetch the correct command.

**Tech Stack:** React, Zustand, electron-store.

---

### Task 1: Update State Store

**Files:**

- Modify: `src/store/athenaStore.ts`

- [ ] **Step 1: Add CustomAgent interface**
      Above `interface AthenaState`, add:

```typescript
export interface CustomAgent {
  id: string
  name: string
  command: string
}
```

- [ ] **Step 2: Update AthenaState and store implementation**
      Remove `customCommand` and `setCustomCommand`. Add `customAgents: CustomAgent[]`, `addCustomAgent: (agent: CustomAgent) => void`, `removeCustomAgent: (id: string) => void`.

```typescript
interface AthenaState {
  messages: AthenaMessage[]
  isOpen: boolean
  isPtyReady: boolean
  model: string
  bypassMode: boolean
  autoLaunch: boolean
  customAgents: CustomAgent[]
  addMessage: (msg: AthenaMessage) => void
  setOpen: (open: boolean) => void
  toggleOpen: () => void
  setPtyReady: (ready: boolean) => void
  setModel: (model: string) => void
  setBypassMode: (bypass: boolean) => void
  setAutoLaunch: (auto: boolean) => void
  addCustomAgent: (agent: CustomAgent) => void
  removeCustomAgent: (id: string) => void
  clearMessages: () => void
}

export const useAthenaStore = create<AthenaState>((set) => ({
  messages: [],
  isOpen: false,
  isPtyReady: false,
  model: 'claude',
  bypassMode: true,
  autoLaunch: true,
  customAgents: [],
  addMessage: (msg) => set((s) => ({ messages: [...s.messages, msg] })),
  setOpen: (open) => set({ isOpen: open }),
  toggleOpen: () => set((s) => ({ isOpen: !s.isOpen })),
  setPtyReady: (ready) => set({ isPtyReady: ready }),
  setModel: (model) => set({ model }),
  setBypassMode: (bypass) => set({ bypassMode: bypass }),
  setAutoLaunch: (auto) => set({ autoLaunch: auto }),
  addCustomAgent: (agent) => set((s) => ({ customAgents: [...s.customAgents, agent] })),
  removeCustomAgent: (id) =>
    set((s) => ({ customAgents: s.customAgents.filter((a) => a.id !== id) })),
  clearMessages: () => set({ messages: [] }),
}))
```

- [ ] **Step 3: Commit**

```bash
git add src/store/athenaStore.ts
git commit -m "feat(store): restructure athena store for multiple custom agents"
```

---

### Task 2: Hydrate Custom Agents from Electron Store

**Files:**

- Modify: `src/App.tsx`

- [ ] **Step 1: Replace customCommand hydration with customAgents**
      In `src/App.tsx`, find the `athena-customCommand` fetch and replace it completely with `athena-customAgents`:

```tsx
window.athena.store.get('athena-customAgents').then((saved: any) => {
  if (saved && Array.isArray(saved)) {
    saved.forEach((agent) => useAthenaStore.getState().addCustomAgent(agent))
  }
})
```

- [ ] **Step 2: Commit**

```bash
git add src/App.tsx
git commit -m "feat(app): hydrate custom agents array on startup"
```

---

### Task 3: Update Dropdowns and Agent Tab UI

**Files:**

- Modify: `src/components/Settings/SettingsModal.tsx`

- [ ] **Step 1: Update imports and state mappings**
      Add `Trash2`, `Plus` to the `lucide-react` imports.
      Import `nanoid` from `'nanoid'`.
      In `SettingsModal`, remove `customCommand` and `setCustomCommand`. Add `customAgents, addCustomAgent, removeCustomAgent`. Add local state for the form:

```tsx
const [newAgentName, setNewAgentName] = useState('')
const [newAgentCmd, setNewAgentCmd] = useState('')
```

- [ ] **Step 2: Implement Agents Tab**
      Replace the entire `tab === 'agents'` rendering block with:

```tsx
{
  tab === 'agents' && (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col gap-2">
        {customAgents.map((ag) => (
          <div
            key={ag.id}
            className="flex items-center justify-between p-2 rounded border"
            style={{ borderColor: 'var(--border)', background: 'var(--bg)' }}
          >
            <div className="flex flex-col">
              <span className="text-sm" style={{ color: 'var(--text)' }}>
                {ag.name}
              </span>
              <span className="text-xs font-mono" style={{ color: 'var(--textMuted)' }}>
                {ag.command}
              </span>
            </div>
            <button
              onClick={() => {
                removeCustomAgent(ag.id)
                window.athena.store.set(
                  'athena-customAgents',
                  useAthenaStore.getState().customAgents.filter((a) => a.id !== ag.id),
                )
              }}
              className="p-1 hover:text-red-500 transition-colors"
            >
              <Trash2 size={16} />
            </button>
          </div>
        ))}
        {customAgents.length === 0 && (
          <div className="text-xs italic" style={{ color: 'var(--textMuted)' }}>
            No custom agents configured.
          </div>
        )}
      </div>
      <div
        className="flex items-end gap-2 p-3 rounded bg-black/20"
        style={{ border: '1px solid var(--border)' }}
      >
        <div className="flex-1 flex flex-col gap-1">
          <span className="text-xs" style={{ color: 'var(--textMuted)' }}>
            Agent Name
          </span>
          <input
            value={newAgentName}
            onChange={(e) => setNewAgentName(e.target.value)}
            placeholder="My Super Agent"
            className="px-2 py-1 flex-1 rounded text-xs outline-none bg-transparent"
            style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
          />
        </div>
        <div className="flex-1 flex flex-col gap-1">
          <span className="text-xs" style={{ color: 'var(--textMuted)' }}>
            CLI Command
          </span>
          <input
            value={newAgentCmd}
            onChange={(e) => setNewAgentCmd(e.target.value)}
            placeholder="my-agent-cli --flag"
            className="px-2 py-1 flex-1 rounded text-xs outline-none bg-transparent"
            style={{ border: '1px solid var(--border)', color: 'var(--text)' }}
          />
        </div>
        <button
          onClick={() => {
            if (!newAgentName || !newAgentCmd) return
            const newAgent = { id: nanoid(), name: newAgentName, command: newAgentCmd }
            addCustomAgent(newAgent)
            window.athena.store.set('athena-customAgents', [...customAgents, newAgent])
            setNewAgentName('')
            setNewAgentCmd('')
          }}
          className="px-3 py-1 flex items-center justify-center rounded text-xs transition-colors h-6"
          style={{ background: 'var(--accent)', color: '#fff' }}
        >
          <Plus size={14} className="mr-1" /> Add
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 3: Update Athena Tab Models Dropdown**
      In the `tab === 'athena'` block, remove the dynamic conditional rendering containing the `handleCustomCmdChange` text box.
      Update the `model` `<select>` options to dynamically map the custom keys:

```tsx
                    <option value="claude">Claude Code</option>
                    <option value="codex">Codex</option>
                    <option value="opencode">OpenCode</option>
                    <option value="gemini">Gemini CLI</option>
                    <option disabled>──────────</option>
                    {customAgents.map(ag => (
                      <option key={ag.id} value={ag.id}>{ag.name}</option>
                    ))}
```

- [ ] **Step 4: Commit**

```bash
git add src/components/Settings/SettingsModal.tsx
git commit -m "feat(settings): agent management UI and dynamic model dropdown"
```

---

### Task 4: Integrate with PTY Runner

**Files:**

- Modify: `src/components/Athena/useAthena.ts`

- [ ] **Step 1: Replace customCommand with customAgents in useAthena.ts**
      Remove `customCommand` from `useAthenaStore()` destructuring. Add `customAgents`.
      Update `getCommand` to dynamically fall back to the array lookup:

```tsx
const getCommand = useCallback(() => {
  switch (model) {
    case 'claude':
      return bypassMode ? 'claude --dangerously-skip-permissions' : 'claude'
    case 'codex':
      return 'codex'
    case 'opencode':
      return 'opencode'
    case 'gemini':
      return 'gemini'
    default: {
      const custom = customAgents.find((a) => a.id === model)
      if (custom) return custom.command
      return 'claude'
    }
  }
}, [model, bypassMode, customAgents])
```

Update the bottom `useEffect` dependency array (the watcher triggering `spawnAthena()`) to sync to `customAgents`:

```tsx
// Re-spawn the underlying agent PTY if the user modifies CLI parameters while it's running
useEffect(() => {
  if (isPtyReady && activeSpace) {
    spawnAthena()
  }
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [model, bypassMode, customAgents])
```

- [ ] **Step 2: Commit**

```bash
git add src/components/Athena/useAthena.ts
git commit -m "fix(athena): use dynamic custom agents for command execution"
```
