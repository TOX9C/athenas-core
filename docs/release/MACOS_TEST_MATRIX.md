# macOS Production Test Matrix

Test the exact signed/notarized artifact intended for users, not only `cargo tauri dev`.

## Test environments

**Supported release target:** Apple Silicon arm64, macOS 13.0 or newer. The current candidate uses bundle identifier `com.athena.core`; test migration from any earlier `com.athena.app` installation separately.

Record each environment:

| Environment     | macOS | Chip               | Clean user data? | Existing dev tools? | Artifact     | Result                                                               |
| --------------- | ----- | ------------------ | ---------------- | ------------------- | ------------ | -------------------------------------------------------------------- |
| Clean install A |       | Apple Silicon      | Yes              | No                  | Signed DMG   | **PENDING** (needs signed artifact + clean Mac)                      |
| Upgrade A       |       | Apple Silicon      | No               | No                  | Signed DMG   | **PENDING** (needs signed artifact + prior `com.athena.app` install) |
| Developer smoke | 26.3  | Apple Silicon (M2) | No (dev data)    | Yes                 | Debug binary | ✅ **PASS** (see "Developer-machine results" below)                  |

## Installation and first launch

- [ ] Download completes and checksum matches.
- [ ] DMG opens without corruption warning.
- [ ] App drags to Applications.
- [ ] Finder launch succeeds.
- [ ] Gatekeeper accepts the notarized app.
- [ ] No terminal, Rust, Node, or repository is required.
- [ ] First-run notification permission behavior is correct.
- [ ] First-run API-key flow is understandable.
- [ ] App can be quit and launched again.

## Core workflow matrix

| Area          | Scenario                      | Expected result                 | Automated? | Manual evidence |
| ------------- | ----------------------------- | ------------------------------- | ---------- | --------------- |
| Workspace     | Create a workspace            | Workspace appears and persists  |            |                 |
| Workspace     | Open existing directory       | Correct directory is used       |            |                 |
| Terminal      | Spawn shell                   | Prompt appears                  |            |                 |
| Terminal      | Type command                  | Input reaches PTY               |            |                 |
| Terminal      | ANSI/true color               | Output renders correctly        |            |                 |
| Terminal      | Resize pane/window            | PTY dimensions update           |            |                 |
| Terminal      | Clipboard paste               | Bracketed paste works           |            |                 |
| Terminal      | Long output                   | UI remains responsive           |            |                 |
| Layout        | Add/remove panes              | No duplicate or orphan PTYs     |            |                 |
| Layout        | Drag/swap panes               | Labels/processes remain correct |            |                 |
| Panels        | Switch workspace/Kanban/Swarm | No freeze or blank render       |            |                 |
| Modals        | Open/close each modal         | No leaked listeners or dead UI  |            |                 |
| Chat          | Missing API key               | Actionable setup prompt         |            |                 |
| Chat          | Valid provider request        | Response completes              |            |                 |
| Chat          | Timeout/offline               | Retryable error                 |            |                 |
| Kanban        | Create/update/move/delete     | State persists                  |            |                 |
| Swarm         | Launch agents                 | Status and notifications update |            |                 |
| Agents        | Needs input/completion/error  | Badge and notification correct  |            |                 |
| Plugins       | Invalid manifest              | Safe rejection with explanation |            |                 |
| Plugins       | Enable/disable/remove         | Lifecycle cleans up             |            |                 |
| Browser       | Navigate/reload/back/forward  | Safe bounded behavior           |            |                 |
| Settings      | Change theme/font/provider    | Persists after restart          |            |                 |
| Notifications | Read/dismiss/clear            | Counts and history correct      |            |                 |

## Stability and recovery

- [ ] Repeat panel switching for at least 30 minutes.
- [ ] Repeat modal open/close for at least 30 minutes.
- [ ] Repeatedly mount/unmount xterm panes.
- [ ] Run long-lived PTYs while switching panels.
- [ ] Run agent output while opening modals.
- [ ] Minimize/restore during terminal output.
- [ ] Sleep/wake while PTYs and agents are active.
- [ ] Force-close the WebView/UI test harness and relaunch.
- [ ] Simulate or induce heartbeat silence in a test build.
- [x] Run the automated renderer smoke/soak with `cd e2e-tests && SOAK_ITERATIONS=10 npm run test:soak` and attach the reporter output/screenshots. **Result: PASS on developer Mac (debug binary), 10 lifecycle cycles, 0 errors — 2026-08-14.** This supplements, but does not replace, the required 4–8 hour packaged PTY/memory/listener soak.
- [ ] Confirm watchdog reload does not duplicate backend sessions.
- [ ] Confirm manual recovery remains available after automatic recovery fails.

## Data and upgrade

- [ ] Restart with active workspaces.
- [ ] Restart with active chat sessions.
- [ ] Restart after changing settings.
- [ ] Upgrade from previous beta data.
- [ ] Remove a trusted root and retry access.
- [ ] Corrupt a disposable test store and verify recovery guidance.
- [ ] Quit with active PTYs and verify resume behavior.

## Soak protocol

Recommended release-candidate soak: **4–8 hours** on at least one clean Apple Silicon machine.

Record every hour:

| Time | RSS memory | CPU | PTY count | Visible UI responsive? | Watchdog reloads | Errors | Notes |
| ---- | ---------: | --: | --------: | ---------------------- | ---------------: | -----: | ----- |
| 0h   |            |     |           |                        |                  |        |       |
| 1h   |            |     |           |                        |                  |        |       |
| 2h   |            |     |           |                        |                  |        |       |
| 4h   |            |     |           |                        |                  |        |       |
| 8h   |            |     |           |                        |                  |        |       |

**Pass criteria:** no reproducible freeze, no data loss, no unbounded PTY/listener growth, no unexplained crash, and all recovery actions remain usable.

## Developer-machine (pre-artifact) results — 2026-08-14

These were run on the development Mac (macOS 26.3, Apple M2, arm64) against the **debug binary**
(`target/debug/athenas-core`), **not** the signed/notarized artifact. They establish the
pre-artifact baseline; they do **not** satisfy the clean-machine or packaged-soak gates.

| Gate                                                             | Result            | Evidence                                                          |
| ---------------------------------------------------------------- | ----------------- | ----------------------------------------------------------------- |
| App launches + renders empty state                               | ✅ PASS           | `e2e-tests/test/specs/app-launch.e2e.mjs`                         |
| New Workspace click opens modal (was the WASM-panic repro)       | ✅ PASS           | same spec, assertion now hard                                     |
| 10-cycle lifecycle soak (panel switch / modal open / pane mount) | ✅ PASS, 0 errors | `e2e-tests/test/specs/release-soak.e2e.mjs`, `SOAK_ITERATIONS=10` |
| 12-pane geometry: non-overlapping, responsive, survives relayout | ✅ PASS           | `e2e-tests/test/specs/pane-scaling-10plus.e2e.mjs`                |

### Packaged (ad-hoc) informal soak — 2026-08-14

The release owner used the packaged (ad-hoc-signed) app on the development Mac for ~8–9 hours of
real use (panes, terminals, panels, modals, agents) with no freeze, crash, or dead control
reported. This is informal usage evidence on the ad-hoc artifact, **not** a substitute for the
structured 4–8 hour packaged soak on a clean machine, which remains pending the signed artifact.

**Not covered here (require the signed artifact / a clean Mac / a human):**
clean install, upgrade migration, Gatekeeper acceptance, Finder launch, notarization checks,
4–8h packaged soak, VoiceOver/a11y, and abuse soak of the packaged MCP/Agent-Comms/relay surfaces.
