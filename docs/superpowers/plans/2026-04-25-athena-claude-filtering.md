# Athena Claude Code Filtering Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Clean up raw Claude Code CLI output so only the final AI response (without spinners, tool-use status lines, box drawings, and keybinding hints like "Ctrl G") is rendered in the chat UI.

**Architecture:** We will implement a two-stage filter. The first stage (already mostly complete in `src/utils/ansi.ts`) strips ANSI escape sequences. The new second stage will split the output into lines and use regex heuristics to discard CLI/UI chrome lines before adding the message to the store in `src/components/Athena/useAthena.ts`.

**Tech Stack:** React, TypeScript, Regular Expressions

---

### Task 1: Fix `ansi.ts` regex typo

**Files:**

- Modify: `src/utils/ansi.ts`

- [ ] **Step 1: Fix the OSC regex typo**

Find the OSC pattern in `src/utils/ansi.ts` and fix the typo. The pattern `(?:\\x1b\\]|x1b\\])` should have `\\x1b` correctly escaped for both options or consolidated.

```typescript
// Look for the oscPattern definition and update it
const oscPattern = new RegExp(
  // before: '(?:\\x1b\\]|x1b\\])[^\x07\x1b]*?(?:\x07|\\x1b\\\\)',
  // after:
  '(?:\\x1b\\]|\\x1b\\])[^\x07\x1b]*?(?:\x07|\\x1b\\\\)',
  'g',
)
```

### Task 2: Create a Claude output filtering utility

**Files:**

- Create: `src/utils/claudeOutputParser.ts`
- Test: `test-claude-parser.js`

- [ ] **Step 1: Write the filtering logic**

Create the new file `src/utils/claudeOutputParser.ts`:

```typescript
export function extractAiResponse(rawText: string): string {
  if (!rawText) return ''

  const lines = rawText.split('\n')
  const extractedLines: string[] = []

  const UI_CHROME_PATTERNS = [
    /^[╭╰│╞╟╠─━┄┅]+/, // Box-drawing borders
    /Ctrl\s+[A-Za-z]/i, // Keybinding hints like Ctrl G
    /to edit in/i, // NeoVim hints
    /esc to interrupt/i, // Interrupt hints
    /tokens?[:\s\d]/i, // Token usage
    /cost[:$\s]/i, // Cost info
    /^[⠋-⡇]/, // Braille spinner block
    /^\s*(✓|✗|⚙|⠿)\s/, // Status icons
    /^\s*>\s*$/, // Bare prompt echo
  ]

  for (const line of lines) {
    const isChrome = UI_CHROME_PATTERNS.some((pattern) => pattern.test(line))
    if (!isChrome) {
      extractedLines.push(line)
    }
  }

  return extractedLines.join('\n').trim()
}
```

- [ ] **Step 2: Create a test script**

Create `test-claude-parser.js` in the root:

```javascript
import { extractAiResponse } from './src/utils/claudeOutputParser.ts';

const raw = \`
╭─
│ ⚙ Thinking...
│ Ctrl G to edit in NeoVim
│ This is the actual AI response we want.
│ tokens: 154
╰─
cost: $0.05
\`;

console.log(extractAiResponse(raw));
```

- [ ] **Step 3: Run the test to verify**
      Run `bun run test-claude-parser.js` (or `node`) and ensure the output is exactly "This is the actual AI response we want."

### Task 3: Integrate filtering into the Chat UI flow

**Files:**

- Modify: `src/components/Athena/useAthena.ts`

- [ ] **Step 1: Import the parser**
      Add the import at the top of the file:

```typescript
import { extractAiResponse } from '../../utils/claudeOutputParser'
```

- [ ] **Step 2: Apply the parser before adding the message**
      Find where the debounce timeout flushes the buffer (around lines 60-80). Apply the parser to the cleaned chunk before calling `addMessage`.

```typescript
// In the debounce flush function:
const cleanMsg = stripAnsi(bufferRef.current).trim()

// Add extraction here:
const finalResponse = extractAiResponse(cleanMsg)

// Update block to use finalResponse
if (finalResponse && !isPromptOnly(finalResponse)) {
  addMessage({
    id: currentMessageIdRef.current,
    role: 'athena',
    content: finalResponse,
    timestamp: Date.now(),
  })
}
```

- [ ] **Step 3: Verify the UI state**
      Run the app and test prompting Athena. Verify that only the AI response text is visible in the chat bubble.
