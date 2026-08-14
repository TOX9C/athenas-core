# Incident Response and Rollback

**Owner:** `<name> <email>` — release owner  
**Escalation contact:** `<name> <email>` — incident owner (on call for P0/P1)  
**Release channel:** Official signed GitHub release plus documented support channel

Use this runbook for crashes, freezes, data loss, security issues, bad updates, provider failures, or compromised release artifacts.

## Severity

- **P0:** data loss/corruption, credential exposure, remote/untrusted code execution, widespread startup failure, or unusable release. Stop rollout immediately.
- **P1:** reproducible core-workflow crash/freeze, broken update, serious security boundary failure, or major platform-specific failure. Halt expansion and prepare hotfix.
- **P2:** degraded feature or recoverable workflow issue. Triage for next release.
- **P3:** cosmetic/docs/minor issue. Normal backlog.

## Immediate response

1. Record version, commit/tag, artifact hash, OS/chip, and reproduction steps.
2. Freeze rollout and updater publication if P0/P1 is suspected.
3. Preserve relevant logs and screenshots after redacting credentials.
4. Determine whether the issue affects the app, provider, plugin, relay, updater, or packaging.
5. Disable or remove the affected release/update endpoint if necessary.
6. Notify affected testers/users with safe recovery instructions.
7. Open a tracked incident with owner and timeline.

## Renderer freeze procedure

For a UI freeze:

1. Wait for watchdog recovery once.
2. Use the manual interface reload action if available.
3. If backend processes remain active, do not blindly kill the app until state is understood.
4. Record the last interaction and console/runtime error.
5. If state is at risk, quit/restart only after capturing diagnostics.
6. Check for duplicate PTYs/listeners after recovery.
7. Escalate as P1 if reproducible in a core workflow.

## Security incident procedure

1. Stop publication and updater rollout.
2. Revoke/rotate affected tokens, updater keys, relay tokens, or provider credentials as applicable.
3. Identify affected versions and users.
4. Preserve evidence without publishing secrets.
5. Prepare a patched signed release.
6. Publish a concise advisory and required user action.
7. Review why CI/release checks did not catch the issue.

## Rollback options

- Pause staged rollout.
- Remove a bad updater manifest.
- Repoint downloads to the last known-good signed artifact.
- Publish a hotfix release.
- Provide manual installation of the last known-good version.
- Disable an experimental feature remotely only if the app supports it; otherwise publish explicit user steps.
- Never ask users to install an unsigned replacement as a normal fix.

## Communications template

```text
We identified an issue affecting Athena's Core <version> on <platform>.

Impact:
Affected versions:
What users should do now:
Data safety/status:
Current workaround:
Next update:
Support/contact:
```

## Post-incident review

- [ ] Root cause identified.
- [ ] A regression test exists.
- [ ] Release gate updated.
- [ ] Documentation/support guidance updated.
- [ ] Affected credentials/tokens rotated.
- [ ] Rollout resumed only after owner approval.
