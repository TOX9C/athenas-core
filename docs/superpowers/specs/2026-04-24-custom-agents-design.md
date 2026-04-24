# Custom Agent Management Design Specification

**Goal:** Allow users to define, label, and manage multiple locally installed AI CLI agents from the "Agents" tab, and make those custom agents available globally inside dropdown selectors natively alongside hardcoded ones like "Claude".

## Architecture 

1. **State and Persistence (`src/store/athenaStore.ts` & `src/App.tsx`)**
   - We will replace the single `customCommand` string with a `customAgents: CustomAgent[]` array inside `useAthenaStore`, where `CustomAgent` is `{ id: string, name: string, command: string }`.
   - We will wire `customAgents` into `window.athena.store.get('athena-customAgents')` inside `App.tsx` on boot, and push changes to `window.athena.store.set(...)` whenever a custom agent is added/removed.

2. **The "Agents" Tab UI (`src/components/Settings/SettingsModal.tsx`)**
   - We will drop the "coming soon" placeholder.
   - We will build a small list showing existing custom agents with a Trash (`Trash2` icon) button to remove them.
   - We will add an inline form with two inputs (`Name` and `Command`) and a "Add Agent" button to push new entries to the Zustand state and Electron persistent store.

3. **The "Athena" Tab UI (`src/components/Settings/SettingsModal.tsx`)**
   - The dropdown list for "Model" will be updated. Instead of hardcoding `<option value="custom">Custom</option>`, we will map over the `customAgents` array and render a new `<option>` for each valid custom agent using its custom ID.
   - We will delete the `customCommand` textbox entirely since commands are now securely linked to specific named instances.

4. **PTY Runner (`src/components/Athena/useAthena.ts`)**
   - We will update the `getCommand()` dispatcher. Instead of looking for `model === 'custom'`, it will actively search the `customAgents` array for a matching ID and return its command string. If it's a default generic handler like `claude` or `gemini`, it returns its safe default shell string.

Does this accurately cover your request, or are there any specific nuances about the custom agents you would like me to adjust before we start building the plan out?