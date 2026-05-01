# Athena Multi-Provider Orchestrator Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Modify the Athena orchestrator to safely launch without crashing when an API key is missing. Add support for switching between the Anthropic API and the NVIDIA NIM API, and provide a frontend UI in the Settings panel to configure the active provider and its API key.

**Architecture:**

1. **Lazy Initialization:** Make `AthenaOrchestrator` defer SDK instantiation until `sendMessage` is actually invoked.
2. **Provider Routing:** Read the provider configuration (`athena.provider` and `athena.apiKey`) dynamically from `electron-store`.
3. **NVIDIA NIM Support:** Use the `openai` SDK to connect to NVIDIA NIM's API (`https://integrate.api.nvidia.com/v1`), utilizing an OpenAI-compatible interface for tool use.
4. **Settings UI:** Expand `Settings` with a new "Athena Configuration" tab/section.

**Tech Stack:** React, Electron, `electron-store`, `@anthropic-ai/sdk`, `openai` SDK

---

### Task 1: Fix Startup Crash & Architecture Preparation

**Files:**

- Modify: `package.json`
- Modify: `electron/athenaOrchestrator.ts`

- [ ] **Step 1: Install OpenAI SDK**

```bash
npm install openai
```

- [ ] **Step 2: Remove eager initialization in Orchestrator**
      Modify `electron/athenaOrchestrator.ts`. Remove the global `const anthropic = new Anthropic({ apiKey })`.
      Change `sendMessage` so that it expects to initialize the API client _inside_ the function, retrieving the API key safely first. Give it a helper to grab the key from `electron-store` safely via IPC or directly from the `getStore` helper in `main.ts` (you will need to export `getStore` from `main.ts` or refactor it into a separate utility file).

### Task 2: Implement Multi-Provider API Logic

**Files:**

- Modify: `electron/athenaOrchestrator.ts`
- Modify: `electron/storeUtil.ts` (create a central store fetcher)

- [ ] **Step 1: Create a unified Store Utility**
      Extract the `getStore` logic from `main.ts` into a `electron/storeUtil.ts` to cleanly share it with `athenaOrchestrator.ts`.

- [ ] **Step 2: Add NVIDIA NIM / Anthropic Routing**
      In `AthenaOrchestrator.sendMessage()`:

```typescript
const store = await getStore()
const provider = store.get('athena.provider') || 'anthropic'
const apiKey = store.get('athena.apiKey')

if (!apiKey) {
  return 'Error: API Key is required. Please set it in Settings.'
}

if (provider === 'nvidia_nim') {
  // Use OpenAI SDK directed at NIM
  // baseURL: 'https://integrate.api.nvidia.com/v1'
  // Translate system prompt, messages, and tools accordingly.
} else {
  // Default to Anthropic SDK
}
```

### Task 3: Backend Settings Manager IPC Updates

**Files:**

- Modify: `electron/main.ts`

- [ ] **Step 1: Refactor `electron-store` logic**
      Update `<ipcMain.handle('store:get')>` to use the newly extracted `storeUtil.ts` helper.

### Task 4: Frontend Settings UI

**Files:**

- Modify: `src/components/Settings/Settings.tsx` (or appropriate file based on folder `src/components/Settings`)

- [ ] **Step 1: Add Athena Settings Tab/Section**
      Add a configuration area for Athena inside the Settings Component.
- Dropdown: `Provider` (Options: "Anthropic API", "NVIDIA NIM")
- Password Input: `API Key`

- [ ] **Step 2: Bind Settings to IPC**
      Use `window.electron.ipcRenderer.invoke('store:get', 'athena.provider')` to populate defaults on mount.
      Use `window.electron.ipcRenderer.invoke('store:set', 'athena.apiKey', val)` when the input changes.

### Task 5: Testing & Verification

- [ ] **Step 1:** Verify the app boots cleanly without any environment variables.
- [ ] **Step 2:** Provide a dummy key in Settings and verify the backend catches it during message send.
