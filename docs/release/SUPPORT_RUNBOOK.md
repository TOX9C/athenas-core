# Public Launch Support Runbook

**Owner:** TOX9C — support/communications owner

## Support intake

Ask for:

- App version.
- macOS version and Apple Silicon model.
- Install source and artifact/checksum if available.
- Whether the app is signed/notarized release or development build.
- Exact reproduction steps.
- Expected versus actual result.
- Whether the issue is reproducible after restart.
- Redacted logs/diagnostics and screenshot/screen recording.
- Active plugins, providers, Mobile Mirror state, and number of panes/agents.

Never request API keys, updater private keys, relay tokens, or unredacted credentials.

## Triage categories

- Startup/install.
- WebView freeze/reload.
- Terminal/PTY.
- Workspace/filesystem.
- Chat/provider.
- Agent/swarm.
- Plugin.
- Browser.
- Notifications.
- Mobile Mirror.
- Update/signing.
- Data migration/corruption.

## First-response troubleshooting

1. Confirm app version and platform.
2. Ask whether a reload/restart reproduces it.
3. Check whether the backend/PTY is still running.
4. Ask the user to disable experimental plugins/relay if relevant.
5. Confirm workspace path and permission state without requesting private file contents.
6. For provider failures, check endpoint/provider status and API-key presence without requesting the key.
7. For data issues, stop destructive cleanup until a backup is made.
8. Escalate P0/P1 incidents using `INCIDENT_RESPONSE.md`.

## Diagnostics policy

Diagnostics must be:

- Opt-in.
- Redacted for API keys, authorization headers, tokens, private paths where practical, and prompt content where not needed.
- Versioned and attributable to a specific release.
- Safe to attach to an issue.

## Common recovery guidance

### UI appears frozen

- Wait for the watchdog reload.
- Use the manual reload action.
- Record the last action before restarting.
- Report whether terminals/agents continued running.

### API key/provider failure

- Open Settings.
- Verify provider and endpoint.
- Re-enter or clear the key.
- Test the key using the built-in test action.
- Do not paste the key into support.

### Terminal does not start

- Verify the configured/default shell exists.
- Verify workspace directory permissions.
- Try a new pane.
- Record shell/platform details.

### Workspace cannot be read/written

- Verify the directory still exists.
- Re-add it as a trusted root through the app.
- Do not bypass path restrictions manually.
- Preserve a copy of the affected data before repair.

### Mobile Mirror issue (experimental / unsupported for public launch)

Mobile Mirror is excluded from the public-launch guarantees for this release. Do not treat it as a supported production access path or ask users to enable it as a workaround. If an internal/beta user reports an issue:

- Confirm it was enabled intentionally for a trusted-LAN test.
- Stop the relay and rotate/revoke the session token if supported.
- Verify phone and Mac are on the trusted LAN.
- Never expose port 8787 to the public internet.
- Escalate any unexpected file, terminal, or session access as a security incident.

## Escalation thresholds

Escalate immediately for:

- Data loss or corruption.
- Credential/token exposure.
- Reproducible core-workflow freeze.
- Unsigned or invalid update artifact.
- Unexpected access to another workspace or pane.
- Plugin executing outside its documented trust boundary.

## Support metrics

Track:

- Time to first response.
- Time to reproduce.
- Version/platform distribution.
- Top failure category.
- Freeze/watchdog rate.
- Update failure rate.
- Open P0/P1 count.
- Regressions by release.
