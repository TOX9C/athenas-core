# Athena's Core

Native macOS workspace (terminal grid, AI chat, Kanban, agent swarm) — Tauri 2 + Rust + Dioxus. License: **BUSL-1.1** (source-available; commercial use requires a purchased license). Repo is public; also public: `TOX9C/homebrew-tap`.

## Dev loop

```bash
bash frontend/build-dist.sh   # rebuild frontend (release WASM is the ONLY build; --debug is ignored — Dioxus devtools websocket panics in WKWebView, so there is no frontend hot reload)
cargo tauri dev               # run the app; only Rust changes recompile live
```

- `dx` (dioxus-cli 0.7.9) must be at `~/.cargo/bin/dx` — a Homebrew `dx` shadows it in PATH.
- Rust-only changes: `cargo check --workspace` is the fast gate; `cargo test -p <crate>` per crate.
- CI runs `cargo test --workspace --exclude athena-terminal --locked` (the terminal crate's PTY fork kills Ubuntu runners — run it locally).
- Before pushing, mirror CI (ci.yml): `npm run lint`, `npm test`, `npm run test:mcp`, `npm run test:release-scripts`, `npm run check:plugin-integration`, `check:tauri-security`, `check:release-privacy`, and `node scripts/run-clippy-baseline.mjs` (clippy is a **no-regression baseline** — bare `cargo clippy` shows warnings CI tolerates). `check:tauri-commands`/`check:tauri-permissions` only run in the release workflow. No pre-commit hooks — CI is the only gate.

## Shipping a release

1. Bump the version in the 4 gated manifests: `package.json`, `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, `frontend/Cargo.toml` (`check:release-identity` enforces the match with the tag).
2. `git tag vX.Y.Z && git push origin vX.Y.Z` → `release-macos.yml` runs the full gate suite and publishes DMG + `.sha256` to a GitHub release if green. Tags already used: v3.3.0, v3.3.1 — never re-tag.
3. Update the tap (`TOX9C/homebrew-tap` → `Casks/athenas-core.rb`): bump `version` + `sha256` from the release's `.sha256` asset; confirm the asset name (GitHub normalizes `Athena's Core_…` to `Athena.s.Core_…` on upload — do NOT change the workflow's `--expected-name`, which validates the local artifact). Mirror the cask into this repo's `scripts/homebrew-tap/`.
4. Users install with `brew install --no-quarantine tox9c/tap/athenas-core` (app is unsigned: no Apple Developer account yet — that's a deliberate $ decision, not an oversight).

## Gotchas

- macOS 15+ install: System Settings → Privacy & Security → Open Anyway (right-click → Open no longer bypasses Gatekeeper).
- Pricing/store link lives in the README License section (placeholder until the Lemon Squeezy store exists). License key check is NOT implemented; when built: activate once, store locally, offline forever — never gate a running app on license status.
- Verbatim-legal files (`LICENSE`) must not diverge from the published BSL 1.1 text.

Rules: commit only files you changed — NEVER `git commit -am` / `-A`; stage explicit paths only (the working tree often carries unrelated in-flight work, and 2026-09-26 incident: a `-am` commit swept ~190 files of user WIP into a docs commit and pushed it public). Prefer `gh` CLI for GitHub operations (authenticated as TOX9C).
