# Dependency Audit Evidence

**Audit date:** 2026-08-09  
**Scope:** npm production dependency graph and Cargo workspace lockfile  
**Status:** High-severity npm and Cargo vulnerability gates passed; residual maintenance/supply-chain warnings remain for review

## npm

The repository lockfile was regenerated with:

```bash
npm audit fix --package-lock-only --ignore-scripts
npm ci --ignore-scripts
npm audit --omit=dev --audit-level=high
```

The committed repository state now resolves the prior production-graph advisories. The resulting audit reported **0 vulnerabilities**. Updated transitive packages include `@hono/node-server` 2.1.0, `@modelcontextprotocol/sdk` 1.30.0, `body-parser` 2.3.0, `fast-uri` 3.1.5, `hono` 4.13.1, `ip-address` 10.4.0, `qs` 6.15.3, and `ws` 8.21.3. The workspace link for `@athenas-core/mcp-server` remains present.

Post-update evidence:

- [x] Repository `npm ci --ignore-scripts` succeeds.
- [x] `npm audit --omit=dev --audit-level=high` reports zero vulnerabilities.
- [x] `npm test` passes: 27 tests.
- [x] `npm run test:mcp` passes: 144 tests.
- [x] `npm run test:release-scripts` passes.
- [x] `npm run lint` passes with 0 errors; the existing 45 warnings remain cosmetic and are not silently treated as zero-warning clearance.
- [ ] Review the broad lockfile diff and confirm the release owner accepts the transitive upgrade set before tagging.

## Cargo

The repository was remediated with these lockfile-only updates:

- `plist` 1.9.0 → 1.10.0, bringing the Tauri path to `quick-xml` 0.41.0.
- `wayland-scanner` 0.31.10 → 0.31.11, removing the clipboard path's vulnerable `quick-xml` 0.39.4 instance.
- `tauri-plugin-log` 2.8.0 → 2.9.0, removing the `rkyv` 0.7.46 chain.
- `quinn-proto` 0.11.14 → 0.11.16.
- `anyhow` 1.0.102 → 1.0.104.
- `event-listener` 5.4.1 → 5.4.2.
- `memmap2` 0.9.10 → 0.9.11.
- `enumset` 1.1.12 → 1.1.14.
- `spin` 0.9.8 → 0.9.9.

Post-remediation evidence from the exact CI-pinned `cargo-audit 0.22.2` reports **zero vulnerabilities**. The high-severity advisories previously recorded here (`RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`, `RUSTSEC-2026-0185 / CVE-2026-25800`, and `RUSTSEC-2026-0235`) are no longer present in the audited lockfile.

The audit still reports 18 unmaintained/unsound transitive warnings. Four previously reported warnings were cleared by the patch updates above: `event-listener`, `memmap2`, `enumset`, and `spin`. The remaining warnings are not the same as the cleared vulnerability gate, but they require a separate supply-chain review and should not be silently waived. The release workflow installs the same pinned `cargo-audit 0.22.2` used for this evidence.

### Residual warning inventory

The following warnings remain open for the release owner and dependency maintainer. The default disposition is **do not expose new public attack surface, track an upstream replacement, and re-audit on every release**; none of these entries is an automatic vulnerability waiver.

| Warning group                     | Crates                                                                                                           | Current disposition                                                                                                                      | Owner / exit evidence                                                                           |
| --------------------------------- | ---------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------- |
| Unmaintained GTK3 bindings        | `atk`, `atk-sys`, `gdk`, `gdk-sys`, `gdkwayland-sys`, `gdkx11`, `gdkx11-sys`, `gtk`, `gtk-sys`, `gtk3-macros`    | Transitive platform/UI dependencies; retain only while required by the shipped target and document the upstream replacement path.        | Dependency owner; `cargo tree` path, target-scope review, and replacement/upgrade plan.         |
| Unmaintained parser/helper crates | `proc-macro-error`, `unic-char-property`, `unic-char-range`, `unic-common`, `unic-ucd-ident`, `unic-ucd-version` | Transitive build/runtime dependencies; no high-severity vulnerability in the pinned audit, but replacement should be tracked.            | Dependency owner; upstream status and next-version review.                                      |
| Unsoundness advisories            | `glib`                                                                                                           | Must receive a target/reachability review before publication; do not claim “no dependency risk” solely because `cargo audit` exits zero. | Security reviewer + dependency owner; written reachability/threat-model disposition or upgrade. |
| Cleared patch warnings            | `event-listener`, `memmap2`, `enumset`, `spin`                                                                   | Cleared by lockfile-only patch updates; retain the exact audit evidence and recheck on the release candidate.                            | Dependency owner; exact pinned audit output and lockfile review.                                |
| Broad lockfile change             | npm and Cargo transitive updates                                                                                 | Requires release-owner review before tagging; application tests passed after the updates.                                                | Release owner; approved lockfile diff attached to the release candidate.                        |

## Decision

The npm production vulnerability gate and Cargo high-severity vulnerability gate are green. Public distribution remains **NO-GO** for the independent reasons recorded in `SECURITY_REVIEW.md` and `RELEASE_CHECKLIST.md`: signed/notarized artifact evidence, clean-machine validation, packaged freeze/soak evidence, open plugin/trust-scope decisions, and named release approvals remain outstanding. Residual dependency maintenance warnings also require release-owner review before publication.
