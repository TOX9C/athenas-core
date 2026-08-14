# Dependency Maintenance and Unsoundness Disposition — v0.3.0

**Date:** 2026-08-13
**Scope:** macOS Apple Silicon production artifact (`aarch64-apple-darwin`)
**Tooling:** `cargo-audit 0.22.2`, `cargo tree` (workspace resolver)
**Reference:** [`DEPENDENCY_AUDIT.md`](./DEPENDENCY_AUDIT.md) — pre-disposition evidence

## Audit baseline (locked)

```
vulnerabilities: found=false, count=0
warnings.unmaintained: 17 packages
warnings.unsound: 1 package (glib@0.18.5)
warnings.notice: 0
database.last-updated: 2026-08-12
```

The 18 advisory packages are listed in [`DEPENDENCY_AUDIT.md`](./DEPENDENCY_AUDIT.md)
table "Residual warning inventory" plus one correction: `bincode@1.3.3`
(unmaintained) is also present; it is pulled in transitively by the
`athena-frontend` build via `gloo-worker`.

## Disposition table

| Package(s)                                                                                                                 | Class        | Target reachability                                                                                                                                                | 0.3.0 disposition                                                                                                                                                                                                                                                                                                     | Owner exit evidence                                                                                                                                       |
| -------------------------------------------------------------------------------------------------------------------------- | ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `glib@0.18.5`                                                                                                              | unsound      | **macOS: not in link graph.** Pulled in only by Linux `webkit2gtk` chain (`tauri → muda → gtk → atk → glib`). 0.3.0 ships macOS only.                              | **Accept** for 0.3.0. No macOS binary contains `glib` symbols; unsoundness is not exploitable in the public artifact. Track GTK4 migration in `tauri`/`wry` for any future Linux release.                                                                                                                             | Security reviewer: confirm macOS binary does not link `glib` via `nm -gU Athena's\ Core.app/Contents/MacOS/Athena's\ Core \| grep -i glib` returns empty. |
| `atk`, `atk-sys`, `gdk`, `gdk-sys`, `gdkwayland-sys`, `gdkx11`, `gdkx11-sys`, `gtk`, `gtk-sys`, `gtk3-macros`              | unmaintained | **macOS: not in link graph** (same GTK3 Linux-only path as `glib`).                                                                                                | **Accept** for 0.3.0. Track GTK4 upstream migration; remove when `wry` drops GTK3.                                                                                                                                                                                                                                    | Same `nm` check as above.                                                                                                                                 |
| `bincode@1.3.3`                                                                                                            | unmaintained | **macOS frontend build only.** Path: `athena-frontend → gloo → gloo-worker → bincode`. `gloo-worker` is not used at runtime; the chain is a dev-dep side effect.   | **Accept** for 0.3.0. The unmaintained crate does not run in the production WASM binary; it is a build-time serde helper. Replace when `gloo` 0.12+ lands or pin `bincode` away.                                                                                                                                      | Dependency owner: confirm `bincode` is not present in `frontend/dist/*.wasm` symbols.                                                                     |
| `proc-macro-error@1.0.4`                                                                                                   | unmaintained | **Build-time only.** Pulled in by a transitive `proc-macro` helper, never compiled into the binary.                                                                | **Accept** for 0.3.0. Build-time unmaintained crates are out of threat scope.                                                                                                                                                                                                                                         | No binary check needed.                                                                                                                                   |
| `unic-char-property@0.9.0`, `unic-char-range@0.9.0`, `unic-common@0.9.0`, `unic-ucd-ident@0.9.0`, `unic-ucd-version@0.9.0` | unmaintained | **Transitive at runtime.** Path: `tauri → tauri-utils → urlpattern → unic-ucd-ident → unic-*`. Bundled in the macOS binary as part of Tauri's URL pattern matcher. | **Conditional accept.** The 0.9 line is the actively maintained fork at the time of last review, but the upstream `unic` maintainer is unresponsive per the RustSec advisory. Accept for 0.3.0 with a tracking item: re-evaluate each release; remove when `tauri` upgrades `urlpattern` to a maintained alternative. | Dependency owner: track `tauri-utils` release notes; document any URL-pattern matcher change in the next dependency audit.                                |

## Replacement plan (track, do not block 0.3.0)

| Group                                           | Trigger to act                                                                        | Action                                                                                                    |
| ----------------------------------------------- | ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| GTK3 (`atk`, `gdk`, `gtk`, `gtk3-macros`, etc.) | Any of: future Linux release target, `wry` upstream GTK4 support, `tauri` drops GTK3. | Re-evaluate; remove the entire group once unsupported.                                                    |
| `glib` (unsound)                                | Same as GTK3.                                                                         | Remove when GTK3 is removed.                                                                              |
| `bincode`                                       | `gloo` upgrades to 0.12+ or drops `gloo-worker`.                                      | Update `frontend/Cargo.toml` to drop `gloo-worker` or pin `bincode` to a maintained version if one ships. |
| `unic-*`                                        | `tauri` upgrades `urlpattern` to a maintained URL pattern crate.                      | Re-run `cargo audit`.                                                                                     |
| `proc-macro-error`                              | Resolved naturally when the consuming crate upgrades.                                 | No action.                                                                                                |

## Verification evidence required before sign-off

- [ ] macOS production binary `nm` check: no `glib`, `gtk`, `atk`, `gdk`, `gtk3-macros` symbols present. Capture command and output in the release-candidate report.
- [ ] Frontend WASM bundle check: `bincode` not present. Capture `strings frontend/dist/assets/*.wasm | grep -i bincode` or equivalent.
- [ ] Re-run `cargo audit --json` and attach output to release-candidate report.

## Re-evaluation cadence

Every release. The next release must re-run `cargo audit`, compare the
warning set against this table, and update the disposition row by row.

## Sign-off

- [ ] Security reviewer confirms `glib` unsoundness is not reachable in the macOS artifact.
- [ ] Dependency owner confirms replacement plan entries and triggers.
- [ ] Release owner accepts this disposition.
