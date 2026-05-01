# Athena Orchestrator — Fix Plan for Claude Code

## Overview

You are fixing 5 bugs in an Electron app that contains an AI orchestrator called Athena. Athena lets the user control multiple agents through a chat interface. The relevant files are:

- `electron/athenaOrchestrator.ts` — handles communication with Anthropic/OpenAI APIs
- `electron/toolExecutor.ts` — executes tool calls and sends IPC messages to the frontend
- The frontend chat input component (locate it by searching for the Athena input field)

Read all three files fully before making any changes. Do not make assumptions about variable names, types, or structure — derive everything from the actual source code.

---

## Bug 1 — Missing Orchestration Loop

### Problem

`sendAnthropic` and `sendOpenAI` execute the first set of tool calls returned by the LLM, then immediately return `responseText` and terminate. They never feed the tool results back to the LLM, so the AI cannot chain multiple actions or react to tool outcomes.

### Fix

Wrap the API call and tool execution logic in a `while (true)` loop inside both `sendAnthropic` and `sendOpenAI`.

- On each iteration, call the LLM with the current message history.
- If the response contains tool calls, execute them, append the results to the message history, and continue the loop.
- If the response contains no tool calls (text-only), capture the final text, break the loop, and return it.
- Do not return early inside the loop.

### Pseudocode

```typescript
while (true) {
  const response = await llm.createMessage(this.messages)

  const toolCalls = extractToolCalls(response)

  if (toolCalls.length === 0) {
    return extractText(response) // final answer, exit loop
  }

  const toolResults = await executeAllToolCalls(toolCalls) // see Bug 2
  this.messages.push({ role: 'assistant', content: response.content })
  this.messages.push({ role: 'user', content: toolResults })
  // loop continues
}
```

---

## Bug 2 — Malformed Anthropic Tool Results (Parallel Calls)

### Problem

In `sendAnthropic`, a new `user` message is pushed to `this.messages` inside the `for` loop for each individual `tool_result`. Anthropic's API requires all tool results from a single assistant turn to be grouped into the `content` array of a **single** `user` message. Sending multiple consecutive `user` messages causes dropped context and API validation errors.

### Fix

- Declare a `toolResultContents = []` array **before** the loop over tool use blocks.
- Inside the loop, push each `{ type: "tool_result", tool_use_id, content }` object into `toolResultContents`.
- After the loop completes, push **one** message: `{ role: "user", content: toolResultContents }`.
- Run the tool executions with `Promise.all` so parallel tool calls execute concurrently instead of sequentially.

### Pseudocode

```typescript
const toolResultContents = await Promise.all(
  toolUseBlocks.map(async (block) => {
    const output = await executeToolCall(block.name, block.input)
    return {
      type: 'tool_result',
      tool_use_id: block.id,
      content: output,
    }
  }),
)

// ONE user message containing ALL results
this.messages.push({ role: 'user', content: toolResultContents })
```

Apply the same parallel grouping logic to `sendOpenAI`, adjusted for OpenAI's tool result message format.

---

## Bug 3 — Fire-and-Forget Race Conditions in `toolExecutor.ts`

### Problem

`executeToolCall` sends IPC events to the frontend (`win.webContents.send(...)`) and immediately returns a hardcoded string like `{ text: "Done" }` without waiting for confirmation. This causes race conditions when multiple panes are added or removed simultaneously, because Zustand state updates from concurrent operations overwrite each other.

### Fix

- Convert the relevant IPC calls to a request/response pattern using `ipcMain.handle` on the main process side and `ipcRenderer.invoke` on the frontend side.
- Alternatively, use a one-time listener pattern: send the IPC event, then await a Promise that resolves when the frontend sends back an acknowledgement event (e.g. `athena:close-panes:ack`).
- Return the actual success/failure result from the frontend instead of a hardcoded string.
- On the frontend, after handling each IPC action (spawning, closing panes, etc.), send back an ack with `{ success: boolean, error?: string }`.

### Pseudocode (main process)

```typescript
const result = await new Promise((resolve) => {
  ipcMain.once('athena:close-panes:ack', (_event, data) => resolve(data))
  win.webContents.send('athena:close-panes', { ids })
})
return result.success ? 'Panes closed successfully' : `Failed: ${result.error}`
```

Apply this pattern to every tool action that mutates frontend state (spawning agents, closing panes, etc.).

---

## Bug 4 — No Input History (Arrow Up / Arrow Down)

### Problem

The Athena chat input has no history. Pressing arrow up/down does nothing. This is standard terminal/shell UX that is expected by users.

### Fix

Locate the Athena chat input component in the frontend. Add the following behaviour:

- Maintain a `history: string[]` array in component state (or a ref).
- Maintain a `historyIndex: number | null` state value, starting as `null`.
- Maintain a `draft: string` state to save the user's unsent input when they start browsing history.
- On submit: append the submitted value to `history`, reset `historyIndex` to `null`, clear `draft`.
- On `ArrowUp` keydown:
  - If `historyIndex` is `null`, save current input to `draft` and set `historyIndex` to `history.length - 1`.
  - Else decrement `historyIndex` (min 0).
  - Set input value to `history[historyIndex]`.
- On `ArrowDown` keydown:
  - If `historyIndex` is `null`, do nothing.
  - If `historyIndex + 1 >= history.length`, reset `historyIndex` to `null` and restore `draft`.
  - Else increment `historyIndex` and set input to `history[historyIndex]`.
- Persist history to `localStorage` or Electron's `store` so it survives app restarts. Cap it at the last 100 entries.

---

## Bug 5 — No Agent Status Feedback

### Problem

When Athena is working, the UI shows nothing. The user cannot tell if the AI is thinking, if a tool call was dispatched, if something failed, or when the operation is complete.

### Fix

#### Backend (`athenaOrchestrator.ts` and `toolExecutor.ts`)

Add a helper function that emits a status event to the frontend at key points during execution:

```typescript
function emitStatus(
  win: BrowserWindow,
  status: 'thinking' | 'tool_dispatched' | 'tool_done' | 'failed' | 'complete',
  meta?: Record<string, unknown>,
) {
  win.webContents.send('athena:status', {
    status,
    timestamp: Date.now(),
    ...meta,
  })
}
```

Call `emitStatus` at these points:

- Before each LLM API call: `emitStatus(win, "thinking")`
- Before each tool execution: `emitStatus(win, "tool_dispatched", { tool: block.name })`
- After each tool execution: `emitStatus(win, "tool_done", { tool: block.name, success: true })`
- On any caught error: `emitStatus(win, "failed", { error: err.message })`
- When the loop exits with a final response: `emitStatus(win, "complete")`

#### Frontend

- Subscribe to `athena:status` IPC events.
- Maintain a `statusLog: StatusEvent[]` array in state.
- Render a status area below or above the chat input that shows the live log. Each entry should clearly show what is happening, for example:
  - 🟡 Athena is thinking...
  - 🔵 Dispatching tool: `close_agent`
  - ✅ Tool complete: `close_agent`
  - 🔴 Failed: timeout
  - ✅ Done
- Clear or collapse the log when a new user message is submitted.

---

## Implementation Order

Work through the bugs in this order. Commit or checkpoint after each one so regressions are easy to identify.

1. **Bug 4** — Input history (isolated frontend change, no risk to orchestrator logic)
2. **Bug 2** — Fix tool result grouping in `sendAnthropic` (prerequisite for Bug 1 to work correctly)
3. **Bug 1** — Add the orchestration loop to both `sendAnthropic` and `sendOpenAI`
4. **Bug 5** — Add status event pipeline (helps debug Bug 3)
5. **Bug 3** — Make IPC awaitable in `toolExecutor.ts`

---

## Testing Checklist

After all fixes are applied, verify the following manually:

- [ ] Arrow up recalls the last sent message; arrow up again goes further back; arrow down returns toward the draft.
- [ ] History persists after restarting the app.
- [ ] "Close agent 1, agent 2, and agent 3" closes all three, not just one.
- [ ] "Spawn two agents and then close them both" executes all steps in the correct order.
- [ ] The status log updates in real time during multi-step operations.
- [ ] A failed tool call shows an error in the status log and does not silently succeed.
- [ ] No race conditions when spawning or closing multiple panes simultaneously (check Zustand state in devtools).
