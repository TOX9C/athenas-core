# Manual Update and Emergency Rollback Runbook

**Scope:** macOS Apple Silicon beta/public-candidate artifacts while no in-app updater is shipped.

This is a fallback delivery process, not proof that the updater gate is complete. Public launch still requires release-owner approval, signed/notarized artifacts, a published checksum, and a tested communication path.

## Publish

1. Build from a clean, tagged revision using `.github/workflows/release-macos.yml`.
2. Publish only the signed/notarized DMG and its `.sha256` file.
3. Publish the release notes and supported macOS/architecture scope.
4. Keep unsigned CI artifacts private for internal testing; never link them as public downloads.
5. Record the DMG SHA-256, tag, commit, signing identity, notarization result, and release owner in `RELEASE_CHECKLIST.md`.

## User installation

1. Download the DMG and checksum from the official release channel.
2. Verify the checksum:

   ```bash
   shasum -a 256 -c "Athena's Core_0.3.0_aarch64.dmg.sha256"
   ```

3. Open the DMG and replace the existing app in Applications.
4. Launch from Finder and confirm the version in the app/about surface.
5. If macOS reports an invalid signature or the checksum fails, do not open the app; report the release immediately.

The app stores user state outside the application bundle. Replacing the app must not be presented as a data reset, but users should still back up important workspace/session data before upgrading.

## Emergency rollback

1. Stop publication and mark the affected release unavailable.
2. Notify the release/support owners with the affected version, artifact hash, and failure mode.
3. Publish the last known-good signed/notarized DMG with its checksum.
4. Ask affected users to quit Athena, preserve diagnostics, and reinstall the known-good artifact.
5. Do not ask users to delete application-support data unless a separate, reviewed recovery procedure requires it.
6. Record whether the incident requires a data migration or a release-note warning before re-publication.

## Required evidence

- Release tag and commit.
- DMG SHA-256.
- `hdiutil verify` output.
- `codesign --verify --deep --strict --verbose=2` output.
- `spctl --assess --type execute --verbose` output.
- `xcrun stapler validate` output.
- Clean Apple Silicon install/launch result.
- Communication/rollback owner and timestamps.
