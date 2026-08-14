# Plugin Development Guide

This guide describes the current Athena's Core plugin contract. Plugins are **trusted developer integrations**: they are not sandboxed, and a plugin process can access anything allowed to the user account that launches it. Only install plugin code you trust.

## Architecture

The canonical external integration path is:

```text
Claude Code / OpenCode
        │ stdio
        ▼
  bin/mcp-proxy.js
        │ authenticated TCP
        ▼
Rust McpServer on 127.0.0.1:4545
        │
        ▼
Athena's Core backend and workspace
```

The Node MCP package is an optional compatibility facade. New integrations should use the bundled proxy configuration rather than inventing a second listener or connecting directly to the legacy agent-comms port (`4546`).

The native plugin manager stores manifests, validates them before registration, tracks plugin sessions, scopes capabilities, and forwards plugin events. It does not execute arbitrary plugin code inside the renderer.

## Manifest format

The authoritative Rust type is `athena_plugins::PluginManifest`, serialized with camelCase field names:

```json
{
  "id": "com.example.workspace-tools",
  "name": "Workspace Tools",
  "version": "1.0.0",
  "description": "Tools for the local workspace.",
  "author": "Example Team",
  "permissions": ["file_access", "notifications"],
  "mcpConfig": {
    "command": "node",
    "args": ["dist/server.js"],
    "env": { "NODE_ENV": "production" }
  },
  "minAthenaVersion": "0.3.0",
  "capabilities": ["notifications", "status", "file_access"],
  "tools": [],
  "subscribesTo": ["notification", "status_update"],
  "config": {
    "schema": {
      "type": "object",
      "properties": {
        "mode": { "type": "string", "enum": ["safe", "fast"] }
      },
      "required": ["mode"],
      "additionalProperties": false
    },
    "defaults": { "mode": "safe" }
  },
  "install": {
    "type": "mcp_server",
    "command": "node",
    "args": ["dist/server.js"],
    "env": { "NODE_ENV": "production" }
  }
}
```

`mcpConfig` is the runtime MCP connection description. `install` describes the supported installation method and is one of:

```json
{"type": "builtin"}
{"type": "mcp_server", "command": "node", "args": [], "env": {}}
{"type": "hook", "script": "scripts/setup.sh"}
```

There is no `entryPoint` field in the native manifest. Use `install` and/or `mcpConfig` instead.

### Capabilities and agent types

The capability enum is:

- `notifications`
- `status`
- `tasks`
- `agentControl`
- `userInput`
- `file_access`
- `swarm`

Supported agent types are `claude`, `codex`, `opencode`, `gemini`, `qwen`, `aider`, `cursor`, `freebuff`, `omp`, `custom`, and `shell`.

A session receives only the intersection of its agent's safe defaults, the plugin manifest's declared capabilities, and the requested capabilities. Requesting extra capabilities does not elevate a session.

## Validation and limits

Manifests are rejected before registration when they violate security or resource rules:

- Plugin IDs are non-empty, at most 128 bytes, and contain only ASCII letters, digits, `-`, `_`, or `.`.
- Name, author, and description have bounded lengths.
- MCP commands must be a whitelisted bare executable (`node`, `python`, `python3`, `ruby`, `cargo`, `sh`, `bash`, `zsh`, `npx`, `deno`, `uv`, `uvx`, or `pipx`). Absolute paths and path separators are rejected.
- Hook scripts must be relative paths without `..` traversal or shell metacharacters.
- Plugin and event configuration payloads are limited to 256 KiB.
- Plugin configuration schemas use a bounded supported JSON-Schema subset: `type`, `properties`, `required`, `additionalProperties`, `items`, `enum`, `const`, string length, and numeric bounds.
- Unsupported schema keywords and malformed schema shapes are rejected rather than silently ignored.
- Schema nesting is limited to 64 levels.
- Configuration defaults are validated against their schema before the manifest is accepted.
- `PATH` and `HOME` cannot be overridden through MCP environment maps.

The configuration validator supports boolean schemas and rejects tuple-style `items`, unsupported combinators, malformed types, duplicate required names, non-finite numeric bounds, and inverted min/max ranges.

## MCP server implementation

A plugin MCP process should keep stdout reserved for the transport protocol. Send diagnostics to stderr or a local logger. A minimal Node server can use the MCP SDK:

```ts
import { Server } from '@modelcontextprotocol/sdk/server/index.js'
import { StdioServerTransport } from '@modelcontextprotocol/sdk/server/stdio.js'

const server = new Server(
  { name: 'workspace-tools', version: '1.0.0' },
  { capabilities: { tools: {} } },
)

// Register tools using the SDK's current tool registration API.
// Keep schemas narrow and validate all arguments again in the implementation.

await server.connect(new StdioServerTransport())
```

For Athena's bundled integrations, prefer the generated setup scripts in `plugins/claude-code-athena` and `plugins/opencode-athena`. They install the `athena` entry that launches `bin/mcp-proxy.js`; do not hard-code a direct unauthenticated TCP connection.

## Plugin manager API

The native manager exposes these Tauri operations to the frontend/plugin host. The current ID-only unregister/subscribe/message commands are compatibility operations for trusted in-process callers; they do not authenticate a remote caller. The manager also provides owner-aware internal methods that atomically verify a session's stored plugin ID, but the supplied `plugin_id` must come from an authenticated host context before those methods can serve as a security boundary.

- `plugin_host_discover_plugins`
- `plugin_host_setup_plugin`
- `plugin_host_remove_plugin`
- `plugin_host_register_session`
- `plugin_host_unregister_session`
- `plugin_host_subscribe`
- `plugin_host_send_message`
- `plugin_get_config`
- `plugin_set_config`
- `plugin_enable`
- `plugin_disable`
- `plugin_set_error`

The low-level `plugin_register` command accepts an ID, name, and version and creates a minimal manifest for trusted internal callers. It is not a replacement for loading and validating a complete manifest from disk.

Configuration updates merge object values into the existing plugin configuration, then validate the complete merged value against the manifest schema. A rejected update leaves the previous configuration unchanged.

## Events and sessions

Plugin sessions carry a plugin ID, agent ID, optional pane ID, agent type, status, and scoped capabilities. Session status values are `active`, `idle`, `waitingInput`, or `disconnected`.

Plugin event types include notifications, status updates, task completion/errors, input requests/responses, agent connect/exit/stall, progress, artifacts, control commands, registry changes, plugin errors, and output forwarding.

When an event crosses from agent/session state into the renderer, consumers should use the explicit `paneId`/`sessionId` fields rather than guessing from display labels.

## Testing checklist

Before distributing a plugin:

1. Validate the manifest with the native plugin manager.
2. Test registration, enable/disable, session cleanup, and re-registration after disable.
3. Test invalid configuration values, unknown properties, oversized payloads, and invalid defaults.
4. Test malformed MCP requests and authentication failures.
5. Confirm the plugin never logs API keys, bearer tokens, prompts, or raw workspace contents.
6. Run the repository checks:

```bash
cargo test -p athena-plugins
npm run check:plugin-integration
npm run check:tauri-permissions
npm run check:release-privacy
```

For a release, also complete the packaged artifact and trust-model review in `docs/release/SECURITY_REVIEW.md` and `docs/release/PLUGIN_TRUST_POLICY.md`.
