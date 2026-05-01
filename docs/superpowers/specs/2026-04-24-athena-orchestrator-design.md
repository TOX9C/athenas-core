# Athena Swarm Orchestrator Design Spec

**Goal:** Turn the Athena chat interface into a high-level orchestrator capable of planning tasks on a Kanban board and spinning up multiple "worker" agents that pull these tasks via MCP.

**Architecture: The Swarm Worker Pool**
We will implement an enterprise-style "Message Queue" (worker pool) mechanism. Athena will act as the Orchestrator, planning out tasks on a Kanban board. The spawned CLI instances (e.g., Claude Code) act as stateless workers. They use MCP to automatically request tasks from the IDE, execute them, and report back.

## 1. The MCP Bridge (Transport mechanism)

Electron Main processes do not have clean access to `stdin/stdout` (they swallow output for Electron logging). To allow standard CLI tools to connect via MCP `stdio`, we need a proxy bridging script.

- `electron/mcp-server.ts`: We will create a local TCP server (`net.createServer`) inside the Electron Main process. It will listen on a local port (e.g., `4545`) and parse typical MCP JSON-RPC messages.
- `bin/mcp-proxy.js`: A tiny, standalone Node script running standard `process.stdin`/`stdout`. It simply connects an underlying TCP socket to `localhost:4545`.
- **Workflow:** When configuring an agent, its MCP configuration points to `node bin/mcp-proxy.js`.

## 2. Shared State: The Kanban Brain

The `tasks` state managed by Zustand currently persists to `electron-store`.

- Because `electron-store` reads and writes synchronously from disk, the Electron Main process (the MCP Server) can directly `store.get('tasks')` and `store.set('tasks')`.
- When a worker uses an MCP tool to fetch or update a task, the Electron MCP server updates `electron-store`.
- The IDE's frontend will use standard IPC bridging or file-watching to instantly reflect UI changes on the Kanban board when a worker agent claims a task.

## 3. The Orchestrator Tools (MCP Endpoints)

The `mcp-server.ts` will expose the following distinct tools to the CLI agents:

1.  **`create_tasks`**: Takes an array of `{title, desc}`. Pushes new "To Do" tickets to the Kanban board.
2.  **`get_next_task`**: An agent calls this. The server finds the highest-priority "To Do" task, immediately assigns it to that agent, updates its status to "In Progress", and returns the task payload.
3.  **`update_task_status`**: Takes `task_id`, `status`, and `notes`. The agent calls this upon failure or success to move the card to "Done" (or back to "To Do" if it crashed/failed).
4.  **`spawn_agents`**: Takes `count`. Calls `ptyManager.spawn` multiple times.

## 4. Bootstrapping Workers

When the Orchestrator calls `spawn_agents(3)`:

- The IDE opens 3 terminal tabs.
- It boots the default Custom Agent (e.g., `claude`).
- It appends a standard override prompt flag to the boot command (e.g., `claude -p "You are a Swarm Worker. Continuously use the 'get_next_task' MCP tool to pull work until no tasks remain."`).

## 5. Security & Isolation

- Agents will only have access to the Workspace directory they are spawned in.
- The proxy script `mcp-proxy.js` will be restricted strictly to `localhost` to prevent external network hijacking.
