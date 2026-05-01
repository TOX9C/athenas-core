# Custom Agent Duplication Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix the UI race condition where multiple custom agents with the same command overwrite and duplicate each other by shifting from value-based (command) to identity-based (ID) tracking.

**Architecture:** Extend the `PaneConfig` interface to include a `customAgentId` property. Update `NewSpaceModal.tsx` so that its internal array array tracking, `count` calculation, `addPaneAgent`, and `removePaneAgent` functions use this unique `customAgentId` instead of the `customCmd` string.

**Tech Stack:** React, TypeScript, Zustand

---

### Task 1: Update Types

**Files:**

- Modify: `src/types/workspace.ts:7-14`

- [ ] **Step 1: Add customAgentId to PaneConfig**

```typescript
export interface PaneConfig {
  id: string
  agentType: AgentType
  customCmd?: string
  customAgentId?: string
  label?: string
  bypassMode?: boolean
}
```

- [ ] **Step 2: Commit**

```bash
git add src/types/workspace.ts
git commit -m "fix(types): add customAgentId to PaneConfig for stable agent tracking"
```

### Task 2: Fix Modal Counting and Mapping Logic

**Files:**

- Modify: `src/components/Workspace/NewSpaceModal.tsx`

- [ ] **Step 1: Update the paneAgents state type**

Modify the `useState` hook around line 54:

```typescript
const [paneAgents, setPaneAgents] = useState<
  { agentType: AgentType; customCmd?: string; customAgentId?: string }[]
>([])
```

- [ ] **Step 2: Update the count calculation mapping**

Find the `const count = ...` definition mapping `AGENT_TYPES` inside the modal body:

```typescript
const isCustomStoreAgent = customAgents.some((a) => a.id === (type as string))
const storeAgent = customAgents.find((a) => a.id === (type as string))
const count = paneAgents.filter(
  (p) =>
    p.agentType === type ||
    (isCustomStoreAgent && p.agentType === 'custom' && p.customAgentId === storeAgent?.id),
).length
```

- [ ] **Step 3: Update `addPaneAgent` reducer**

```typescript
const addPaneAgent = (type: AgentType | string) => {
  if (paneAgents.length >= 16) return

  // Check if handling custom store agent
  const storeAgent = customAgents.find((a) => a.id === type)

  const newAgents = [
    ...paneAgents,
    storeAgent
      ? {
          agentType: 'custom' as AgentType,
          customCmd: storeAgent.command,
          customAgentId: storeAgent.id,
        }
      : { agentType: type as AgentType },
  ]
  setPaneAgents(newAgents)
  setGrid(gridForPaneCount(newAgents.length))
}
```

- [ ] **Step 4: Update `removePaneAgent` reducer**

```typescript
const removePaneAgent = (type: AgentType | string) => {
  const storeAgent = customAgents.find((a) => a.id === type)

  const idx = [...paneAgents]
    .reverse()
    .findIndex((p) =>
      storeAgent
        ? p.agentType === 'custom' && p.customAgentId === storeAgent.id
        : p.agentType === type,
    )
  if (idx === -1) return
  const realIdx = paneAgents.length - 1 - idx
  const newAgents = paneAgents.filter((_, i) => i !== realIdx)
  setPaneAgents(newAgents)
  setGrid(gridForPaneCount(newAgents.length))
}
```

- [ ] **Step 5: Update `handleLaunchTerminal` to pass customAgentId**

```typescript
  const handleLaunchTerminal = () => {
    if (!dir.trim()) return

    const panes: PaneConfig[] = paneAgents.map((pa) => ({
      id: nanoid(),
      agentType: pa.agentType,
      customCmd: pa.customCmd,
      customAgentId: pa.customAgentId,
    }))
// ... existing state creation
```

- [ ] **Step 6: Commit**

```bash
git add src/components/Workspace/NewSpaceModal.tsx
git commit -m "fix(workspace): map custom agents using unique customAgentId instead of command string"
```
