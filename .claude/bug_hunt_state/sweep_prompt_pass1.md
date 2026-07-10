You are running a single SWEEP PASS 1 of a sequential bug-hunt continuous agent loop over the athenas-core codebase.

## Your goal in this pass
Read source files across the WHOLE codebase and reason about bugs. Produce a queue of concrete, actionable findings. You are NOT fixing anything in this pass — only identifying and documenting issues.

## Scope (whole codebase)
- Rust: src-tauri/src/*.rs, crates/athena-{core,terminal,store,plugins,browser,fs}/src/**/*.rs
- Dioxus frontend: frontend/src/**/*.rs
- MCP server (TS): packages/mcp-server/src/**/*.ts
- Tests: crates/*/tests/*.rs, packages/mcp-server/test/*.ts, tests/*.ts, e2e-tests/*.mjs

## What counts as a finding (all severity classes — do NOT filter)
- Panics / unwraps that can explode on real input
- Logic bugs: wrong conditions, off-by-one, inverted comparisons, incorrect state transitions
- Resource leaks: unclosed handles, dropped guards, unbounded buffers
- Race conditions / deadlock / mutex-holding-across-await
- Error handling gaps: swallowed errors, lost context, wrong error propagation
- Unsafe misuse, UB, aliasing violations
- API contract violations: wrong JSON key casing for Tauri commands (never prefix params with _), misuse of tauri_bridge.rs
- Data corruption / data loss paths
- Performance footguns (avoidable clones in hot paths, O(n²) in loops)
- Dead code / unused imports (triage as low severity)
- Inconsistencies between frontend and backend command signatures

## Output format
Write findings to .claude/bug_hunt_state/findings_queue.jsonl — one JSON object per line:
{"id":"<stable-id>","file":"<path>","line":<n>,"severity":"high|med|low","category":"<category>","description":"<one paragraph: bug + why it's wrong + suggested fix>","status":"open"}

Stable id format: "<file-basename>:<line>:<category-slug>".

## Process
1. Use glob to map the source tree (use limit=400).
2. Read files in sections (use :offset:limit selectors) — avoid whole-file reads of the 148KB commands/mod.rs.
3. Reason about each file's invariants and failure modes. Prioritize the hot modules: orchestrator.rs, mcp.rs, tool_executor.rs, agent_comms.rs, commands/mod.rs, state.rs, athena-terminal session.rs.
4. Deduplicate against .claude/bug_hunt_state/fixed_findings.jsonl (already fixed in prior passes) — do not re-report an issue that's recorded there as fixed.
5. Commit your findings file. Do NOT edit any source file in this pass.

Stop the sweep when you've covered the whole tree. Quality over volume — each finding must be a real, fixable issue, not speculation.
