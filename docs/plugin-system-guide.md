# Plugin System Guide

This is the current native plugin-manager guide for Athena's Core. Plugins are trusted developer integrations, not sandboxed extensions. The user must review plugin source, installation hooks, MCP commands, environment variables, and requested capabilities before enabling a plugin.

## Runtime topology

Bundled Claude Code and OpenCode integrations use:

```text
external CLI → stdio → bin/mcp-proxy.js → authenticated TCP → Rust McpServer :4545
```

The legacy agent-comms service on port `4546` is not the public plugin transport. Do not expose it or configure new integrations against it.

## Plugin lifecycle

A plugin moves through these statuses:

```text
registered → installed → enabled → disabled
                         └──────→ error
```

Registration validates the full manifest before it enters the registry. Disabling a plugin removes its active sessions, subscriptions, and pending messages. Unregistering also removes the plugin entry and emits a registry update.

The manager provides lifecycle operations through Tauri commands and callbacks:

| Operation                          | Purpose                                                    |
| ---------------------------------- | ---------------------------------------------------------- |
| `plugin_host_discover_plugins`     | Read JSON manifests from a validated workspace directory   |
| `plugin_host_setup_plugin`         | Register a trusted internal plugin by ID/name/version      |
| `plugin_host_remove_plugin`        | Remove a plugin and clean up sessions                      |
| `plugin_host_register_session`     | Associate an agent/session with a plugin and optional pane |
| `plugin_host_unregister_session`   | Remove a plugin session                                    |
| `plugin_host_subscribe`            | Subscribe a session to bounded event types                 |
| `plugin_host_send_message`         | Queue a bounded message for a session                      |
| `plugin_get_config`                | Read the current plugin configuration                      |
| `plugin_set_config`                | Merge and schema-validate configuration                    |
| `plugin_enable` / `plugin_disable` | Change active lifecycle state                              |
| `plugin_set_error`                 | Mark a plugin as unhealthy with a bounded diagnostic       |

`plugin_register` is a low-level internal command that creates a minimal manifest. Complete disk manifests should go through discovery and registration validation.

## Manifest and capabilities

The native manifest uses camelCase JSON names:

```json
{
  "id": "com.example.plugin",
  "name": "Example Plugin",
  "version": "1.0.0",
  "description": "A trusted local integration.",
  "author": "Example",
  "permissions": ["notifications"],
  "capabilities": ["notifications", "status"],
  "tools": [],
  "subscribesTo": ["notification", "status_update"],
  "mcpConfig": {
    "command": "node",
    "args": ["dist/server.js"],
    "env": { "NODE_ENV": "production" }
  },
  "config": {
    "schema": {
      "type": "object",
      "properties": { "mode": { "type": "string", "enum": ["safe", "fast"] } },
      "required": ["mode"],
      "additionalProperties": false
    },
    "defaults": { "mode": "safe" }
  },
  "install": { "type": "mcp_server", "command": "node", "args": ["dist/server.js"] }
}
```

Supported capabilities are `notifications`, `status`, `tasks`, `agentControl`, `userInput`, `file_access`, and `swarm`.

A session's effective capabilities are the intersection of:

1. safe defaults for its agent type,
2. capabilities declared by the plugin manifest, and
3. capabilities requested by the session.

The plugin cannot expand the agent's safe defaults by requesting additional values.

## Configuration

`config.schema` and `config.defaults` are validated during manifest registration. The host supports a bounded JSON-Schema subset:

- `type` (including a list of types)
- `properties`
- `required`
- `additionalProperties`
- `items` for one repeated item schema
- `enum` and `const`
- `minLength` and `maxLength`
- `minimum` and `maximum`

Boolean schemas are supported. Unsupported keywords—including `oneOf`, `anyOf`, `allOf`, `pattern`, tuple-style `items`, and references—are rejected so the UI never implies validation that the host does not perform. Schema nesting is capped at 64 levels.

The following limits apply:

- Manifest/configuration/event payloads: 256 KiB serialized.
- Plugin error text: bounded before it is stored or emitted.
- Event subscriptions: at most 32 per manifest/session as applicable.
- Sessions and pending messages: bounded by the plugin manager limits.

`plugin_set_config` merges object updates and validates the complete merged object. Invalid updates do not replace the existing value.

## Session and event model

A plugin session contains a generated/session-provided ID, plugin ID, agent ID, optional pane ID, agent type, status, timestamps, and effective capabilities. Session statuses are `active`, `idle`, `waitingInput`, and `disconnected`.

Events include:

- `notification`
- `status_update`
- `taskComplete`, `taskError`, and `progressUpdate`
- `needsInput` and `userResponse`
- `agentSpawned`, `agentExited`, `agentStalled`, `agentConnected`, and `agentDisconnected`
- `artifactProduced`
- `controlCommand`
- `pluginRegistered` and `pluginError`
- `outputForwarded`

Event payloads should carry stable IDs (`pluginId`, `sessionId`, `agentId`, and `paneId`) and must not rely on human-readable pane labels.

## Security rules

Manifest validation rejects:

- empty/oversized/invalid plugin IDs;
- absolute or traversing hook paths;
- shell metacharacters in hook paths;
- non-whitelisted MCP executables and executable paths;
- `PATH` or `HOME` environment overrides;
- oversized subscription/configuration payloads;
- malformed schemas, unknown validation keywords, invalid type arrays, duplicate required names, invalid numeric bounds, and excessive nesting.

The plugin host is not a sandbox. A trusted plugin can still read files, start processes, access network resources, and observe data available to its process. Capability declarations are coordination and UI policy; they are not an OS-level security boundary. Owner-aware session methods perform an atomic stored-owner check, but they only become an authentication boundary when `plugin_id` is obtained from an authenticated host/session context; legacy ID-only Tauri operations remain trusted compatibility paths.

## Operational checklist

When installing a plugin:

1. Review its source and manifest.
2. Review every `install` hook and MCP command/argument.
3. Confirm environment variables do not contain secrets unless intentionally passed by the integration.
4. Confirm requested capabilities are necessary.
5. Test enable/disable and session cleanup.
6. Test invalid configuration and malformed event payloads.
7. Keep diagnostics on stderr or a redacted logger; never write MCP protocol output or credentials to stdout/logs.

Repository checks:

```bash
cargo test -p athena-plugins
npm run check:plugin-integration
npm run check:tauri-permissions
npm run check:release-privacy
```

For release approval, reconcile this guide with `docs/release/PLUGIN_TRUST_POLICY.md`.
