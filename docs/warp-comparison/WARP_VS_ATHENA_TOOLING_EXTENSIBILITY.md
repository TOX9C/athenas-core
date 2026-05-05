# Warp vs Athena's Core: Tooling & Extensibility Comparison

> Comprehensive diff of build systems, tooling, CI/CD, testing, linting, plugin architecture, API surface, customization, hooks, and telemetry between [warpdotdev/warp](https://github.com/warpdotdev/warp) and Athena's Core.

---

## 1. Language, Runtime & Architecture

| Dimension                     | Warp                                                                      | Athena                                                    |
| ----------------------------- | ------------------------------------------------------------------------- | --------------------------------------------------------- |
| Core language                 | Rust 98.2% (edition 2018)                                                 | TypeScript / JavaScript                                   |
| UI framework                  | Custom WarpUI — Entity-Component-Handle pattern, GPU-rendered via wgpu 29 | React 19 + Tailwind CSS 3.4 + CSS variable-driven theming |
| Runtime                       | Native binary (self-contained)                                            | Electron ^32 (Chromium ~128, Node ~20.x)                  |
| Terminal engine               | Custom VTE fork (`warpdotdev/vte`), PTY via `nix` crate                   | `node-pty` ^1.0 + `@xterm/xterm` ^5.5                     |
| Secondary compilation targets | WASM (`wasm32-unknown-unknown`), CLI binary (`oz`)                        | MCP server (standalone Node ESM package)                  |
| Windowing                     | `winit` (custom fork `warpdotdev/winit`)                                  | Electron BrowserWindow                                    |
| Rendering                     | wgpu (Metal / Vulkan / DX12 / GLES) + WGSL shaders                        | Chromium renderer (CSS/HTML)                              |
| Text shaping                  | `font-kit` (custom fork), `arborium` for syntax highlighting              | Monaco Editor, browser text rendering                     |

---

## 2. Build System

### 2.1 Build Tool & Monorepo Structure

| Dimension               | Warp                                                                                                                                     | Athena                                      |
| ----------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| Build tool              | Cargo workspace (resolver v2)                                                                                                            | electron-vite ^2.3.0 (Vite ^5)              |
| Workspace members       | 60+ crates in `crates/*` + `app/`                                                                                                        | 1 workspace package (`packages/mcp-server`) |
| Default members         | 10 crates (app, channel_versions, command, editor, graphql, markdown_parser, sum_tree, warpui, warp_completer, warp_terminal, warp_util) | Root only                                   |
| Lockfile                | `Cargo.lock` (committed, CI enforces `--locked`)                                                                                         | `package-lock.json` (standard npm)          |
| Dependency management   | 150+ workspace-level deps in `[workspace.dependencies]` with centralized versioning                                                      | Per-package.json deps (root + mcp-server)   |
| Patched upstream crates | 8 `[patch.crates-io]` entries (core-foundation, objc, pathfinder_simd, yaml-rust, tink, jemalloc)                                        | None                                        |

### 2.2 Build Profiles

| Profile                         | Purpose                                                                                    | Athena Equivalent                            |
| ------------------------------- | ------------------------------------------------------------------------------------------ | -------------------------------------------- |
| `dev`                           | Dev builds, `debug = "line-tables-only"`, `split-debuginfo = "unpacked"`                   | Vite dev server (HMR)                        |
| `release`                       | Production, `debug = 1` (line tables only for Sentry), `split-debuginfo = "packed"` (dSYM) | `electron-vite build` (Vite production mode) |
| `release-lto`                   | ThinLTO for better runtime perf                                                            | None                                         |
| `rlto`                          | Shorthand for `release-lto` (avoids Windows 255-char path limit)                           | None                                         |
| `rltoda`                        | `release-lto` + `debug-assertions = true`                                                  | None                                         |
| `release-cli`                   | CLI tarball: `opt-level = "s"`, `codegen-units = 1` for smaller binary                     | None                                         |
| `release-cli-debug_assertions`  | CLI + debug assertions for dev channel                                                     | None                                         |
| `release-wasm`                  | WASM target: `opt-level = "s"`, LTO = true, `codegen-units = 1`                            | None                                         |
| `release-wasm-debug_assertions` | WASM + debug assertions                                                                    | None                                         |
| `dev-remote`                    | Remote server dev: `strip = "symbols"` for faster rsync                                    | None                                         |
| `dev-wasm`                      | WASM dev: `opt-level = "s"`                                                                | None                                         |

Warp also has per-package `opt-level` overrides in `dev` profile for critical crates: `backtrace`, `pprof`, `jemalloc`, `ttf-parser`, `strsim`, `memchr`, `nom`, `tokio`, `rayon-core`, `image`.

**Athena has no custom build profiles** — it relies entirely on Vite's built-in dev/production modes.

### 2.3 Build Commands

| Action        | Warp                                                                 | Athena                                                                   |
| ------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------ |
| Dev           | `cargo run` or `./script/run`                                        | `npm run dev` (`electron-vite dev`)                                      |
| Build         | `./script/bundle --channel <channel> [--arch] [--artifact app\|cli]` | `npm run build` (`electron-vite build`)                                  |
| Preview       | N/A (run the binary directly)                                        | `npm run preview` (`electron-vite preview`)                              |
| Native deps   | Cargo handles natively                                               | `npm run postinstall` (`electron-builder install-app-deps` for node-pty) |
| WASM build    | `./script/wasm/bundle --channel oss`                                 | N/A                                                                      |
| Cross-compile | Separate arch builds then universal binary merge                     | Single-platform per `electron-builder.yml` target                        |

### 2.4 Compiler Flags

| Flag                    | Warp                                            | Athena         |
| ----------------------- | ----------------------------------------------- | -------------- |
| Symbol mangling         | `-C symbol-mangling-version=v0`                 | N/A            |
| Link args               | `-C link-args=-Wl,-headerpad_max_install_names` | N/A            |
| WASM unstable APIs      | `--cfg=web_sys_unstable_apis`                   | N/A            |
| macOS deployment target | `MACOSX_DEPLOYMENT_TARGET=10.14`                | Not configured |

---

## 3. Toolchain Pinning

| Dimension          | Warp                                                                               | Athena                                                                 |
| ------------------ | ---------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| Language version   | `rust-toolchain.toml`: Rust 1.92.0, components: rustfmt + clippy, profile: minimal | No Node version pinning — no `.nvmrc`, no `engines` field in root      |
| MCP server pinning | N/A                                                                                | `engines: { node: ">= 18.0.0" }` in `packages/mcp-server/package.json` |
| Electron version   | N/A                                                                                | `electron` ^32.0.0 (Node ~20.x)                                        |
| Platform minimum   | `MACOSX_DEPLOYMENT_TARGET=10.14`                                                   | Not configured                                                         |

---

## 4. Linting & Formatting

| Dimension             | Warp                                                                                                                                                                                   | Athena                                          |
| --------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------- |
| **Rust/TS linter**    | `cargo clippy --workspace --all-targets --all-features --tests -- -D warnings`                                                                                                         | **No ESLint — not installed, not configured**   |
| **Rust/TS formatter** | `cargo fmt` (`.rustfmt.toml`: edition 2018)                                                                                                                                            | **No Prettier — not installed, not configured** |
| C/C++/Obj-C formatter | `clang-format` via `script/run-clang-format.py`                                                                                                                                        | N/A                                             |
| WGSL shader formatter | `wgslfmt --check`                                                                                                                                                                      | N/A                                             |
| PowerShell linter     | PSScriptAnalyzer (`.PSScriptAnalyzerSettings.psd1` + custom rules `PSScriptAnalyzerCustomRules.psm1`)                                                                                  | N/A                                             |
| Disallowed APIs       | `.clippy.toml` bans: `std::dbg!`, `std::time::Instant`, `std::process::Command`, `async_process::Command`, `async_channel::Sender::send_blocking`, `LineEnding::from_current_platform` | No equivalent                                   |
| License compliance    | `cargo-deny` (`deny.toml`) with explicit allowlist (14 licenses), `check_license_config_sync` script                                                                                   | None                                            |
| Presubmit script      | `./script/presubmit` — runs fmt → clippy → clang-format → wgslfmt → PSScriptAnalyzer → nextest → doc tests                                                                             | **No presubmit script**                         |
| Pre-commit hooks      | None (enforced via CI + presubmit)                                                                                                                                                     | **None (no Husky, no lint-staged)**             |
| Editor config         | N/A                                                                                                                                                                                    | **No `.editorconfig`**                          |
| Commit convention     | Not enforced by tooling (documented in CONTRIBUTING.md)                                                                                                                                | **No commitlint**                               |

---

## 5. Testing

| Dimension                  | Warp                                                                                  | Athena                                                                                 |
| -------------------------- | ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| Test runner                | `cargo nextest` (parallel, JUnit XML, profiles)                                       | Vitest ^4.1.5 (global API mode)                                                        |
| Test environment           | 3-OS matrix (macOS, Linux, Windows) + WASM target                                     | Node only (`environment: 'node'`)                                                      |
| DOM/UI testing             | GPU-real-display integration tests (`WARPUI_USE_REAL_DISPLAY_IN_INTEGRATION_TESTS=1`) | **No jsdom/happy-dom, no component tests, no Playwright**                              |
| Shell integration tests    | bash (default + latest), fish, zsh, PowerShell — each as separate CI matrix entries   | N/A                                                                                    |
| SSH integration tests      | GCP-authed SSH tunnel tests (skipped for fork PRs)                                    | None                                                                                   |
| Integration framework      | Custom framework in `crates/integration/`                                             | None                                                                                   |
| Unit test placement        | `${filename}_tests.rs` or `mod_test.rs` alongside source, `#[cfg(test)]` gated        | `tests/` dir + `__tests__/` dirs                                                       |
| Doc tests                  | `cargo test --doc` (explicit, since nextest doesn't support these)                    | None                                                                                   |
| Feature-gated tests        | `warp_completer` tested with `--features v2` separately                               | None                                                                                   |
| Coverage tool              | None (no coverage config in Cargo)                                                    | V8 coverage provider, scoped to `electron/**/*.ts` + `packages/mcp-server/src/**/*.ts` |
| Coverage thresholds        | N/A (not configured)                                                                  | **None — coverage can silently degrade**                                               |
| Renderer (`src/`) coverage | Covered by integration tests                                                          | **Excluded from coverage entirely**                                                    |
| Test count                 | 60+ crates with tests + integration suite                                             | 15 test files                                                                          |
| Test analytics             | Trunk.io JUnit upload per test category (unit, integration per shell, per OS)         | None                                                                                   |
| xvfb                       | `coactions/setup-xvfb` for headless Linux/macOS GUI tests                             | N/A                                                                                    |

---

## 6. CI/CD

### 6.1 CI Infrastructure

| Dimension         | Warp                                                                                | Athena   |
| ----------------- | ----------------------------------------------------------------------------------- | -------- |
| CI platform       | GitHub Actions — **20+ workflow files**                                             | **None** |
| Main CI workflow  | `ci.yml` — 6 parallel job types with OS matrices                                    | N/A      |
| Draft PR handling | CI skipped for draft PRs; repo-sync PRs only run if merge-conflict label            | N/A      |
| Change detection  | `dorny/paths-filter` — skips DB migration / lint jobs when irrelevant files changed | N/A      |
| Rust cache        | `Swatinem/rust-cache` (save only on master)                                         | N/A      |
| CI concurrency    | Cancel in-progress runs for same PR (`cancel-in-progress: true`)                    | N/A      |

### 6.2 CI Jobs

| Job                         | Matrix                         | Purpose                                                                                                                   |
| --------------------------- | ------------------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| `params`                    | ubuntu-latest                  | Compute workflow parameters, change detection                                                                             |
| `tests`                     | macOS / Linux / Windows        | Unit tests, shell-agnostic integration, per-shell integration (bash ×2, fish, zsh, pwsh), doc tests, completions v2 tests |
| `lints`                     | macOS / Linux / Windows        | `cargo fmt --check`, `cargo clippy -D warnings`, `clang-format` (macOS only)                                              |
| `wasm-lint`                 | Linux large                    | `cargo fmt --check`, `cargo clippy --target wasm32-unknown-unknown`                                                       |
| `general-lint`              | ubuntu-latest                  | `cargo-deny` license check, license config sync, `wgslfmt`, PSScriptAnalyzer, repo-sync marker validation                 |
| `database-migration`        | ubuntu-latest (conditional)    | Run all Diesel migrations on empty DB, regenerate schema, diff against HEAD                                               |
| `check-release-compilation` | macOS / Linux / Windows / WASM | Verify `script/bundle --check-only` succeeds for all targets                                                              |
| `ci-result`                 | ubuntu-latest                  | Aggregator — single required status check                                                                                 |

### 6.3 Release Pipeline

| Dimension                  | Warp                                                                                                                                                         | Athena                                        |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------- |
| Release channels           | 3: `dev` (nightly, auto-push), `preview` (weekly), `stable` (weekly)                                                                                         | None                                          |
| Channel config             | `release_configurations.json` — per-channel Sentry project, GCS bucket, Slack channel, cache headers                                                         | N/A                                           |
| Release scheduling         | Cron daily at 08:00 UTC; weekly channels cut on Wednesday (`WEEKLY_RELEASE_DAY: 3`)                                                                          | None                                          |
| Release artifacts          | macOS: DMG (arm64 + x86_64 + universal), CLI tarballs; Linux: DEB, RPM, AppImage, Arch PKG, CLI tarballs (x86_64 + aarch64); Windows: NSIS; WASM: web bundle | macOS DMG, Windows NSIS, Linux DEB + AppImage |
| Code signing               | Apple Developer ID cert + notarization in CI                                                                                                                 | **None**                                      |
| GPG signing (Linux)        | PGP key from GCP Secret Manager for DEB/RPM/AUR                                                                                                              | None                                          |
| Distribution               | GitHub Releases + Google Cloud Storage (CDN with cache-control headers)                                                                                      | Local `electron-builder` output only          |
| Error reporting (releases) | Sentry — per-channel project + environment, debug symbols (dSYM/DWARF) uploaded automatically                                                                | None                                          |
| Dependabot                 | Cargo (security-only, limit 0) + GitHub Actions (14-day cooldown, grouped updates for `actions/*` and `namespacelabs/*`)                                     | None                                          |
| Feature flag cleanup       | Automated `feature_flag_cleanup.yml` workflow                                                                                                                | None                                          |
| PR automation              | Oz agent auto-review, `/oz-review` re-review (3x limit), stale PR cleanup, approval checks                                                                   | None                                          |
| Release candidate flow     | `cut_new_release_candidate.yml` for updating existing release branches                                                                                       | None                                          |
| Build cache                | `populate_build_cache.yml` for warming CI caches                                                                                                             | None                                          |

---

## 7. Database & Persistence

| Dimension         | Warp                                                                                              | Athena                            |
| ----------------- | ------------------------------------------------------------------------------------------------- | --------------------------------- |
| ORM               | Diesel (SQLite) with migrations in `crates/persistence/migrations/`                               | `electron-store` (JSON key-value) |
| Schema definition | `crates/persistence/src/schema.rs` (auto-generated by Diesel)                                     | N/A                               |
| Schema validation | CI job runs `diesel migration run` on empty DB, regenerates schema, diffs against HEAD            | None                              |
| Schema patching   | Custom `schema.patch` file for type overrides (`diesel.toml` `patch_file`)                        | N/A                               |
| FK optimization   | `allow_tables_to_appear_in_same_query_config = "fk_related_tables"` reduces generated trait impls | N/A                               |

---

## 8. Plugin Architecture & Extensibility

### 8.1 Plugin / Extension System

| Dimension                     | Warp                                                                                                                                | Athena                                                                                                                                                                                                   |
| ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| **Formal plugin API**         | No public plugin system in OSS repo                                                                                                 | No formal plugin system                                                                                                                                                                                  |
| **MCP server**                | `.mcp.json` configures GitHub MCP server for agent tooling                                                                          | Full MCP server implementation (`@athenas-core/mcp-server`) with: stdio + WebSocket transport, tool/transport/type sub-path exports, Zod-validated schemas, installable CLI binary (`athena-mcp-server`) |
| **MCP server publishability** | N/A — just a config reference                                                                                                       | Publishable npm package with `exports` map, `declaration` + `declarationMap`, `prepublishOnly` script                                                                                                    |
| **MCP proxy**                 | N/A                                                                                                                                 | `bin/mcp-proxy.js` — TCP proxy bridging stdio to MCP server socket (`ATHENA_MCP_PORT` / `ATHENA_MCP_HOST`)                                                                                               |
| **Agent integration**         | Built-in Agent Mode, external CLI agents (Claude Code, Codex, Gemini CLI), Oz cloud agent                                           | `athenaOrchestrator.ts` (Claude/OpenAI SDK), `swarmCoordinator.ts` for multi-agent                                                                                                                       |
| **Agent context files**       | `.agents/skills/` directory with `/write-product-spec` and `/write-tech-spec` skills; `WARP.md` as agent-readable engineering guide | `CLAUDE.md` as Claude Code instructions                                                                                                                                                                  |
| **Completion system**         | `warp_completer` with v2 JS-based completions (feature-gated, using `rquickjs` + `node_runtime`), Fig completion specs              | None                                                                                                                                                                                                     |
| **Custom commands**           | Command signatures via `command-signatures-v2` crate (JS-based)                                                                     | None                                                                                                                                                                                                     |

### 8.2 API Surface

| Dimension              | Warp                                                                                                  | Athena                                                                                                                                              |
| ---------------------- | ----------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| **IPC bridge**         | `crates/ipc/` — inter-process communication crate                                                     | `electron/preload.ts` — context isolation bridge, `window.athena` global with `athena.window`, `athena.fs`, `athena.pty`, `athena.store` namespaces |
| **GraphQL client**     | `crates/graphql/` + `crates/warp_graphql_schema/` (cynic codegen from schema.graphql)                 | None                                                                                                                                                |
| **HTTP server**        | `crates/http_server/` (axum 0.8.4)                                                                    | None                                                                                                                                                |
| **HTTP client**        | `crates/http_client/` (reqwest with rustls, HTTP/2, brotli, streaming)                                | `@anthropic-ai/sdk` ^0.91, `openai` ^6.34                                                                                                           |
| **WebSocket**          | `crates/websocket/` + `graphql-ws-client`                                                             | `ws` ^8.18 (in MCP server)                                                                                                                          |
| **JSON-RPC**           | `crates/jsonrpc/`                                                                                     | None (MCP uses its own protocol)                                                                                                                    |
| **Settings API**       | `settings` + `settings_value` + `settings_value_derive` crates with `schemars` JSON Schema generation | `electron-store` with `athena.store` IPC namespace                                                                                                  |
| **LSP**                | `crates/lsp/` — dedicated Language Server Protocol crate                                              | None                                                                                                                                                |
| **Virtual filesystem** | `crates/virtual_fs/` — virtual filesystem abstraction                                                 | `electron/fileSystem.ts` + `athena.fs` IPC namespace                                                                                                |
| **File watcher**       | `crates/watcher/` (custom `notify` fork)                                                              | `chokidar` ^5.0                                                                                                                                     |

### 8.3 Customization & Hooks

| Dimension                  | Warp                                                                                                                                                                                                   | Athena                                                                                                                                                                                                                   |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Feature flags**          | Runtime feature flags (`warp_features` crate) — `FeatureFlag` enum, `DOGFOOD_FLAGS` / `PREVIEW_FLAGS` / `RELEASE_FLAGS` sets, `FeatureFlag::X.is_enabled()` runtime checks, automated cleanup workflow | None                                                                                                                                                                                                                     |
| **Config file**            | `.warp/` directory (terminal configs, themes, launch configurations)                                                                                                                                   | `electron-store` persisted config via `athena.store`                                                                                                                                                                     |
| **Theme system**           | Custom WarpUI theming via settings system (JSON Schema-validated)                                                                                                                                      | CSS variable-driven theming (`var(--bg)`, `var(--accent)`, etc.) with Tailwind utility wrappers — enables runtime theme switching without rebuild                                                                        |
| **Keybindings**            | Settings-driven                                                                                                                                                                                        | Global key bindings in `App.tsx`                                                                                                                                                                                         |
| **Vim mode**               | Dedicated `crates/vim/` crate                                                                                                                                                                          | None                                                                                                                                                                                                                     |
| **Shell hooks**            | Shell integration scripts for bash/zsh/fish/pwsh (prompt markers, cursor positioning, command marking)                                                                                                 | PTY hooks via `ptyManager.ts` (stdout capture for AI ingestion)                                                                                                                                                          |
| **Startup hooks**          | `crates/onboarding/`                                                                                                                                                                                   | None                                                                                                                                                                                                                     |
| **Content security**       | Native app — no CSP needed                                                                                                                                                                             | CSP in `index.html`: `default-src 'self'; script-src 'self' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline' https://fonts.googleapis.com; font-src 'self' https://fonts.gstatic.com; worker-src 'self' blob:` |
| **Terminal model locking** | Strict deadlock prevention rules (documented in `WARP.md`) — single lock scope, pass locked references down                                                                                            | Zustand stores (no explicit lock model needed — single-threaded JS)                                                                                                                                                      |

---

## 9. Telemetry & Observability

| Dimension                  | Warp                                                                                                                                                                                    | Athena                                |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- |
| **Error reporting**        | Sentry — per-channel project (`warp-client-dev`, `warp-client-beta-stable`), per-channel environment, automatic release creation, debug symbol upload (dSYM for macOS, DWARF for Linux) | None                                  |
| **Logging**                | `crates/warp_logging/` + `crates/simple_logger/` + `sentry-log`                                                                                                                         | `console.log` (no structured logging) |
| **Profiling**              | `pprof` ^0.15 (CPU profiling), `jemalloc_pprof` (memory profiling), opt-level overrides for profiling crates                                                                            | None                                  |
| **Analytics**              | Trunk.io test analytics (JUnit XML upload per test category, OS, shell variant)                                                                                                         | None                                  |
| **Usage metrics**          | Firebase (`crates/firebase/`)                                                                                                                                                           | None                                  |
| **Crash handling**         | `sentry` crate with panic handler, backtrace, contexts, debug images                                                                                                                    | None                                  |
| **Performance monitoring** | `sysinfo` ^0.37 for system metrics                                                                                                                                                      | None                                  |
| **Build telemetry**        | CI timing per job/matrix, cargo build timing                                                                                                                                            | None                                  |

---

## 10. Collaboration & Workflow Features

| Dimension                     | Warp                                                                                                                                               | Athena                     |
| ----------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- |
| **Contribution model**        | Spec-first: `ready-to-spec` → product.md + tech.md under `specs/` → `ready-to-implement` → code PR. Bug fixes are implicitly `ready-to-implement`. | No contribution guidelines |
| **Automated review**          | Oz agent auto-assigned to PRs targeting ready issues; `/oz-review` for re-review (3x max); auto-requests SME after approval                        | None                       |
| **Issue triage**              | Oz automated triage; `@oss-maintainers` escalation; informational labels (`area:*`, `repro:*`)                                                     | None                       |
| **Changelog**                 | PR template with `CHANGELOG-NEW-FEATURE`, `CHANGELOG-IMPROVEMENT`, `CHANGELOG-BUG-FIX`, `CHANGELOG-IMAGE` prefixes                                 | None                       |
| **Branch conventions**        | `handle/feature-name` prefix (documented)                                                                                                          | None documented            |
| **PR template**               | `.github/pull_request_template.md` with structured sections                                                                                        | None                       |
| **Issue templates**           | `.github/ISSUE_TEMPLATE/` directory                                                                                                                | None                       |
| **Community**                 | Slack `#oss-contributors` channel, `/feedback` command in-app                                                                                      | None                       |
| **Security disclosure**       | Private reporting via `SECURITY.md`                                                                                                                | None                       |
| **Code of conduct**           | `CODE_OF_CONDUCT.md` (Contributor Covenant v2.1)                                                                                                   | None                       |
| **Cloud sync**                | Warp Drive (GraphQL-based, `warp_server_client` + `warp_graphql_schema`)                                                                           | None                       |
| **Voice input**               | `crates/voice_input/`                                                                                                                              | None                       |
| **Computer use**              | `crates/computer_use/`                                                                                                                             | None                       |
| **Session sharing**           | `session-sharing-protocol` (custom git dep)                                                                                                        | None                       |
| **Multi-agent orchestration** | `warp_multi_agent_api` (protobuf-based, custom git dep)                                                                                            | `swarmCoordinator.ts`      |

---

## 11. Packaging & Distribution

| Dimension                  | Warp                                                                                 | Athena                                                                            |
| -------------------------- | ------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------- |
| **macOS**                  | DMG (arm64, x86_64, universal) via `cargo-bundle` + `create-dmg`, Apple notarization | DMG via `electron-builder` (no notarization)                                      |
| **Linux**                  | DEB, RPM, AppImage, Arch PKG (x86_64 + aarch64), GPG-signed                          | DEB + AppImage via `electron-builder` (no signing)                                |
| **Windows**                | NSIS (non-one-click, custom install dir)                                             | NSIS (non-one-click, custom install dir)                                          |
| **CLI binary**             | `oz` CLI tarball per OS/arch                                                         | N/A                                                                               |
| **WASM**                   | Web bundle via `./script/wasm/bundle`                                                | N/A                                                                               |
| **CDN distribution**       | Google Cloud Storage with cache-control headers per channel                          | None                                                                              |
| **Auto-update**            | Channel-based via `channel_versions.json` (auto-push for dev channel)                | None (`electron-updater` not configured)                                          |
| **Native module bundling** | All native deps compiled by Cargo                                                    | `node-pty/build` explicitly bundled as `extraResources` in `electron-builder.yml` |
| **Code signing**           | Full Apple Developer ID + notarization + GPG for Linux packages                      | None                                                                              |

---

## 12. Full Difference Summary

### What Warp Has That Athena Doesn't

1. **Multi-OS CI pipeline** — 3-OS test/lint matrix with 20+ workflow files
2. **Scheduled multi-channel releases** — dev/preview/stable with per-channel Sentry, GCS, and Slack config
3. **Code signing + notarization** — Apple Developer ID cert, notarization, GPG for Linux
4. **`cargo clippy`** with disallowed API enforcement (`dbg!`, `std::time::Instant`, `std::process::Command`, etc.)
5. **`cargo-deny`** — license compliance auditing with explicit allowlist
6. **`cargo fmt`** — automated Rust formatting
7. **8 custom build profiles** — dev, release, release-lto, release-cli, release-wasm, and debug-assertion variants
8. **WASM compilation target** — full web build pipeline with dedicated formatter/linter
9. **Multi-shell integration testing** — bash (default + latest), fish, zsh, PowerShell as separate CI matrix entries
10. **GPU-real-display integration tests** — tests open actual windows with GPU rendering
11. **Sentry** — per-channel error reporting with debug symbol upload
12. **Feature flag system** — runtime `FeatureFlag` enum with dogfood/preview/release sets and automated cleanup
13. **Diesel ORM** — SQLite with CI-enforced migration validation and schema diffing
14. **GraphQL cloud sync** — Warp Drive with `cynic` codegen from `schema.graphql`
15. **LSP integration** — dedicated `crates/lsp/`
16. **Vim mode** — dedicated `crates/vim/`
17. **Voice input** — dedicated `crates/voice_input/`
18. **Computer use** — dedicated `crates/computer_use/`
19. **Custom UI framework** — WarpUI (MIT-licensed), Entity-Component-Handle pattern, GPU-rendered
20. **Oz agent** — automated issue triage, spec writing, PR review, and implementation
21. **Spec-first contribution model** — `product.md` + `tech.md` under `specs/`, readiness labels
22. **Dependabot** — security-only Cargo updates + grouped GitHub Actions updates with 14-day cooldown
23. **Profiling** — `pprof` CPU profiling, `jemalloc_pprof` memory profiling
24. **Toolchain pinning** — `rust-toolchain.toml` with exact version, components, and profile
25. **Cross-arch builds** — arm64 + x86_64 with universal binary merge

### What Athena Has That Warp Doesn't

1. **Full MCP server implementation** — publishable npm package with stdio + WebSocket transport, tool/transport/type exports, Zod validation, CLI binary
2. **MCP proxy** — TCP-to-stdio bridge (`bin/mcp-proxy.js`) for external tool integration
3. **Swarm coordinator** — `swarmCoordinator.ts` for multi-agent orchestration
4. **Context-isolated IPC bridge** — `preload.ts` with `window.athena` namespaces (`athena.window`, `athena.fs`, `athena.pty`, `athena.store`)
5. **CSS variable-driven theming** — runtime theme switching without rebuild (Tailwind utilities wrapping CSS custom properties)
6. **Monaco Editor** — built-in code editor via `@monaco-editor/react`
7. **Embedded browser views** — `browserManager.ts` for embedded browser panels
8. **Hot module replacement** — Vite HMR for renderer process during development
9. **Content Security Policy** — CSP defined in `index.html`
10. **Persistent JSON store** — `electron-store` with type-safe IPC access
11. **React component architecture** — component-based UI with `react-resizable-panels`, `@dnd-kit`, `lucide-react`
12. **V8 coverage provider** — test coverage instrumentation (though renderer is excluded)

### Where Both Are Similar

1. Both are terminal emulators with AI integration (Claude/OpenAI SDKs)
2. Both have agent-readable context files (`WARP.md` / `CLAUDE.md`)
3. Both build for macOS, Linux, and Windows
4. Both use PTY-based terminal emulation (node-pty vs nix crate)
5. Both have custom completion systems (Warp: `warp_completer`, Athena: not yet)
6. Neither has a formal public plugin/extension API
7. Neither uses pre-commit hooks (Husky/lint-staged) — Warp enforces via CI, Athena doesn't enforce at all
