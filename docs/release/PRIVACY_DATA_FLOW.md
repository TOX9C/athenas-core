# Privacy and Data Flow Inventory

**Status:** Inventory complete; user-facing notice published at [`PRIVACY_NOTICE.md`](./PRIVACY_NOTICE.md)  
**Scope:** Athena's Core desktop app and any enabled Mobile Mirror/agent/plugin features.

The user-facing disclosure is [`PRIVACY_NOTICE.md`](./PRIVACY_NOTICE.md). Keep both documents aligned. Do not claim that data stays local when prompts, files, or output are sent to an external provider selected by the user.

## Data categories

| Data                    | Source                   | Stored locally?   | Sent externally?                       | Destination                            | User control/retention                                |
| ----------------------- | ------------------------ | ----------------- | -------------------------------------- | -------------------------------------- | ----------------------------------------------------- |
| Workspace paths         | Workspace setup          | Yes               | Possibly as context                    | Selected LLM/provider or agent         | Trusted roots/remove workspace                        |
| Workspace file contents | Athena tools/user action | Potentially       | If included in prompt/tool call        | Selected provider                      | User chooses workspace/provider                       |
| Chat prompts/responses  | Chat panel               | Session store     | Provider request/response              | Selected LLM provider                  | Delete sessions/store                                 |
| API key                 | User settings            | OS keychain       | Provider authorization only            | Selected provider                      | Clear API key                                         |
| Terminal output         | PTY/agent panes          | Buffers/history   | If forwarded to Athena/provider/plugin | Selected local/external service        | Close pane/clear history                              |
| Agent metadata          | PTY/plugin/event systems | Local state       | Plugin/MCP/notification paths          | Configured local process               | Disable plugin/agent                                  |
| Plugin manifests/config | Plugin directories/store | Yes               | Plugin-defined                         | Plugin process/network                 | Disable/remove plugin                                 |
| Notifications           | Backend services         | Local history     | macOS notification service             | OS                                     | Notification settings/clear                           |
| Browser URLs/content    | Browser panel            | UI/session state  | Visited sites                          | Website and native Tauri child webview | User navigation; browser commands remain desktop-only |
| Mobile Mirror traffic   | Desktop relay            | In transit on LAN | Phone on same LAN                      | Paired client                          | Disable relay/revoke token                            |
| Diagnostics/logs        | App runtime              | Local/system logs | Only if user shares                    | Support channel                        | Redaction/copy choice                                 |

## External services

Document each configured provider separately:

- LLM providers and API endpoints.
- Local model servers such as LM Studio.
- Any release/update endpoint.
- Any telemetry/crash-reporting service, if added.
- Any plugin-host network destination.

For each service record:

```text
Service:
What data is sent:
Why it is sent:
Authentication:
Retention controlled by:
User-facing disclosure:
Opt-out/control:
```

## Local storage

Verify and document:

- Application support directory.
- Key-value store.
- Chat session files.
- Images/attachments.
- Workspace metadata.
- Plugin configuration.
- Swarm state and mailboxes.
- Logs and crash artifacts.
- OS keychain entries.

## Privacy requirements before launch

- [x] User-facing privacy notice published as `PRIVACY_NOTICE.md`.
- [ ] Provider data handling is accurately described.
- [ ] API-key storage and clearing are documented.
- [ ] Workspace context behavior is explained.
- [ ] Plugin trust/data behavior is explained.
- [ ] Mobile Mirror LAN behavior is explained.
- [ ] Diagnostic sharing is opt-in and redacted; the repository privacy guard covers audited native log paths, but a formal diagnostic-export implementation and global redaction proof remain open.
- [x] Data deletion controls and support-safe sharing guidance are documented in `PRIVACY_NOTICE.md` and `SUPPORT_RUNBOOK.md`; a formal diagnostic export remains future work.
- [ ] No hidden telemetry is enabled without disclosure/consent.
