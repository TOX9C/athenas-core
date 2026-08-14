# Public Launch Release Checklist

Use this checklist for every release candidate. A checked item must link to evidence: test output, artifact URL/hash, screenshot, review, or signed approval.

## Release identity

- [x] Release version is consistent in root/package/frontend/Tauri metadata (`0.3.0`; guarded by `check-release-identity`).
- [x] Product name and bundle identifier are final for this candidate: Athena's Core / `com.athena.core`.
- [x] Supported architecture and minimum macOS version are documented: Apple Silicon arm64, macOS 13.0+.
- [x] Bundle identifier decision recorded in `docs/release/BUNDLE_ID_DECISION_0.3.0.md`: clean reinstall from `com.athena.app` to `com.athena.core` for 0.3.0.
- [ ] Scope decisions are recorded in `PUBLIC_LAUNCH_PLAN.md`.
- [ ] Release owner and rollback owner are named.
- [ ] Release notes match the actual artifact and date.

## Repository and dependency hygiene

- [ ] Working tree contains only intended release changes.
- [ ] No secrets, certificates, private keys, tokens, or local databases are tracked.
- [ ] Lockfiles are present and intentional.
- [x] High-severity npm and Cargo vulnerability audits are run in CI and recorded in `docs/release/DEPENDENCY_AUDIT.md`; residual unmaintained/unsound/yanked dependency warnings remain a separate supply-chain review item.
- [ ] Generated artifacts are not accidentally included.
- [ ] Stale docs do not claim unavailable updater/platform features.

## Automated verification

Record the repository baseline before enforcing clean gates. At the current baseline, repository-wide Rustfmt/Prettier checks have pre-existing failures, Clippy has pre-existing warnings, and ESLint has warnings but no errors. The release workflow therefore runs a focused rustfmt check for touched MCP transport code; a public release must either clean the full baseline or attach an approved warning/format baseline and require zero new findings in touched/release-critical code.

- [x] `cargo fmt --all -- --check` — clean (2026-08-14).
- [x] `cargo check --workspace` — clean (2026-08-14).
- [x] `npm run check:clippy-baseline` — 0 warnings (2026-08-14).
- [x] `cargo test --workspace` — passes (2026-08-14); fixed a hanging stdio-loop EOF test, see note below.
- [x] `bash frontend/build-dist.sh` — succeeded (2026-08-14).
- [x] `npm test` — 29 passing (2026-08-14).
- [x] `npm run test:mcp` — 144 passing (2026-08-14).
- [x] `npm run test:release-scripts` (includes frontend watchdog, macOS artifact verifier, Tauri security-config invariants, and release privacy invariants)
- [x] Focused local release runner `npm run check:macos-release` passes repository identity, Tauri command/permission/security/privacy, plugin integration, and release-script checks; it accepts `--artifact <path>` for unsigned DMG structure/arm64 verification.
- [x] Deterministic 12-pane geometry/performance regression spec is present at `e2e-tests/test/specs/pane-scaling-10plus.e2e.mjs`; packaged soak remains a separate manual gate.
- [x] `npm run check:release-privacy` is wired into CI and guards audited native logging paths; it is not a substitute for full release-build/diagnostic-export review.
- [x] `npm run lint` — 0 errors, 45 pre-existing warnings (2026-08-14).
- [ ] `npm run format:check` — Prettier baseline: 60 files (vendored xterm, historical docs); rustfmt is now clean. Baseline pending release-owner approval.
- [x] `node scripts/check-tauri-command-drift.mjs` — 133 commands consistent (2026-08-14).
- [x] `node scripts/check-tauri-permission-drift.mjs` — 133 commands consistent (2026-08-14).
- [x] `node scripts/check-plugin-integration.mjs` — passed (2026-08-14).
- [ ] Dependency maintenance/unsoundness disposition recorded in `docs/release/DEPENDENCY_DISPOSITION_0.3.0.md`; verification evidence (`nm` and WASM symbol checks) attached to the release-candidate report.
- [x] `git diff --check` — clean (2026-08-14).

> **2026-08-14 regression fixed:** `cargo test --workspace` previously hung on
> `mcp::mcp_integration_tests::stdio_loop_uses_the_configured_tool_executor`.
> Root cause: the test dropped only the duplex `WriteHalf`, which never delivers
> EOF because `tokio::io::split` keeps both halves behind a shared `Arc`; the
> `ReadHalf` must be dropped too. Fixed in `crates/athena-core/src/mcp_integration_tests.rs`.

## Stability and core workflows

- [ ] Fresh launch reaches the welcome screen.
- [ ] New workspace opens and persists.
- [ ] Add, focus, resize, swap, and remove terminal panes.
- [ ] Terminal input, output, ANSI rendering, clipboard, resize, and long output work.
- [ ] Switch Workspace/Kanban/Swarm/Editor/Settings repeatedly.
- [ ] Open/close New Workspace, Swarm, Settings, input-request, and notification modals repeatedly.
- [ ] Chat onboarding blocks safely without an API key.
- [ ] Valid API key saves and persists after restart.
- [ ] Invalid key, timeout, offline, and provider errors show retryable UI.
- [ ] Kanban create/update/move/delete works.
- [ ] Swarm launch, agent status, attention state, and shutdown work.
- [ ] Plugin discovery/setup/error/removal behaves safely.
- [ ] Browser panel navigation/error/reload behavior is bounded.
- [ ] Watchdog heartbeat detects a simulated dead UI and recovery is usable, including a freeze with no recent user input.
- [x] Renderer smoke evidence: 10-cycle local soak passed (2026-08-14, debug binary, 0 errors) — recorded in `MACOS_TEST_MATRIX.md`; full packaged stability soak remains open (`LOCAL_UNSIGNED_CANDIDATE_0.3.0.md`).
- [ ] Reload preserves workspace/session state and does not duplicate PTYs/listeners.
- [ ] Sleep/wake, minimize/restore, and Cmd+Q behave correctly.

## Data integrity and migration

- [ ] Existing beta data is backed up before testing.
- [ ] Workspace data survives restart and upgrade.
- [ ] Chat sessions survive restart and upgrade.
- [ ] API key migration to OS keychain is verified.
- [ ] Corrupt/missing store data produces a safe recovery path.
- [x] `athena-store` snapshot writes use unique exclusive temp files, file/parent sync where supported, atomic replacement, and retryable dirty-state tracking; other filesystem writers and packaged interrupted-write behavior remain part of the artifact test matrix.
- [ ] Trusted-root deletion/recreation behaves safely.
- [ ] PTYs and resume metadata shut down/persist as designed.
- [ ] MCP/relay ports are released on normal and abnormal shutdown.

## Security sign-off

- [ ] Tauri command/capability inventory reviewed; command/permission drift and Tauri security-config invariants pass, but exact production caller/entitlement review remains open.
- [x] Filesystem path traversal, symlink, atomic-write race hardening, and fail-closed resolver behavior have focused regression coverage; exact packaged Finder validation remains open.
- [ ] `effective_roots` and bundled/Finder workspace-root behavior reviewed against the exact signed/notarized artifact.
- [x] Plugin trust boundary reviewed; `PLUGIN_TRUST_POLICY.md` limits public scope to trusted developer integrations and excludes sandboxed/untrusted plugins.
- [x] Plugin API docs now match the Rust manifest/validation/runtime contract; owner-aware session operations are atomic and covered by cross-plugin isolation tests. Legacy ID-only IPC cleanup remains trusted-internal compatibility surface, not a claim of end-to-end caller authentication.
- [x] MCP and Agent Comms loopback/authentication/limits reviewed with malformed-input and oversized-line regression coverage; packaged abuse soak remains open.
- [ ] Browser URL/navigation policy reviewed.
- [x] Mobile Mirror is excluded from public-launch guarantees, disabled by default, and normal startup requires explicit per-process activation; token/revocation, event filtering, and command allowlist implementation checks pass. Full LAN threat-model approval remains external.
- [ ] Secret/log/session/diagnostic redaction verified; release privacy invariants now guard the audited native log paths, but full release-build and diagnostic-export review remains open.
- [ ] No high-severity security finding remains.
- [ ] `SECURITY_REVIEW.md` is signed by the reviewer.

## UX, docs, and support

- [ ] First-run onboarding is understandable without developer context.
- [ ] Critical actions have loading/success/error/retry states.
- [ ] Error messages explain what happened and what to do next.
- [ ] System requirements and supported platforms are current.
- [x] Privacy/data-flow documentation is published (`PRIVACY_NOTICE.md` plus `PRIVACY_DATA_FLOW.md`).
- [ ] Plugin and relay trust warnings are visible.
- [ ] Bug-report path and diagnostic collection instructions are published.
- [ ] Known limitations are current.
- [ ] README, architecture, migration guide, and release notes do not contradict the release.
- [x] `docs/ARCHITECTURE.md` and `docs/MIGRATION_GUIDE.md` now state that no in-app updater is shipped; manual delivery remains conditional on release-owner approval and testing.
- [ ] `SUPPORT_RUNBOOK.md` and `INCIDENT_RESPONSE.md` are ready.

## macOS artifact

See [`MACOS_SIGNING_SETUP.md`](./MACOS_SIGNING_SETUP.md) for the credential-safe Developer ID and notarization setup.

- [x] Production entitlements are explicitly empty in `src-tauri/entitlements.plist`; final signed-artifact entitlement review remains open.
- [ ] App is built from a clean tagged revision.
- [ ] App bundle is Developer ID signed.
- [ ] Unsigned local DMG is created successfully; local evidence is recorded below, but signed/notarized DMG remains blocked on Apple credentials.
- [ ] Notarization succeeds.
- [ ] Ticket is stapled.
- [ ] `codesign --verify --deep --strict --verbose=2` passes.
- [ ] `spctl --assess --type execute --verbose` passes.
- [ ] `xcrun stapler validate` passes.
- [ ] Local unsigned DMG passes `hdiutil verify`; evidence: `docs/release/LOCAL_UNSIGNED_CANDIDATE_0.3.0.md` and `target/release/bundle/dmg/Athena's Core_0.3.0_aarch64.dmg`.
- [x] Local verifier can require exactly one `.app` and an arm64-capable executable (`--require-app --require-arm64`); negative architecture coverage is exercised by `scripts/test-verify-macos-artifact.mjs`.
- [ ] Local unsigned DMG checksum generated; observed candidate hash `946781fd5b5d33e88d75a1e74c379198f13deab695c56cf90eb5de620afbf98d`; publication checksum remains pending signed artifact. See `LOCAL_UNSIGNED_CANDIDATE_0.3.0.md`.
- [ ] Clean Apple Silicon Mac installs from the published DMG.
- [ ] Clean Mac launches from Finder without right-click/Open workaround.
- [ ] Uninstall/reinstall behavior is documented and tested.

## Updates and rollback

- [ ] Updater decision recorded for this release in `docs/release/UPDATER_DECISION_0.3.0.md` (ship or formally waive); owners named and dry-runs completed.
- [x] Emergency manual installation path is documented in `docs/release/MANUAL_UPDATE_RUNBOOK.md`.
- [ ] Update manifest endpoint is reachable.
- [ ] Update artifact signatures verify.
- [ ] Upgrade preserves user data.
- [ ] Invalid signature is rejected.
- [ ] Interrupted/offline update is recoverable.
- [ ] Downgrade/rollback procedure is tested.
- [x] Emergency manual installation path is documented in `docs/release/MANUAL_UPDATE_RUNBOOK.md`.

## Final approval

- [ ] No open P0/P1 defects.
- [ ] Release candidate report completed.
- [ ] Security reviewer approves.
- [ ] Validation owner approves.
- [ ] Release owner approves.
- [ ] Staged rollout cohort is defined.
- [ ] Monitoring and rollback are active.
- [ ] Public launch decision recorded.

**Release:**  
**Commit/tag:**  
**Artifact hash:**  
**Release owner:**  
**Decision:** **NO-GO** — high-severity vulnerability scans are green, but this repository is not cleared for public distribution until residual supply-chain warnings are reviewed, the signed/notarized artifact passes clean-machine validation, the packaged stability soak passes, trust-scope decisions are approved, and named release approvals are recorded.  
**Evidence links:** Pending release-candidate artifact and approvals.
