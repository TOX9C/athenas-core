# Codebase Knowledge Graph (RAG)

This repository maintains a full knowledge graph of its codebase in `graphify-out/`.
Use it as a **retrieval-augmented generation (RAG)** system to quickly find files, functions, and relationships.

## How to Query the Graph

The graph is at `graphify-out/graph.json`. Use the `graphify` CLI to query it.

### Basic Queries

```bash
# Find shortest path between two concepts
graphify path "XtermMount" "pty_spawn"

# Explain a single node (everything connected to it)
graphify explain "AthenaOrchestrator"

# BFS search - broad context around a topic
graphify query "how does terminal output flow from PTY to xterm.js?"

# DFS search - trace a specific dependency chain
graphify query "terminal channel IPC" --dfs
```

### Available Tools

| Command | Purpose |
|---------|---------|
`graphify path "A" "B"` | Shortest path between two nodes |
`graphify explain "Node"` | Plain-language explanation of a node |
`graphify query "..."` | BFS traversal for broad context |
`graphify query "..." --dfs` | DFS traversal for specific chains |

### MCP Server (Optional)

Start an MCP server for agentic access:

```bash
python3 -m graphify.serve graphify-out/graph.json
```

This exposes tools: `query_graph`, `get_node`, `get_neighbors`, `get_community`, `god_nodes`, `graph_stats`, `shortest_path`.

## Graph Statistics

- **3323 nodes** (functions, structs, modules, concepts, docs)
- **5472 edges** (calls, uses, implements, references, rationale_for)
- **284 communities** (auto-detected clusters/files/features)
- **30 labeled communities** including: Agent Commands, Browser Management, Tauri Bridge, Xterm.js, Athena UI, Kanban, Sessions & PTY, MCP Server, Swarm, etc.

## Key Entry Points

| Area | Nodes to Query |
|------|----------------|
Terminal system | `xterm_mount`, `pty_spawn`, `session_manager`, `TerminalSession` |
Athena AI | `AthenaOrchestrator`, `athena_chat`, `AthenaPanel`, `AthenaInput` |
Plugins | `PluginManager`, `plugin_system_mcp`, `McpServer` |
Swarm | `SwarmCoordinator`, `swarm_board`, `swarm_launcher` |
Notifications | `NotificationService`, `notification_bell`, `NotificationPanel` |
Browser | `BrowserManager`, `browser_back`, `browser_forward` |

## Regenerating the Graph

After significant code changes:

```bash
# Incremental update (only changed files)
graphify --update

# Full rebuild
graphify .
```
