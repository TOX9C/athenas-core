# Changelog

All notable changes to Athena's Core. Format: grouped conventional-commit entries per release window; newest first. Release windows are seeded from git history; a release lands as a new `## [vX.Y.Z] — <date>` heading.

## [Unreleased]

## [v3.5.2] — 2026-10-24

### feat
- feat(resources): status-bar ResourcesPill — live per-app RAM + CPU readout (main + WebKit renderer + PTY shells) with hover per-process breakdown; renderer attributed via launch-time proximity under WebKit's XPC reparenting (ac4a8f2, 906c65b)

### fix
- fix(dialog): pre-warm the NSOpenPanel panel-service handshake at New Space modal open — first Browse no longer freezes the app ~5 s (macOS 27; see .plans/2026-10-22-browse-dialog-main-thread-freeze.md)
- fix(resources): resource_stats off the main thread (async + spawn_blocking) — sync commands run inline on the main thread under wry IPC
- test(commands): pin the first-run trust ordering contract (2026-10-19 plan §1) (cf345fa)

## [v3.5.1] — 2026-10-21

### fix
- fix(workspace): await trust before mounting terminal panes on first launch (6137ec3)
- fix(terminal): keep panes pinned across resize; no-op trust for already-trusted dirs (2295e15)
- fix(ci): register output_capture_set_consumer in build.rs; silence Linux-only cfg orphans (198656e)
- fix(ci): patch rustls-webpki 0.103.16 and webpki-roots 1.0.10 (RUSTSEC-2026-0381/0382) (0cb6efe)

### perf
- perf(build): compile the Dioxus frontend crate at opt-level 3; release logs at Info (e13a42a)
- perf(capture): gate output-capture:batch emission on registered consumers (bba3581)
- perf(scanner): needle-gate the resume-id search on the scan window (c9b6f42)
- perf(xterm): paint only dirty rows per output flush; dedup the viewport read (22fb460)
- perf(sandbox): cache the resolved workspace root after first lookup (8bfdbc5)
- perf(boot): load terminal vendor scripts on first pane mount (5f5012c)

### test
- test(e2e): new-terminal blackout regression spec (93b1744)
- test(e2e): restore trustDir definition dropped by 810238a (0e889e2)

## [v3.5.0] — 2026-10-19

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
- ci(clippy): count all compiler warnings, not just clippy::*; prune stale await_holding_lock baseline entry (8b5932f)
- ci: refresh terminal-exclusion TODO with re-enable path; fix stale workflow_dispatch version default (8b3ccee)
- docs: add CHANGELOG.md seeded from the 2026-10-01 → HEAD window (76b479e)
- fix(athena): fail fast with a busy error when a second turn queues behind ask_user's conversation lock (5f6e6f8)
- chore(release): bump version to 3.5.0 (2340aa8)

### Prior releases
- v3.4.4 — see ROADMAP.md dated sections (tap mirror fc8d37d; release shipped 2026-10-15).
- v3.4.0–v3.4.3 — see ROADMAP.md and git history.
