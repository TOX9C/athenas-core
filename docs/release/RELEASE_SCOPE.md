# Release Scope — Athena's Core

**Status:** Current release scope (source of truth for supported product scope)
**Applies to:** public releases (v0.3.0 onward)

This document records the supported platform and feature scope of public
releases. It is the public, permanent record of the launch scope decisions;
working launch plans, gates, and owner assignments are development-internal
and are kept out of the public repository.

## Supported platform

- **Apple Silicon macOS first** — release builds target macOS 13.0+ on Apple
  Silicon (`aarch64`). Universal macOS builds are not in scope for the current
  release.
- Bundle identifier: `com.athena.core`.
- Public artifacts are signed and notarized DMGs.
- **Windows/Linux are out of scope** for this release and must not be
  advertised as released platforms.

## Feature scope

- **Mobile Mirror remains experimental and disabled by default.**
- Plugins are trusted developer integrations only — plugins are clearly marked
  as trusted-code functionality.
- **No in-app updater is shipped.** Updates are delivered manually (signed DMG
  plus published checksum) per `MANUAL_UPDATE_RUNBOOK.md`.
