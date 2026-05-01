import { readFileSync, writeFileSync } from 'fs'

const path = 'electron/toolExecutor.ts'
let content = readFileSync(path, 'utf-8')

content = content.replace(
  "import { randomUUID } from 'node:crypto'",
  "import { randomUUID } from 'node:crypto'\nimport { ipcMain } from 'electron'"
)

content = content.replace(
  "export function executeToolCall(name: string, args: ToolInput): ToolCallResult {",
  "export async function executeToolCall(name: string, args: ToolInput): Promise<ToolCallResult> {"
)

// Helper for close_terminals
content = content.replace(
  `    case 'close_terminals': {
      const { pane_ids } = args
      if (Array.isArray(pane_ids)) {
        win.webContents.send('athena:close-panes', pane_ids)
      }
      return { text: \`Closed \${pane_ids?.length ?? 0} terminal(s).\` }
    }`,
  `    case 'close_terminals': {
      const { pane_ids } = args
      if (Array.isArray(pane_ids)) {
        await new Promise<void>((resolve) => {
          ipcMain.once('athena:close-panes:ack', () => resolve())
          win.webContents.send('athena:close-panes', pane_ids)
        })
      }
      return { text: \`Closed \${pane_ids?.length ?? 0} terminal(s).\` }
    }`
)

// Helper for launch_builtin_agent
content = content.replace(
  `    case 'launch_builtin_agent': {
      const { task_prompt, agent_type = 'claude', agent_count = 1 } = args
      const agentCommand = buildAgentCommand(agent_type, task_prompt)
      for (let i = 0; i < agent_count; i++) {
        const id = \`agent-\${randomUUID()}\`
        win.webContents.send('athena:agent-spawned', { id, agentType: agent_type, agentCmd: agentCommand })
      }
      return { text: \`Done, launched \${agent_count} \${agent_type} agents.\` }
    }`,
  `    case 'launch_builtin_agent': {
      const { task_prompt, agent_type = 'claude', agent_count = 1 } = args
      const agentCommand = buildAgentCommand(agent_type, task_prompt)
      const promises = []
      for (let i = 0; i < agent_count; i++) {
        const id = \`agent-\${randomUUID()}\`
        promises.push(new Promise<void>((resolve) => {
          ipcMain.once(\`athena:agent-spawned:ack:\${id}\`, () => resolve())
          win.webContents.send('athena:agent-spawned', { id, agentType: agent_type, agentCmd: agentCommand })
        }))
      }
      await Promise.all(promises)
      return { text: \`Done, launched \${agent_count} \${agent_type} agents.\` }
    }`
)

// Helper for launch_custom_agent
content = content.replace(
  `    case 'launch_custom_agent': {
      const { command, agent_count = 1 } = args
      for (let i = 0; i < agent_count; i++) {
        const id = \`custom-agent-\${randomUUID()}\`
        win.webContents.send('athena:agent-spawned', { id, agentType: 'custom', agentCmd: command })
      }
      return { text: \`Done, launched \${agent_count} custom agents.\` }
    }`,
  `    case 'launch_custom_agent': {
      const { command, agent_count = 1 } = args
      const promises = []
      for (let i = 0; i < agent_count; i++) {
        const id = \`custom-agent-\${randomUUID()}\`
        promises.push(new Promise<void>((resolve) => {
          ipcMain.once(\`athena:agent-spawned:ack:\${id}\`, () => resolve())
          win.webContents.send('athena:agent-spawned', { id, agentType: 'custom', agentCmd: command })
        }))
      }
      await Promise.all(promises)
      return { text: \`Done, launched \${agent_count} custom agents.\` }
    }`
)

writeFileSync(path, content)
