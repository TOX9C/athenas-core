# Public Launch Security Review

**Status:** Active review — NO-GO for public launch  
**Reviewer:** `<name> <email>` — solo release author performing self-review with AI assistance (no independent human reviewer available)  
**Date:** 2026-08-08  
**Release/tag:** 0.3.0 candidate

This review covers the public-launch scope. It is not a substitute for a professional penetration test, but every unresolved high-severity finding blocks launch.

## Threat model

Athena's Core is a native developer application that can:

- Read/write trusted workspace files.
- Spawn shells, PTYs, and AI agents.
- Send prompts and workspace context to configured LLM providers.
- Load plugins and plugin-host processes.
- Run local MCP and Agent Comms services.
- Embed browser content.
- Optionally expose a LAN Mobile Mirror relay.

Assume a malicious workspace file, plugin, local process, LAN peer, malformed IPC request, compromised provider response, or renderer input may be attacker-controlled.

## Tauri command inventory

The evidence inventory is maintained in [`CAPABILITY_PLUGIN_INVENTORY.md`](./CAPABILITY_PLUGIN_INVENTORY.md). It records the command/build/permission counts, high-impact command groups, generated-permission drift, and the release findings.

For the final release candidate, attach the machine-generated comparison for every command in `src-tauri/src/main.rs`:

| Command | Capability | Caller | Inputs validated? | FS effect | Process effect | Network effect | Secret exposure | Confirmation? | Review |
| ------- | ---------- | ------ | ----------------- | --------- | -------------- | -------------- | --------------- | ------------- | ------ |
|         |            |        |                   |           |                |                |                 |               |        |

Verify the command list, build manifest, permissions, and frontend bridge remain consistent. Extra generated permissions must be removed or explicitly justified.

### Inventory findings

- [x] **F-1 / P0 implementation and focused tests complete; packaged validation pending:** `effective_roots()` cannot fall back to `/`; resolver, trusted-root, and standard bundle-layout tests pass. Real signed-artifact/Finder validation remains open.
- [x] **P-1 / P2 conditional:** Plugin host operations enforce state, manifest capabilities, event authorization, payload/config limits, and cleanup; caller-bound ownership remains a documented trusted-renderer limitation. Public untrusted-plugin support is excluded by `PLUGIN_TRUST_POLICY.md`.
- [x] **R-1 / P2 for this scope:** Mobile Mirror is experimental, disabled by default, excluded from public-launch guarantees, and normal startup requires explicit per-process activation; release-owner scope sign-off and artifact-level exclusion evidence remain open.
- [x] **C-1 / P2:** Broad default capability is accepted for the trusted bundled renderer; broad shell-execute permission is absent and `check-tauri-security-config.mjs` guards the CSP/debug boundaries. Feature-area capability splitting remains future work.
- [x] **C-2 / P2:** Stale `pty-listen-binary` permission removed; command/permission drift check added to CI.
- [x] **M-1 / P2 implementation and repository regression tests complete; artifact validation pending:** MCP has connection, rate, lifetime-request, line-size, idle-time, malformed-input, and per-connection auth coverage. Clean packaged-artifact abuse soak and reviewer disposition remain open.

See `CAPABILITY_PLUGIN_INVENTORY.md` for evidence paths and remediation order.

## Filesystem and trusted roots

Review:

- [ ] Absolute and relative path handling.
- [ ] Canonicalization before descendant checks.
- [ ] Symlink escape attempts.
- [ ] `..` traversal.
- [ ] Nonexistent leaf and parent creation.
- [ ] Validation-to-operation TOCTOU for sensitive filesystem commands (store snapshot durability is hardened; per-command race review remains open).
- [ ] Multiple trusted roots.
- [ ] Deleted/moved trusted roots.
- [ ] Bundled Finder launch with `/` or unrelated CWD.
- [ ] Every path-based filesystem command uses the same fail-closed validation policy; native file dialogs are reviewed separately.
- [x] Workspace-root resolution cannot fall back to `/` or another unrestricted root in the tested resolver paths; exact packaged Finder validation remains open.
- [ ] Error messages do not disclose unnecessary absolute paths.

## Tauri capabilities and CSP

- [ ] Each capability is required by a production feature; exact caller-by-caller review remains open.
- [x] Debug-only WebDriver code is source-gated and absent from release builds; the invariant is checked by `check-tauri-security-config.mjs`.
- [x] No broad shell-execute permission is present in the default capability; PTY/default-shell commands remain intentional high-impact APIs and exact production-use review remains open.
- [ ] Dialog, notification, and clipboard scopes are minimal; exact signed-artifact review remains open.
- [ ] Browser navigation accepts only parsed HTTP(S) URLs with a valid host, rejects credentials/control characters/non-web schemes, and has regression tests; implementation is covered, while exact production child-webview/navigation review remains open. Domain allowlisting remains a product-scope decision.
- [ ] CSP is tested against the production frontend assets; source invariants are checked by `check-tauri-security-config.mjs`, while packaged WebView enforcement remains open.
- [x] CSP source allowances are documented: `wasm-unsafe-eval` supports Dioxus WASM, inline styles support the bundled UI, `data:`/`blob:` support local assets, and `ipc:` is the only non-self connect source.
- [x] Generated command-aligned permissions and default capability entries are checked for drift in CI.

## Plugins

- [ ] Manifest schema validation is enforced before registration.
- [ ] Invalid/malicious manifests fail safely.
- [x] Plugin executable/source trust model is documented in `PLUGIN_TRUST_POLICY.md`.
- [x] Public launch decision: trusted developer integrations only; no marketplace, remote installation, provenance guarantee, or process sandbox is included.
- [x] Plugin session capabilities are intersected with agent defaults and manifest declarations; external-process sandboxing remains out of scope.
- [ ] Plugin environment-variable flow is documented for the bundled integrations: the setup passes the MCP bearer token to the deliberately configured local client process. General secret-leak prevention for arbitrary trusted plugin code is not provided.
- [ ] Plugin host commands authenticate the plugin/session and enforce ownership.
- [x] Plugin event subscriptions are limited to declared event types and bounded; untrusted-plugin isolation is explicitly out of scope.
- [x] Payloads, configuration, pending message parameters, subscriptions, and session counts are bounded.
- [x] Disable/remove cleans manager-owned sessions, subscriptions, and pending messages; external MCP/plugin processes are explicitly trusted and externally managed.
- [ ] `CAPABILITY_PLUGIN_INVENTORY.md` findings have owners and dispositions.

## MCP and Agent Comms

- [x] Bind only to intended local interfaces: MCP and Agent Comms bind to loopback; packaged relay remains separately excluded from public guarantees.
- [x] Authentication tokens are random and not hardcoded; MCP/Agent Comms use per-instance UUID tokens.
- [x] Tokens are not logged or returned through generic state APIs; `Debug` implementations redact them and event fallback logs are metadata-only.
- [x] Malformed JSON is rejected without panic, with a JSON-RPC parse-error response regression test.
- [x] Frame/message sizes are bounded, with an oversized TCP-line disconnect regression test.
- [x] Timeouts and connection limits exist and are covered by focused transport tests.
- [x] Agent Comms validates JSON-RPC version and bounds method, request, identity, status, and level fields; invalid requests with IDs receive `-32600` responses and TCP-level regression coverage.
- [x] Unauthorized tools/actions are rejected before per-connection `initialize`, with an integration test in `tests/mcp_auth.rs`.
- [x] Port conflicts are actionable and lifecycle tests cover retry/release behavior.
- [x] Shutdown releases MCP ports and active client state; Agent Comms lifecycle remains covered by focused tests.
- [ ] Sensitive output is scoped to the right client/session; local service and trusted-renderer boundaries still need packaged/artifact review.

## Mobile Mirror

Decision: EXPERIMENTAL / EXCLUDED FROM PUBLIC-LAUNCH GUARANTEES

The excluded status is a release-scope decision, not a claim that plaintext LAN transport is safe for public distribution.

The current evidence and risks are recorded in `CAPABILITY_PLUGIN_INVENTORY.md`. Plain HTTP/WebSocket on `0.0.0.0` plus file-write/PTY-control commands is not equivalent to a local-only feature; it requires an explicit LAN threat-model decision.

If included:

- [ ] Disabled by default.
- [ ] Explicit user confirmation before enabling.
- [ ] Strong token generation.
- [ ] Token rotation/revocation tested.
- [ ] Clear warning: trusted LAN only; do not expose publicly.
- [ ] Connection/client limits.
- [ ] Rate limits or abuse controls.
- [ ] Strict command allowlist.
- [ ] Event filtering tested against unrelated desktop panes.
- [ ] No unintended secret/file/session exposure.
- [ ] Relay shutdown and token invalidation tested.
- [ ] HTTP/WebSocket transport risk documented; TLS decision recorded.

## Secrets and privacy

- [ ] API keys use OS keychain in production.
- [ ] Legacy plaintext migration is bounded and documented.
- [ ] Generic store APIs cannot return secrets.
- [ ] Logs redact API keys, tokens, authorization headers, sensitive prompts, and URL credentials across the full release build. Audited native paths are guarded by `check-release-privacy.mjs`; this is not global release-build or diagnostic-export proof, which remains open.
- [ ] Session history does not retain credentials.
- [ ] Crash/diagnostic export redacts secrets.
- [ ] Provider data flow is documented.
- [ ] User consent and retention policy are documented.

## Findings

| ID  | Severity    | Area | Finding | Reproduction/evidence | Owner | Fix/mitigation | Status |
| --- | ----------- | ---- | ------- | --------------------- | ----- | -------------- | ------ |
|     | P0/P1/P2/P3 |      |         |                       |       |                |        |

## Sign-off

- [ ] No unresolved P0/P1 security findings.
- [ ] All accepted risks have an owner and mitigation.
- [ ] Public privacy/trust documentation is published.
- [ ] Reviewer approves this release scope.

**Security decision:** **NO-GO for notarized public launch** — signed/notarized artifact evidence is unavailable because the release owner has not enrolled in the paid Apple Developer Program (deferred by owner decision, not an open code finding). Packaged validations (clean-machine, abuse soak, Finder filesystem, capability scope) remain open and flow from that signing decision. Code-level review is complete; residual global redaction/diagnostic review and dependency-risk disposition remain under review. The high-severity Cargo vulnerability scan is remediated; residual dependency maintenance warnings remain under review.  
**Reviewer signature/date:** `<name> <email>` — self-review (solo) — `YYYY-MM-DD`
