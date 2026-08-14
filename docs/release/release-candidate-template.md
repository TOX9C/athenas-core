# Release Candidate Report — vX.Y.Z

**Candidate tag/commit:**  
**Build date:**  
**Release owner:** TOX9C
**Supported platform:** macOS / Apple Silicon / other:  
**Scope:**  
**Artifact URL:**  
**Artifact SHA-256:**

## Executive decision

**Decision:** GO / NO-GO / CONDITIONAL  
**Conditions:**  
**Approvers:** TOX9C (solo)

## Gate status

| Gate                    | Status | Evidence | Owner | Notes |
| ----------------------- | ------ | -------- | ----- | ----- |
| Scope/ownership         | ☐      |          |       |       |
| Stability               | ☐      |          |       |       |
| Data integrity          | ☐      |          |       |       |
| Security                | ☐      |          |       |       |
| UX/support              | ☐      |          |       |       |
| Packaging               | ☐      |          |       |       |
| Release CI              | ☐      |          |       |       |
| Updates/manual delivery | ☐      |          |       |       |
| RC validation           | ☐      |          |       |       |
| Operations              | ☐      |          |       |       |

## Automated verification

| Command/check                                       | Result | Log/evidence |
| --------------------------------------------------- | ------ | ------------ |
| `cargo fmt --all -- --check` (or approved baseline) |        |              |
| `cargo check --workspace`                           |        |              |
| `npm run check:clippy-baseline` (warning baseline)  |        |              |
| `cargo test --workspace`                            |        |              |
| `bash frontend/build-dist.sh`                       |        |              |
| `npm test`                                          |        |              |
| `npm run test:mcp`                                  |        |              |
| `npm run lint`                                      |        |              |
| `npm run format:check`                              |        |              |
| Tauri command drift                                 |        |              |
| Plugin integration                                  |        |              |
| Artifact verification                               |        |              |
| `npm run check:release-identity`                    |        |              |
| `npm run check:release-privacy`                     |        |              |

## Manual/macOS verification

- [ ] Clean install.
- [ ] Finder launch/Gatekeeper.
- [ ] First-run onboarding.
- [ ] Workspace and filesystem.
- [ ] Terminal/PTY.
- [ ] Drag/swap/layout.
- [ ] Panels/modals.
- [ ] Chat/provider errors.
- [ ] Kanban/swarm/agents.
- [ ] Plugins.
- [ ] Browser.
- [ ] Settings/persistence.
- [ ] Restart/upgrade.
- [ ] Watchdog/recovery.
- [ ] Sleep/wake/minimize/restore.
- [ ] Soak test (`cd e2e-tests && SOAK_ITERATIONS=10 npm run test:soak`).

Soak duration:  
Soak machine:  
Watchdog reload count:  
Crashes/freezes:  
Data-loss incidents:

## Findings

| ID  | Severity | Description | Reproduction | Disposition | Owner |
| --- | -------- | ----------- | ------------ | ----------- | ----- |
|     |          |             |              |             |       |

## Security and privacy

- [ ] `SECURITY_REVIEW.md` approved.
- [ ] `PRIVACY_DATA_FLOW.md` reconciled with public policy.
- [ ] No secrets in artifact/logs/repository.
- [ ] Mobile Mirror decision documented.
- [ ] Plugin trust model documented.
- [ ] Updater decision recorded ([`UPDATER_DECISION_0.3.0.md`](./UPDATER_DECISION_0.3.0.md)) — owners named and dry-runs completed.

## Distribution and rollback

- [ ] Signed app verified.
- [ ] Notarization/stapling verified.
- [ ] DMG verified.
- [ ] Checksums published.
- [ ] Update or manual-install path tested.
- [ ] Last-known-good rollback artifact identified.
- [ ] Incident response owner on call for staged release.

## Launch notes

- User-facing release notes:
- Known limitations:
- Support channel:
- Staged cohort:
- Monitoring:
- Rollback trigger:
