# macOS Developer ID Signing Setup

This document is for release owners configuring the GitHub Actions macOS workflow. Never commit certificates, private keys, passwords, or notarization credentials to the repository.

## Required Apple prerequisites

- Paid Apple Developer Program membership.
- A **Developer ID Application** certificate with its private key.
- The Apple Developer Team ID.
- An Apple ID with an app-specific password, or an App Store Connect API-key-based notarization setup.
- A clean release commit/tag matching the version being built.

Tauri performs the macOS signing, notarization submission, and stapling during `cargo tauri build` when the required environment variables are available.

## GitHub Actions secrets

Configure these as encrypted repository or environment secrets:

| Secret                       | Value                                                                                            |
| ---------------------------- | ------------------------------------------------------------------------------------------------ |
| `APPLE_SIGNING_IDENTITY`     | Exact Developer ID Application identity, e.g. `Developer ID Application: Example, Inc. (TEAMID)` |
| `APPLE_CERTIFICATE`          | Base64-encoded `.p12` export containing the certificate and private key                          |
| `APPLE_CERTIFICATE_PASSWORD` | Password used when exporting the `.p12` file                                                     |
| `APPLE_ID`                   | Apple ID email used for notarization                                                             |
| `APPLE_PASSWORD`             | Apple app-specific password, not the normal Apple ID password                                    |
| `APPLE_TEAM_ID`              | 10-character Apple Developer Team ID                                                             |

Create the base64 certificate value locally without committing the source file:

```bash
openssl base64 -A -in DeveloperIDApplication.p12 -out certificate-base64.txt
```

Paste the contents of `certificate-base64.txt` into the `APPLE_CERTIFICATE` secret. Do not paste it into chat, an issue, or a repository file.

## Identity verification before publication

After configuring the secrets, use the release workflow's manual dispatch with `publish: false` first. The workflow will:

1. Create an isolated temporary keychain.
2. Import the Developer ID certificate and private key.
3. Verify that a signing identity is available.
4. Run the complete repository release checks.
5. Build the DMG through Tauri.
6. Let Tauri submit the artifact to Apple and staple the notarization ticket.
7. Verify the DMG checksum, app bundle, arm64 executable, code signature, Gatekeeper assessment, and stapled ticket.
8. Delete the temporary keychain and certificate file.

Do not enable public publication until the signed workflow run succeeds and the artifact passes clean-machine testing.

## Local identity check

A developer machine with the certificate installed can inspect identity names without exposing private material:

```bash
security find-identity -v -p codesigning
```

The expected identity begins with `Developer ID Application:`. A machine reporting `0 valid identities found` cannot perform local Developer ID signing.

## Release evidence to record

For the signed candidate, record in the release-candidate report:

- Tag and commit SHA.
- Exact signing identity.
- DMG SHA-256.
- `codesign --verify --deep --strict --verbose=2` result.
- `spctl --assess --type execute --verbose` result.
- `xcrun stapler validate` result.
- Apple Silicon clean-machine Finder install and launch result.

Unsigned artifacts are for internal testing only and must never be attached to a public release.
