# Changelog

All notable changes to Athena's Core. Format: grouped conventional-commit entries per release window; newest first. Release windows are seeded from git history; a release lands as a new `## [vX.Y.Z] — <date>` heading.

## [Unreleased] — v3.5.0 window (2026-10-01 → HEAD)

### feat
- feat: license activation (Lemon Squeezy), plugin message drain, terminal charset/OSC52/OSC8, dead-command purge (6b8fe0a)
- feat(logs): frontend_log command — pipe webview console into backend log targets (e6ed563)
- feat(update): in-app update notification — GitHub release check, banner, About manual check (a05b7d5)
- feat: per-launch opt-in to skip Claude permission prompts, wired to pane bypass_mode (e147482)
- feat: os_permissions_status command reporting mic/speech/notification TCC state (485fb52)
- feat: OS permissions panel in Settings with live status and System Settings deep links (22826a0)
- feat(terminal): right-edge scrollbar on the active pane (ec573ed)
- feat(links): link chooser replaces in-pane browser panel navigation (13da1bf)

### fix
- fix(terminal): root-cause intermittent rendering bugs (fbf9411)
- fix(athena-panel): process-lifetime listeners (func_elem trap), drop stream-in blur (0372547)
- fix(build): cache-bust kitty addon — WKWebView serves stale addon build across restarts (c9b4b51)
- fix: launch_custom_agent allows the user's configured custom agents, not a dead env var (e7399a7)
- fix: launch errors name Athena's trust dialog, not macOS permissions (021639f)
- fix: review findings — permission rows decode from array (panel was dead), swarm error copy, bypass signal mode-leak (03e0b8a)
- fix: review findings — send Copy status enum out of UN callback (UAF contract), cfg voice-gate mappers, regen ACL schemas (64b4f3c)
- security: relay cannot write custom_agents (chat-launch allowlist) or arm pane bypass_mode; absolute /usr/bin/open (2dcf5bd)
- fix(ci): green CI — drop stale-binaryen wasm-opt pass, bump rustls for RUSTSEC-2026-0285 (b6f21c3)
- fix(terminals): fullscreen toggle activates + focuses the pane (734b4d6)
- fix(terminal): kitty overlay scan window covers straddling sentinel blocks (5fbd07e)
- fix(workspace): pane add/remove respects grid capacity (no gratuitous reflow) (3b21c97)
- fix(terminal): kitty overlays pair to blocks by shape, not index (6e1c2f7)
- fix(terminal): kitty overlays pair via sticky block anchors (F1 equal-width steal) (93b5793)
- fix(terminal): kitty sticky anchors survive reflow via content-proved forget (8a441b7)
- fix(terminal): R1 pair-gate survives buffer switch; R2 dual content+length forget (e6ad21d)
- fix(terminal): R1/R3 never-paired same-shape contest rule (equal-width steal) (9e8e5da)
- fix(terminal): R1/R3 unambiguity gate — never-paired images claim only when global contenders <= free same-shape blocks (eec612a)
- fix(terminal): R1/R3 rank-locked fresh pairing (equal-width unambiguity via global same-shape count) (cb738f0)
- fix(terminal): R3 forget resets paired so rank gate re-applies after reflow/trim (c1a30b0)
- perf(xterm): remount cover blits canvas instead of PNG-encoding on the main thread (442bfa5)
- perf(xterm): new-pane mounts join the settled shared atlas instead of clearing it (fa416cc)
- perf(pty): per-id spawn locks + drop outer mutex so SIGWINCH never queues behind fork (1bfd5fb)
- fix(terminal): keep the scroll position across pane remount and resize (775e4f4)
- fix(terminal): refuse spawn on empty cwd instead of forbidden /tmp fallback (46583f3)
- fix(notifications): skip macOS OS mirror in unbundled dev/e2e runs (4076d28)
- fix(e2e): activate the plugin patch and make window endpoints wait for the first window (d4f2307)

### test
- test(e2e): font-size shortcut + xterm glyph-coherence regression specs (dbff275)
- test: fix clippy field_reassign_with_default in allowlist test helper (dc2e6e8)
- test(e2e): extend kitty scroll harness — multi-image + stray-NBSP scenarios (08ff463)
- test(e2e): harness dump emits strayRows (review finding 1+2) (9b88b3c)
- test(e2e): seed trusted roots under a repo-local sandbox, not /tmp (26e3e23)
- test(e2e): sweep PTY orphans left by killed app instances (aef3a2a)
- test(e2e): route specs through trustViaProjectRoot instead of the add command (810238a)

### chore
- docs: tighten hero GIF (4.8s→16s cut, 3.9MB), brew-first install, OG fixes (04606d2)
- chore: refresh clippy baseline (4 warnings fixed in prior commits), CLAUDE.md license note (e587bc3)
- chore(release): bump version to 3.4.0 (a04c18d)
- chore(deps): npm audit fix — proxy-addr (critical IP-spoofing), MCP TS SDK (high), hono/fast-uri/ip-address (d10ac64)
- chore(tap): mirror cask 3.4.0 (0853114); fix deprecated depends_on macos string form (17b7b38)
- chore(release): bump version to 3.4.1 (1922d73, correcting 5ef1106); chore(tap): mirror cask 3.4.1 (cc4c4a1)
- fix(tap): restore mirror template header comments (2924189)
- chore(release): bump version to 3.4.2 (7a28163); chore(tap): mirror cask 3.4.2 (e4a6ffc)
- fix(ci): sync Cargo.lock member versions to 3.4.2 (fbedf15); fix(ci): release identity gate checks Cargo.lock member versions (c04209c)
- chore: untrack local-only review stubs, marketing render output, site drafts, AI-gen branding intermediates (28b0347)
- chore: drop dead UIState.fullscreen_pane_id (fabe833)
- chore(terminal): drop kitty addon debug residue (10-03 leftovers) (5eda455)
- chore(tap): mirror cask 3.4.4 (fc8d37d)
- ci: note why the scroll-regression spec needs the built dist (9244be0)

### 2026-10-18 release-readiness pass
- fix(build): complete relay/voice/xterm cfg-gating for minimal (--no-default-features) builds (ec46084)
- chore: delete dead ported-from-TS modules, unwired terminal-blocks store, orphan components, vestigial lib.rs, design mockup (02f4355)
- docs(roadmap): mark M5 capability toggles as planned-not-shipped; correct O2 claim and stale toolchain note (4bbc3a7)
- docs: truth-up release gates, signing-gate description, version/theme claims (91e875a)
- chore(logging): drop leftover console breadcrumbs, gate pill-drag log, route athena-store warnings through the log crate (7079a97)
- ci(clippy): count all compiler warnings, not just clippy::*; prune stale await_holding_lock baseline entry (8b5932f)
- ci: refresh terminal-exclusion TODO with re-enable path; fix stale workflow_dispatch version default

### Prior releases
- v3.4.4 — see ROADMAP.md dated sections (tap mirror fc8d37d; release shipped 2026-10-15).
- v3.4.0–v3.4.3 — see ROADMAP.md and git history.
