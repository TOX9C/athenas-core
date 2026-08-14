# Public Plugin Trust Policy

**Policy identifier:** `trusted_developer_integrations`  
**Applies to:** Athena's Core 0.3.0 public-launch scope

Plugins are trusted developer integrations. They are **not** sandboxed by Athena's plugin manager and may use the operating-system privileges available to the plugin process or the agent that launches it.

## Included in this release

- The bundled Claude Code and OpenCode integrations.
- Plugin manifests and integrations that a user deliberately registers from a trusted local workspace.
- Manifest validation, capability intersection, event-subscription declarations, payload/configuration limits, and manager-owned session cleanup.

## Explicitly not included

- A public marketplace or remote plugin installation channel.
- Cryptographic provenance or signature verification for arbitrary local plugin code.
- Process, filesystem, network, or environment sandboxing for external plugin/MCP processes.
- A security guarantee that a plugin cannot access data available to its host process.

Manifest validation protects the host from malformed metadata and unsafe command/path declarations; it is not a trust decision for the executable itself. Users must install and enable only code they trust. A future untrusted-plugin ecosystem requires a caller-identity design, schema enforcement, provenance/signature policy, and process sandbox before it can be advertised as supported.

This policy is intentionally separate from the Mobile Mirror decision: Mobile Mirror remains experimental, disabled by default, and excluded from public-launch guarantees.
