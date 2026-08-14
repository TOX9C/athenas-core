# Updater Strategy Decision — v0.3.0

**Status:** Decision recorded (release-owner approval gate)
**Date:** 2026-08-13
**Decision:** Manual delivery, no in-app updater shipped with 0.3.0.

## Decision

Athena's Core 0.3.0 ships without an in-app updater. Updates are delivered via
the manual process documented in [`MANUAL_UPDATE_RUNBOOK.md`](./MANUAL_UPDATE_RUNBOOK.md):
signed/notarized DMG plus published SHA-256 checksum on the official release
channel, replaced by the user in `/Applications`.

This is a **conditional deferral**, not an informal waiver. The conditions in
`PUBLIC_LAUNCH_PLAN.md` (Phase 6) apply: release-owner approval, named
owners, a tested emergency hotfix path, and a tested user communication path.

## Rationale

- The Tauri updater plugin requires a signed update manifest endpoint with key
  rotation, an HTTPS host, and end-to-end tests for upgrade, interrupted
  download, invalid signature, offline mode, and rollback. None of this exists
  today and there is no public update endpoint provisioned.
- A failed updater is a worse user outcome than a missing one for 0.3.0.
- `MANUAL_UPDATE_RUNBOOK.md` already covers publish, user install, checksum
  verification, and emergency rollback against the last known-good artifact.

## Required owners

| Role                       | Owner | Notes                                                                                          |
| -------------------------- | ----- | ---------------------------------------------------------------------------------------------- |
| Manual-delivery owner      | TOX9C | Publishes signed DMG + checksum + release notes; responsible for the official release channel. |
| Emergency hotfix owner     | TOX9C | On call for P0/P1 incidents; can publish a hotfix release within SLA.                          |
| Communication owner        | TOX9C | Owns the user-facing communication path for outages, hotfixes, and rollback announcements.     |
| Certificate rotation owner | TOX9C | Owns Developer ID certificate rotation procedure per `MACOS_SIGNING_SETUP.md`.                 |

## Required pre-launch evidence

- [ ] Manual-delivery owner named.
- [ ] Emergency hotfix owner named.
- [ ] Communication owner named and channel operational.
- [ ] One dry-run of the manual update process against a signed candidate
      artifact (publish → download → checksum → install → launch → verify
      version).
- [ ] One dry-run of the emergency rollback process (mark release unavailable →
      publish last-known-good → notify → verify users can reinstall).
- [ ] `MANUAL_UPDATE_RUNBOOK.md` reviewed against the dry runs and updated if
      any step changed.

## Re-evaluation

The next release (post-0.3.0) must revisit this decision. A no-upater launch
is acceptable once; deferring again requires a written justification and an
explicit release-owner waiver recorded here.

## Cross-references

- [`MANUAL_UPDATE_RUNBOOK.md`](./MANUAL_UPDATE_RUNBOOK.md) — operational procedure.
- [`PUBLIC_LAUNCH_PLAN.md`](./PUBLIC_LAUNCH_PLAN.md) Phase 6 — gate definitions.
- [`RELEASE_CHECKLIST.md`](./RELEASE_CHECKLIST.md) — Updates and rollback section.
- [`INCIDENT_RESPONSE.md`](./INCIDENT_RESPONSE.md) — rollback procedure.
