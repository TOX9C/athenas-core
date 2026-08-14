# Dependency Audit Evidence — v0.3.0

**Date:** 2026-08-13
**Tooling:** `cargo-audit 0.22.2`
**Audit DB:** `last-updated 2026-08-12`
**Disposition:** [`DEPENDENCY_DISPOSITION_0.3.0.md`](./DEPENDENCY_DISPOSITION_0.3.0.md)

## Vulnerability gate

`cargo audit` (informational flags enabled):

```
vulnerabilities: found=false, count=0
warnings.unmaintained: 17 packages
warnings.unsound: 1 package
warnings.notice: 0
```

High-severity vulnerability gate: **GREEN**.

## MacOS binary symbol check (evidence pending packaged DMG)

Command to run after `cargo tauri build --bundles dmg`:

```bash
APP="target/release/bundle/macos/Athena's Core.app"
nm -gU "$APP/Contents/MacOS/Athena's Core" 2>/dev/null | grep -iE 'glib|gtk|atk|gdk' || echo "PASS: no GTK/glib/atk/gdk symbols"
```

Expected: `PASS: no GTK/glib/atk/gdk symbols`.

## Frontend WASM bundle check (run today, 2026-08-13)

```text
$ cd frontend/dist/assets && for f in athena-frontend_bg*.wasm; do
    echo "$f: $(stat -f%z "$f") bytes; bincode=$(strings "$f" | grep -c bincode); glib=$(strings "$f" | grep -c glib); gtk=$(strings "$f" | grep -c gtk)"
  done

athena-frontend_bg-dxhc9a46b34dcc39775.wasm: 2138702 bytes; bincode=0; glib=0; gtk=0
athena-frontend_bg.wasm: 2138702 bytes; bincode=0; glib=0; gtk=0
```

WASM check: **PASS** — no `bincode`, `glib`, or `gtk` strings in the production frontend bundle.

## Re-evaluation

Re-run `cargo audit` and re-attach this evidence for every release.
