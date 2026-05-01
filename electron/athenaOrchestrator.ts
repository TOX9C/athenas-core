import Anthropic from '@anthropic-ai/sdk'
import OpenAI from 'openai'
import { BrowserWindow } from 'electron'
import { getStore } from './storeUtil'
import { ORCHESTRATOR_TOOLS, toOpenAITools, executeToolCall, type ToolInput } from './toolExecutor'

function parseTextToolCall(text: string): { name: string; args: Record<string, any> } | null {
  if (!text) return null

  const xmlMatch =
    text.match(/<tool_call>([\s\S]*?)<\/tool_call>/i) || text.match(/<([\w_]+)>\s*<arg_key>/i)
  if (!xmlMatch) {
    const jsonMatch = text.match(
      /\{\s*"name"\s*:\s*"([\w_]+)"\s*,\s*"arguments"\s*:\s*(\{[\s\S]*\})\s*\}/,
    )
    if (jsonMatch) {
      try {
        return { name: jsonMatch[1], args: JSON.parse(jsonMatch[2]) }
      } catch {
        return null
      }
    }
    return null
  }

  const fullText = text

  const toolNameMatch =
    fullText.match(/<tool_call>\s*([\w_]+)/) || fullText.match(/^<([\w_]+)>\s*<arg_key>/)
  if (!toolNameMatch) return null
  const toolName = toolNameMatch[1]

  const validTools = ORCHESTRATOR_TOOLS.map((t) => t.name)
  if (!validTools.includes(toolName)) return null

  const args: Record<string, any> = {}

  const argKeyPattern = /<arg_key>([\w_]+)<\/arg_key>\s*<arg_value>([\s\S]*?)<\/arg_value>/gi
  let argMatch
  while ((argMatch = argKeyPattern.exec(fullText)) !== null) {
    const key = argMatch[1]
    let value: any = argMatch[2]
    try {
      value = JSON.parse(value)
    } catch {}
    args[key] = value
  }

  if (Object.keys(args).length === 0) return null

  return { name: toolName, args }
}

function buildSystemPrompt(
  spaces: any[],
  tasks: any[],
  customAgents: any[],
  activePanes: any[],
): string {
  return `You are the Athena Orchestrator — an intelligent team lead built into an Electron IDE.

PROJECT CONTEXT:
- Active Workspaces: ${JSON.stringify(spaces.map((s: any) => ({ name: s.name, dir: s.dir })))}
- Current Tasks: ${JSON.stringify(tasks)}
- Custom Agents: ${JSON.stringify(customAgents)}
- Running Terminals/Panes: ${JSON.stringify(activePanes)}

RESPONSE STYLE — CRITICAL:
- After tool calls: respond in 1 sentence MAX. Example: "Done, launched 3 agents." or "All 6 agents prompted."
- NEVER write paragraphs after launching/prompting agents unless the user explicitly asks for details.
- NEVER summarize what each agent was told. NEVER list what you did step by step. Just confirm the action.
- Only give detailed responses when the user asks a question that requires explanation.

TASK DISTRIBUTION:
- When the user says to launch/prompt multiple agents with a task (e.g., "analyze the codebase"), give ALL agents the SAME prompt — the user's exact instruction. Do NOT invent your own splitting strategy.
- Only split tasks into distinct sub-prompts if the user EXPLICITLY tells you how to split (e.g., "one does backend, one does frontend").
- If you think splitting would help but the user didn't specify, use 'ask_user' to propose options. Do NOT assume.

AGENT LAUNCHING:
- Built-in agents (claude, codex, opencode, gemini, shell) → 'launch_builtin_agent'
- Custom agents from the list above → 'launch_custom_agent' with the command from the config
- If the user asks to launch without a task, leave task_prompt empty. Do NOT ask for a prompt.

PLANNING (only for complex multi-step workflows):
- Use 'create_execution_plan' when the user describes a multi-step goal with dependencies between steps.
- Use 'dispatch_plan_step' to launch agents per step.
- Do NOT create plans for simple "launch and prompt" requests.

MONITORING & INTERACTION:
- 'list_agents' — see all running panes and sessions
- 'read_agent_output' — read terminal output from a specific pane
- 'check_agent_status' — check if an agent is active, idle, or waiting
- 'prompt_agent' — send follow-up instructions to a running agent
- 'ask_user' — ask the user a question with clickable options
- 'close_terminals' — remove panes

CLARIFICATION:
- When the user's request is ambiguous, use 'ask_user' with structured options. Do NOT guess.`
}

function emitStatus(
  win: BrowserWindow | null,
  status: 'thinking' | 'tool_dispatched' | 'tool_done' | 'failed' | 'complete',
  meta?: Record<string, unknown>,
) {
  if (win) {
    win.webContents.send('athena:status', {
      status,
      timestamp: Date.now(),
      ...meta,
    })
  }
}

export interface ImageData {
  base64: string
  mediaType: 'image/jpeg' | 'image/png' | 'image/gif' | 'image/webp'
}

type AnthropicContent = string | Anthropic.ContentBlockParam[]
type OpenAIContent = string | OpenAI.ChatCompletionContentPart[]

function compactAnthropicMessages(
  messages: Anthropic.MessageParam[],
  keepRecent: number,
): Anthropic.MessageParam[] {
  if (messages.length <= keepRecent) return messages

  let compacted = messages.slice(-keepRecent)

  const firstUserIdx = compacted.findIndex((m) => m.role === 'user')
  if (firstUserIdx > 0) {
    compacted = compacted.slice(firstUserIdx)
  } else if (firstUserIdx === -1) {
    return messages
  }

  return [
    {
      role: 'user' as const,
      content:
        '[System: Earlier messages in this conversation were compacted to save context. Continue from the most recent context.]',
    },
    { role: 'assistant' as const, content: 'Understood, continuing from recent context.' },
    ...compacted,
  ]
}

function compactOpenAIMessages(
  messages: OpenAI.ChatCompletionMessageParam[],
  keepRecent: number,
): OpenAI.ChatCompletionMessageParam[] {
  if (messages.length <= keepRecent) return messages

  const systemMsg = messages[0]?.role === 'system' ? messages[0] : null
  const nonSystem = systemMsg ? messages.slice(1) : messages
  let compacted = nonSystem.slice(-keepRecent)

  while (compacted.length > 0 && compacted[0].role === 'tool') {
    compacted = compacted.slice(1)
  }

  if (compacted.length > 0 && compacted[0].role === 'assistant') {
    const firstAssistant = compacted[0] as any
    if (firstAssistant.tool_calls && firstAssistant.tool_calls.length > 0) {
      compacted = compacted.slice(1)
      while (compacted.length > 0 && compacted[0].role === 'tool') {
        compacted = compacted.slice(1)
      }
    }
  }

  const prefix: OpenAI.ChatCompletionMessageParam[] = systemMsg ? [systemMsg] : []
  return [...prefix, ...compacted]
}

function stripOldImages(
  messages: Anthropic.MessageParam[],
  preserveLastN: number = 4,
): Anthropic.MessageParam[] {
  const cutoff = messages.length - preserveLastN
  return messages.map((msg, idx) => {
    if (idx >= cutoff) return msg
    if (!Array.isArray(msg.content)) return msg

    const hasImage = (msg.content as any[]).some((b: any) => b.type === 'image')
    if (!hasImage) return msg

    const newContent = (msg.content as any[]).map((block: any) => {
      if (block.type === 'image') {
        return { type: 'text' as const, text: '[image was attached]' }
      }
      return block
    })
    return { ...msg, content: newContent }
  })
}

function stripOldImagesOpenAI(
  messages: OpenAI.ChatCompletionMessageParam[],
  preserveLastN: number = 4,
): OpenAI.ChatCompletionMessageParam[] {
  const cutoff = messages.length - preserveLastN
  return messages.map((msg, idx) => {
    if (idx >= cutoff) return msg
    if (msg.role !== 'user' || !Array.isArray(msg.content)) return msg

    const hasImage = (msg.content as any[]).some((b: any) => b.type === 'image_url')
    if (!hasImage) return msg

    const newContent = (msg.content as any[]).map((block: any) => {
      if (block.type === 'image_url') {
        return { type: 'text' as const, text: '[image was attached]' }
      }
      return block
    })
    return { ...msg, content: newContent } as OpenAI.ChatCompletionMessageParam
  })
}

function buildAnthropicContent(text: string, images?: ImageData[]): AnthropicContent {
  if (!images || images.length === 0) return text
  const blocks: Anthropic.ContentBlockParam[] = images.map((img) => ({
    type: 'image' as const,
    source: { type: 'base64' as const, media_type: img.mediaType, data: img.base64 },
  }))
  blocks.push({ type: 'text' as const, text })
  return blocks
}

function buildOpenAIContent(text: string, images?: ImageData[]): OpenAIContent {
  if (!images || images.length === 0) return text
  const parts: OpenAI.ChatCompletionContentPart[] = images.map((img) => ({
    type: 'image_url' as const,
    image_url: { url: `data:${img.mediaType};base64,${img.base64}` },
  }))
  parts.push({ type: 'text' as const, text })
  return parts
}

export interface SessionHistoryEntry {
  role: 'user' | 'assistant'
  content: string
  images?: ImageData[]
}

export class AthenaOrchestrator {
  private anthropic?: Anthropic
  private openai?: OpenAI
  private messages: Anthropic.MessageParam[] = []
  private openaiMessages: OpenAI.ChatCompletionMessageParam[] = []
  private currentSessionId?: string

  setSessionContext(history: SessionHistoryEntry[]): void {
    this.messages = history.map((m) => {
      const role = m.role as 'user' | 'assistant'
      const content = buildAnthropicContent(m.content, m.images)
      return { role, content } as Anthropic.MessageParam
    })
    this.openaiMessages = history.map((m) => {
      const role = m.role as 'user' | 'assistant'
      const content = buildOpenAIContent(m.content, m.images)
      return { role, content } as OpenAI.ChatCompletionMessageParam
    })
  }

  clearContext(): void {
    this.messages = []
    this.openaiMessages = []
    this.currentSessionId = undefined
  }

  setCurrentSessionId(id: string | undefined): void {
    this.currentSessionId = id
  }

  getCurrentSessionId(): string | undefined {
    return this.currentSessionId
  }

  async sendMessage(userText: string, images?: ImageData[]): Promise<string> {
    const store = await getStore()
    const provider = (store.get('athena.provider') || 'anthropic') as string
    const apiKey = store.get('athena.apiKey') as string | undefined
    const model = (store.get('athena-model') ||
      (provider === 'nvidia_nim'
        ? 'minimaxai/minimax-text-01'
        : 'claude-sonnet-4-20250514')) as string

    if (!apiKey) {
      return 'Error: API Key is required. Please set it in Settings.'
    }

    if (images && images.length > 0 && provider === 'lmstudio') {
      return 'Error: Image attachments are not supported by LM Studio. Only Anthropic, OpenAI, and NVIDIA NIM support vision.'
    }

    const customAgents = (store.get('athena-customAgents') || []) as any[]
    const spaces = (store.get('spaces') || []) as any[]
    const tasks = (store.get('tasks') || []) as any[]

    const activePanes = spaces.flatMap(
      (s: any) =>
        s.panes?.map((p: any) => ({
          id: p.id,
          spaceId: s.id,
          spaceName: s.name,
          type: p.agentType,
          isShell: p.agentType === 'shell',
        })) || [],
    )

    const systemPrompt = buildSystemPrompt(spaces, tasks, customAgents, activePanes)

    if (provider === 'nvidia_nim' || provider === 'openai' || provider === 'lmstudio') {
      let baseURL: string | undefined
      if (provider === 'nvidia_nim') baseURL = 'https://integrate.api.nvidia.com/v1'
      if (provider === 'lmstudio')
        baseURL = (store.get('athena.lmstudioBaseUrl') as string) || 'http://localhost:1234/v1'
      return this.sendOpenAI(apiKey, model, systemPrompt, userText, images, baseURL)
    }
    return this.sendAnthropic(apiKey, model, systemPrompt, userText, images)
  }

  private async sendOpenAI(
    apiKey: string,
    model: string,
    systemPrompt: string,
    userText: string,
    images: ImageData[] = [],
    baseURL?: string,
  ): Promise<string> {
    if (!this.openai) {
      this.openai = new OpenAI({ apiKey, baseURL })
    }

    if (this.openaiMessages.length === 0 || this.openaiMessages[0].role !== 'system') {
      this.openaiMessages = [{ role: 'system', content: systemPrompt }, ...this.openaiMessages]
    } else {
      ;(this.openaiMessages[0] as OpenAI.ChatCompletionSystemMessageParam).content = systemPrompt
    }

    const userContent: OpenAIContent = buildOpenAIContent(
      userText,
      images.length > 0 ? images : undefined,
    )
    this.openaiMessages.push({ role: 'user', content: userContent })

    const win = BrowserWindow.getAllWindows()[0] ?? null

    let iteration = 0
    const MAX_ITERATIONS = 50
    const MAX_MESSAGES = 50
    const KEEP_RECENT = 20
    const STALL_WINDOW = 5
    const recentCallSignatures: string[] = []

    try {
      while (true) {
        if (iteration++ > MAX_ITERATIONS) {
          console.warn('Athena: hit max iterations, halting.')
          return 'Operation halted: too many steps. Please try a more specific command.'
        }
        if (iteration === 40) {
          emitStatus(win, 'thinking', { message: 'Long operation in progress...' })
        }

        if (this.openaiMessages.length > MAX_MESSAGES) {
          this.openaiMessages = compactOpenAIMessages(this.openaiMessages, KEEP_RECENT)
        }

        const processedMessages = stripOldImagesOpenAI(this.openaiMessages)

        emitStatus(win, 'thinking')
        const response = await this.openai.chat.completions.create({
          model,
          max_tokens: 4096,
          messages: processedMessages,
          tools: toOpenAITools(),
        })

        const choice = response.choices[0]
        if (!choice.message) return ''

        const rawContent = (choice.message.content || '').trim()
        const parsedToolCall = parseTextToolCall(rawContent)

        if (
          parsedToolCall &&
          (!choice.message.tool_calls || choice.message.tool_calls.length === 0)
        ) {
          const syntheticId = `call_${Date.now()}`
          choice.message.tool_calls = [
            {
              id: syntheticId,
              type: 'function' as const,
              function: {
                name: parsedToolCall.name,
                arguments: JSON.stringify(parsedToolCall.args),
              },
            },
          ]
          choice.message.content = null
        }

        this.openaiMessages.push(choice.message)

        if (!choice.message.tool_calls || choice.message.tool_calls.length === 0) {
          emitStatus(win, 'complete')
          return rawContent
        }

        const toolCalls = choice.message.tool_calls
        const toolMessages = await Promise.all(
          toolCalls.map(async (toolCall) => {
            const fn = (toolCall as any).function
            const args: ToolInput = JSON.parse(fn.arguments)
            emitStatus(win, 'tool_dispatched', { tool: fn.name })
            const result = await executeToolCall(fn.name, args)
            emitStatus(win, 'tool_done', { tool: fn.name, success: true })

            const sig = `${fn.name}:${fn.arguments}`
            recentCallSignatures.push(sig)
            if (recentCallSignatures.length > STALL_WINDOW) recentCallSignatures.shift()

            return {
              role: 'tool' as const,
              tool_call_id: toolCall.id,
              content: result.text,
            }
          }),
        )

        if (
          recentCallSignatures.length === STALL_WINDOW &&
          recentCallSignatures.every((s) => s === recentCallSignatures[0])
        ) {
          console.warn('Athena: detected stalled loop, breaking.')
          return 'I appear to be stuck in a loop. Please try rephrasing your request.'
        }

        this.openaiMessages.push(...toolMessages)
      }
    } catch (error: unknown) {
      this.openaiMessages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      emitStatus(win, 'failed', { error: msg })
      return `Error calling provider: ${msg}`
    }
  }

  private async sendAnthropic(
    apiKey: string,
    model: string,
    systemPrompt: string,
    userText: string,
    images: ImageData[] = [],
  ): Promise<string> {
    const win = BrowserWindow.getAllWindows()[0] ?? null
    if (!this.anthropic) {
      this.anthropic = new Anthropic({ apiKey })
    }

    const userContent: AnthropicContent = buildAnthropicContent(
      userText,
      images.length > 0 ? images : undefined,
    )
    this.messages.push({ role: 'user', content: userContent })

    let iteration = 0
    const MAX_ITERATIONS = 50
    const MAX_MESSAGES = 50
    const KEEP_RECENT = 20
    const STALL_WINDOW = 5
    const recentCallSignatures: string[] = []

    try {
      while (true) {
        if (iteration++ > MAX_ITERATIONS) {
          console.warn('Athena: hit max iterations, halting.')
          return 'Operation halted: too many steps. Please try a more specific command.'
        }
        if (iteration === 40) {
          emitStatus(win, 'thinking', { message: 'Long operation in progress...' })
        }

        if (this.messages.length > MAX_MESSAGES) {
          this.messages = compactAnthropicMessages(this.messages, KEEP_RECENT)
        }

        const processedMessages = stripOldImages(this.messages)

        emitStatus(win, 'thinking')

        const contentBlocks: Anthropic.ContentBlock[] = []
        let streamedText = ''

        const stream = this.anthropic.messages.stream({
          model,
          max_tokens: 4096,
          system: systemPrompt,
          messages: processedMessages,
          tools: ORCHESTRATOR_TOOLS as any,
        })

        stream.on('text', (text) => {
          streamedText += text
          if (win) {
            win.webContents.send('athena:status', {
              status: 'streaming',
              streamDelta: text,
              streamedText,
              timestamp: Date.now(),
            })
          }
        })

        const response = await stream.finalMessage()

        this.messages.push({ role: 'assistant', content: response.content })

        const toolUseBlocks = response.content.filter(
          (block): block is Anthropic.ToolUseBlock => block.type === 'tool_use',
        )

        if (toolUseBlocks.length === 0) {
          let responseText = ''
          for (const block of response.content) {
            if (block.type === 'text') {
              responseText += block.text
            }
          }
          emitStatus(win, 'complete')
          return responseText.trim()
        }

        const toolResultContents = await Promise.all(
          toolUseBlocks.map(async (block) => {
            emitStatus(win, 'tool_dispatched', { tool: block.name })
            const result = await executeToolCall(block.name, block.input as ToolInput)
            emitStatus(win, 'tool_done', { tool: block.name, success: true })

            const sig = `${block.name}:${JSON.stringify(block.input)}`
            recentCallSignatures.push(sig)
            if (recentCallSignatures.length > STALL_WINDOW) recentCallSignatures.shift()

            return {
              type: 'tool_result' as const,
              tool_use_id: block.id,
              content: result.text,
            }
          }),
        )

        if (
          recentCallSignatures.length === STALL_WINDOW &&
          recentCallSignatures.every((s) => s === recentCallSignatures[0])
        ) {
          console.warn('Athena: detected stalled loop, breaking.')
          return 'I appear to be stuck in a loop. Please try rephrasing your request.'
        }

        this.messages.push({
          role: 'user',
          content: toolResultContents,
        })
      }
    } catch (error: unknown) {
      this.messages.pop()
      const msg = error instanceof Error ? error.message : String(error)
      emitStatus(win, 'failed', { error: msg })
      return `Error calling Anthropic: ${msg}`
    }
  }
}

export const athenaOrchestrator = new AthenaOrchestrator()
