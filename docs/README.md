# Documentation

This directory holds the **public, committed documentation** for Athena's Core —
the durable docs that ship with the repository and are read by users and
contributors on GitHub.

## Index

### Getting started & contributing

- [`CONTRIBUTING.md`](CONTRIBUTING.md) — development environment setup and contribution workflow
- [`development-guide.md`](development-guide.md) — how to build and run the app in development
- [`MIGRATION_GUIDE.md`](MIGRATION_GUIDE.md) — Electron → Tauri migration notes

### Architecture & systems

- [`ARCHITECTURE.md`](ARCHITECTURE.md) — high-level architecture and crate structure
- [`plugin-system-spec.md`](plugin-system-spec.md) — plugin & MCP system specification
- [`plugin-system-guide.md`](plugin-system-guide.md) — plugin system usage guide
- [`plugin-development.md`](plugin-development.md) — how to build plugins
- [`mcp-api-reference.md`](mcp-api-reference.md) — MCP server API reference

### Release & security (public record)

- [`release/RELEASE_SCOPE.md`](release/RELEASE_SCOPE.md) — supported platform and feature scope
- [`release/PRIVACY_NOTICE.md`](release/PRIVACY_NOTICE.md) — user-facing privacy notice
- [`release/PRIVACY_DATA_FLOW.md`](release/PRIVACY_DATA_FLOW.md) — where user data flows
- [`release/PLUGIN_TRUST_POLICY.md`](release/PLUGIN_TRUST_POLICY.md) — public plugin trust boundary
- [`release/SUPPORT_RUNBOOK.md`](release/SUPPORT_RUNBOOK.md) — support and diagnostic collection
- [`release/MANUAL_UPDATE_RUNBOOK.md`](release/MANUAL_UPDATE_RUNBOOK.md) — manual update and rollback
- [`release/MACOS_SIGNING_SETUP.md`](release/MACOS_SIGNING_SETUP.md) — Developer ID signing and notarization
- [`release/MACOS_TEST_MATRIX.md`](release/MACOS_TEST_MATRIX.md) — macOS production validation matrix

### Assets

`assets/`, `fonts/`, `generated-assets/`, `index.html`, `site.js`, and `styles.css`
power the docs site and store generated design assets — see
[`generated-assets/MANIFEST.md`](generated-assets/MANIFEST.md).

## What belongs in `docs/`

Only **durable, public** documentation:

- User-facing docs — privacy notice, install/update, support.
- Contributor docs — architecture, guides, API references.
- The **permanent record** of release scope and policy.

## What belongs in `.plans/` (never `docs/`)

Working plans, specs, agent handoffs, and one-off records are
**development-internal** and must stay out of the public repository. Put them in
`.plans/`, which is gitignored:

- Working plans, specs, and agent handoffs — `.plans/plans/`, `.plans/superpowers/`
- Release **decisions and evidence** whose outcome is already recorded in a public
  doc — e.g. `.plans/release/` (the public record lives in `docs/release/RELEASE_SCOPE.md`,
  `PRIVACY_NOTICE.md`, and `PLUGIN_TRUST_POLICY.md`)
- Marketing drafts, launch notes, and ad compositions — `.plans/marketing/`

Rule of thumb: **a plan, rationale, or evidence record is private; the resulting
public policy or user-facing notice is what belongs in `docs/`.** If a decision's
outcome is already captured in a `docs/release/` public record, commit only the
public record and move the decision/evidence doc to `.plans/`.

## Keeping plans private

`.plans/` is listed in the root `.gitignore`, so its contents are never committed.
Before committing, verify no `.plans/` path is staged:

```bash
git status --short | grep '^A.*\.plans/' || echo "no .plans files staged"
```

If a plan was accidentally written under `docs/`, move it to `.plans/` before
committing.
