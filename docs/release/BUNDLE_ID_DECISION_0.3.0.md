# Bundle Identifier Decision — v0.3.0

**Status:** Decision recorded
**Date:** 2026-08-13
**Decision:** Clean reinstall. `com.athena.core` is the public bundle
identifier. Users with the prior development identifier `com.athena.app` must
uninstall the previous build before installing 0.3.0.

## Background

Athena's Core 0.3.0 ships under a permanent bundle identifier for public
distribution. Earlier development builds used `com.athena.app`; the public
launch candidate uses `com.athena.core` (see `src-tauri/tauri.conf.json`,
`identifier: "com.athena.core"`).

The two identifiers resolve to different Application Support, keychain, and
preferences locations on macOS. macOS treats them as separate applications.

## Decision

**Clean reinstall.** 0.3.0 does not migrate state from `com.athena.app`
installs. The install instructions must require users to:

1. Quit Athena's Core.
2. Delete the prior `/Applications/Athena's Core.app` if present.
3. (Optional, recommended) Back up workspace metadata and chat sessions
   from `~/Library/Application Support/com.athena.app/` before deletion.
4. Install 0.3.0 from the signed DMG.

Application Support data from the old identifier is **not** reused; the
new identifier starts with an empty Application Support directory.

## Why clean reinstall

- The 0.x development line has had breaking schema changes to the local
  store and chat session formats. Migration is real engineering work with
  non-trivial failure modes (corrupt state, lost sessions).
- The audience moving from `com.athena.app` to 0.3.0 is private beta
  testers, not production users with years of accumulated state.
- A documented clean reinstall is honest, low-risk, and reversible — the
  user keeps the old `.app` for the moment it takes to copy workspace
  directories they care about.

## User-facing install instructions (0.3.0)

The release notes for 0.3.0 must include, verbatim or equivalent:

> If you installed an earlier development build (bundle identifier
> `com.athena.app`), uninstall it before installing 0.3.0:
>
> 1. Quit the old Athena's Core.
> 2. Drag the old `/Applications/Athena's Core.app` to the Trash.
> 3. (Recommended) Back up `~/Library/Application Support/com.athena.app/`
>    before emptying the Trash if you want to preserve workspace metadata
>    or chat sessions.
> 4. Install 0.3.0 by opening the signed DMG and dragging the new
>    `Athena's Core.app` into `/Applications`.
>
> 0.3.0 uses bundle identifier `com.athena.core`; data is not migrated
> from the previous identifier.

## Future migration

If a future release needs to migrate `com.athena.app` users, that work
requires:

- A migration tool or on-launch migration path.
- A schema-compatibility test against real exported state.
- A documented rollback path if migration fails.
- Re-evaluation of this decision.

## Cross-references

- [`MACOS_TEST_MATRIX.md`](./MACOS_TEST_MATRIX.md) — migration tested separately under "Upgrade A".
- [`PRIVACY_NOTICE.md`](./PRIVACY_NOTICE.md) — Application Support data handling.
- `src-tauri/tauri.conf.json` — bundle identifier source of truth.
