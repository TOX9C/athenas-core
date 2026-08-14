# Local Unsigned Candidate Evidence — 0.3.0

**Status:** Internal build evidence only — not distributable

## Build

```text
Command: bash frontend/build-dist.sh && cargo tauri build --bundles dmg
Host: Apple Silicon arm64
Target: macOS DMG
Artifact: target/release/bundle/dmg/Athena's Core_0.3.0_aarch64.dmg
Bundle identifier: com.athena.core
Version: 0.3.0
Minimum macOS: 13.0
```

## Integrity evidence

- `hdiutil verify`: passed.
- Mach-O: arm64.
- Bundle version: `0.3.0`.
- Bundle identifier: `com.athena.core`.
- SHA-256 for the rebuilt local candidate: `946781fd5b5d33e88d75a1e74c379198f13deab695c56cf90eb5de620afbf98d`.

## Renderer smoke evidence

- `cd e2e-tests && SOAK_ITERATIONS=3 npm run test:soak`: passed.
- Final state: Dioxus mounted, loader hidden, root present, mounted control present, `ptyCount=3`, browser error list empty.
- This is limited renderer smoke evidence only; it does not replace the required 4–8 hour packaged PTY/memory/listener soak, clean-machine test, or signed-artifact validation.

## Not completed locally

This candidate is **unsigned and notarization-free**. It must not be published. The local keychain has no Developer ID signing identity and the shell has no Apple notarization credentials. The following remain required on the release runner:

- Developer ID Application signing.
- `codesign --verify --deep --strict --verbose=2`.
- Gatekeeper `spctl --assess`.
- Notarization and stapling.
- `xcrun stapler validate`.
- Clean Finder installation and launch.
- Clean-machine stability/upgrade/rollback evidence.

The authoritative release workflow is `.github/workflows/release-macos.yml`; it refuses public tag publication without signing credentials and publishes only after signed artifact verification.
