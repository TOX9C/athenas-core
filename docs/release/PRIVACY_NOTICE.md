# Athena's Core Privacy Notice

**Applies to:** Athena's Core macOS Apple Silicon desktop release  
**Last updated:** 2026-08-08

Athena's Core does not require an Athena telemetry account. The app stores workspace and session state locally, but it is not correct to describe all data as local: when you configure an external provider or plugin, selected prompts, files, terminal output, and responses may leave the computer.

## What the app can handle

- Workspace paths and file contents selected by you or included in an AI request.
- Chat prompts, responses, attached images, and session history.
- Terminal and agent output shown in panes.
- API keys and provider configuration.
- Plugin manifests, configuration, event data, and agent metadata.
- Browser URLs and content loaded in the embedded browser.
- Optional Mobile Mirror traffic on your local network.
- Local diagnostics and logs that you choose to share with support.

## Where data goes

- **Configured LLM provider:** prompts, selected workspace context, attachments, tool requests, and responses are sent to the provider endpoint you configure. The provider's privacy policy and retention terms apply.
- **Local model server:** requests go to the local endpoint you configure, such as LM Studio, but the server may have its own logs and retention.
- **Plugins and MCP clients:** the public release supports trusted developer integrations. Trusted plugin processes can receive declared events and data and can use their own process/network permissions; plugins are not sandboxed by Athena's plugin manager. There is no public marketplace or remote plugin installation channel in this release.
- **Websites:** the native Tauri child browser sends URLs and page requests to the websites you visit. The browser intentionally permits localhost and private IPv4 targets for local development, so only navigate to trusted internal services.
- **Mobile Mirror:** when enabled, the desktop serves a bearer-token-protected HTTP/WebSocket companion over the LAN. It is experimental, plaintext, and must not be exposed to the public internet or an untrusted network.
- **Support:** diagnostics are shared only when you choose to attach them. Redact secrets and private file contents before sharing.

## Local storage and controls

Application state, workspace metadata, chat sessions, plugin configuration, logs, and diagnostics are stored in local application data locations. API keys are stored using the operating system credential store in production builds. Use Settings to clear the configured key and remove workspaces, sessions, plugins, or relay pairing as appropriate.

Athena's Core does not silently sell personal data or enable hidden analytics in the current release scope. If telemetry or crash reporting is added later, this notice must be updated before enabling it.

## Your responsibilities

Review prompts and workspace context before sending them to a provider. Do not place credentials in prompts or commit secrets to workspaces. Install only plugins you trust. Keep Mobile Mirror disabled unless you understand and accept the trusted-LAN risk.

## Deletion and support

Delete sessions, workspace metadata, and plugin configuration using the app where those controls are available. Local logs and diagnostics follow the operating system's application-data lifecycle and may require manual cleanup; do not delete them before preserving evidence for an incident. Provider-side copies must be deleted through the provider. For support, follow [`SUPPORT_RUNBOOK.md`](./SUPPORT_RUNBOOK.md) and never send API keys, relay tokens, or unredacted credentials.

This notice is a product disclosure, not a guarantee about third-party provider retention or plugin behavior. Review each provider and plugin's own terms before use.
