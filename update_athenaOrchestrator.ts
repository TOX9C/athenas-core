import { readFileSync, writeFileSync } from 'fs'

const path = 'electron/athenaOrchestrator.ts'
let content = readFileSync(path, 'utf-8')

// Add imports
content = content.replace(
  "import OpenAI from 'openai'",
  "import OpenAI from 'openai'\nimport { BrowserWindow } from 'electron'"
)

// Add emitStatus
content = content.replace(
  "export class AthenaOrchestrator {",
  `function emitStatus(
  win: BrowserWindow | null,
  status: "thinking" | "tool_dispatched" | "tool_done" | "failed" | "complete",
  meta?: Record<string, unknown>
) {
  if (win) {
    win.webContents.send("athena:status", {
      status,
      timestamp: Date.now(),
      ...meta,
    });
  }
}

export class AthenaOrchestrator {`
)

// Update sendOpenAI
content = content.replace(
  "    try {\n      while (true) {",
  `    const win = BrowserWindow.getAllWindows()[0] ?? null
    
    try {
      while (true) {
        emitStatus(win, "thinking");`
)

content = content.replace(
  `        const toolMessages = await Promise.all(
          toolCalls.map(async (toolCall) => {
            const args: ToolInput = JSON.parse(toolCall.function.arguments)
            const result = executeToolCall(toolCall.function.name, args)
            return {
              role: 'tool' as const,
              tool_call_id: toolCall.id,
              content: result.text
            }
          })
        )`,
  `        const toolMessages = await Promise.all(
          toolCalls.map(async (toolCall) => {
            const args: ToolInput = JSON.parse(toolCall.function.arguments)
            emitStatus(win, "tool_dispatched", { tool: toolCall.function.name })
            const result = await executeToolCall(toolCall.function.name, args)
            emitStatus(win, "tool_done", { tool: toolCall.function.name, success: true })
            return {
              role: 'tool' as const,
              tool_call_id: toolCall.id,
              content: result.text
            }
          })
        )`
)

content = content.replace(
  `        if (!choice.message.tool_calls || choice.message.tool_calls.length === 0) {
          return (choice.message.content || '').trim()
        }`,
  `        if (!choice.message.tool_calls || choice.message.tool_calls.length === 0) {
          emitStatus(win, "complete");
          return (choice.message.content || '').trim()
        }`
)

content = content.replace(
  `    } catch (error: unknown) {
      this.openaiMessages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      return \`Error calling provider: \${msg}\`
    }`,
  `    } catch (error: unknown) {
      this.openaiMessages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      emitStatus(win, "failed", { error: msg });
      return \`Error calling provider: \${msg}\`
    }`
)

// Update sendAnthropic
content = content.replace(
  "    try {\n      while (true) {",
  `    const win = BrowserWindow.getAllWindows()[0] ?? null
    
    try {
      while (true) {
        emitStatus(win, "thinking");`
)

content = content.replace(
  `        const toolResultContents = await Promise.all(
          toolUseBlocks.map(async (block) => {
            const result = executeToolCall(block.name, block.input as ToolInput)
            return {
              type: 'tool_result' as const,
              tool_use_id: block.id,
              content: result.text
            }
          })
        )`,
  `        const toolResultContents = await Promise.all(
          toolUseBlocks.map(async (block) => {
            emitStatus(win, "tool_dispatched", { tool: block.name })
            const result = await executeToolCall(block.name, block.input as ToolInput)
            emitStatus(win, "tool_done", { tool: block.name, success: true })
            return {
              type: 'tool_result' as const,
              tool_use_id: block.id,
              content: result.text
            }
          })
        )`
)

content = content.replace(
  `        if (toolUseBlocks.length === 0) {
          let responseText = ''
          for (const block of response.content) {
            if (block.type === 'text') {
              responseText += block.text
            }
          }
          return responseText.trim()
        }`,
  `        if (toolUseBlocks.length === 0) {
          let responseText = ''
          for (const block of response.content) {
            if (block.type === 'text') {
              responseText += block.text
            }
          }
          emitStatus(win, "complete");
          return responseText.trim()
        }`
)

content = content.replace(
  `    } catch (error: unknown) {
      this.messages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      return \`Error calling Anthropic: \${msg}\`
    }`,
  `    } catch (error: unknown) {
      this.messages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      emitStatus(win, "failed", { error: msg });
      return \`Error calling Anthropic: \${msg}\`
    }`
)

writeFileSync(path, content)
