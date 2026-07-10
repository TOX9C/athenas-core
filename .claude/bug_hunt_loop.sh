#!/usr/bin/env bash
# bug_hunt_loop.sh — Sequential continuous agent loop for whole-codebase bug hunting.
#
# Branch of continuous-agent-loop skill: sequential
# Exit: stop on convergence (pass finds ≤ max(1, last×0.5) findings, or zero) OR regression
# Sweep: AI reasoning over files (OMP agent reads files, reasons about bugs)
# Verification gate: cargo clippy + cargo test --workspace (rust) / npm test + lint (ts)
# Delivery: one commit per finding on a bugfix branch, branched from main after committing dirty tree
# Scope: whole codebase — Rust (src-tauri + crates/*) + Dioxus frontend + MCP server (TS) + tests
# Budget: until convergence or regression
#
# Usage (from repo root):
#   bash .claude/bug_hunt_loop.sh              # interactive — spawns claude per pass
#   bash .claude/bug_hunt_loop.sh --dry-run     # sweeps + reports, applies no fixes, commits nothing
#   MAX_FIXES_PER_PASS=20 bash .claude/bug_hunt_loop.sh   # cap inner fix loop
#   MAX_PASSES=15 bash .claude/bug_hunt_loop.sh          # hard pass ceiling
#
# The script orchestrates passes. Each pass, it writes a fresh finding-queue file and
# hands it to claude with a scoped prompt that REQUIRES it to append JSONL lines to a
# known state path. The agent consumes the queue, fixes one finding, verifies, commits,
# repeats. Pass convergence is measured by the harness, not the agent.

set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "$REPO_ROOT"

DRY_RUN=0
[[ "${1:-}" == "--dry-run" ]] && DRY_RUN=1

# Tunables
MAX_FIXES_PER_PASS="${MAX_FIXES_PER_PASS:-20}"
MAX_PASSES="${MAX_PASSES:-15}"
MAX_FIX_ATTEMPTS="${MAX_FIX_ATTEMPTS:-3}"   # retries per finding if gate fails / queue stalls

STATE_DIR="$REPO_ROOT/.claude/bug_hunt_state"
mkdir -p "$STATE_DIR"

BRANCH_NAME="bughunt/$(date +%Y%m%d-%H%M%S)"
LOG="$STATE_DIR/loop.log"
QUEUE="$STATE_DIR/findings_queue.jsonl"
FIXED="$STATE_DIR/fixed_findings.jsonl"
PASS=0
LAST_PASS_FINDING_COUNT=""
CONVERGED=0

log() { printf '[%s] %s\n' "$(date -u +%FT%TZ)" "$*" | tee -a "$LOG"; }

# ── Phase 0: establish clean base ──────────────────────────────────────────────
# The baseline gate is NOT a hard abort. If main is red, the failures become
# seed findings in the queue (category baseline-clippy / baseline-test). The loop
# fixes those first, then transitions to sweep-based discovery passes. This keeps
# the "don't churn on pre-existing failures indistinguishable from your own"
# guarantee (seed ids are tagged baseline-* so they're trackable) while still
# having the loop DO its job: find and fix bugs.
setup_base() {
  log "Phase 0: establishing clean base on main"
  git checkout main 2>&1 | tail -1 || true

  # Commit the dirty tree as the branch base (user-confirmed).
  if [[ -n "$(git status --porcelain --untracked-files=no)" ]]; then
    log "  committing uncommitted working tree to main as bug-hunt base"
    if [[ $DRY_RUN -eq 1 ]]; then
      log "  [dry-run] would commit $(git status --porcelain --untracked-files=no | wc -l | tr -d ' ') dirty files"
    else
      git add -A
      git commit -m "chore: bug-hunt loop base — checkpoint working tree" >/dev/null
    fi
  fi

  # Baseline gate: capture a snapshot of pre-existing failures. These become seed
  # findings, not an abort reason. The post-fix gate (run_gate_delta) uses this
  # snapshot to ignore pre-existing failures and only fail on NEW ones.
  log "  running baseline verification gate on main"
  run_gate_baseline  # always runs; populates /tmp/bughunt-{clippy,test,*}.log + snapshot
  BASELINE_OPEN_COUNT="$(count_open_findings)"  # set by seed_baseline_failures if any
  if [[ "${BASELINE_OPEN_COUNT:-0}" -gt 0 ]]; then
    log "  baseline gate surfaced $BASELINE_OPEN_COUNT seed findings — loop will fix these first"
  else
    log "  ✓ baseline gate green — no seed findings"
  fi

  if [[ $DRY_RUN -eq 0 ]]; then
    git checkout -b "$BRANCH_NAME" 2>&1 | tail -1
    log "  created branch $BRANCH_NAME"
  else
    log "  [dry-run] would create branch $BRANCH_NAME"
  fi
}

# Seed baseline clippy/test failures into $QUEUE as open findings.
# Ids are tagged baseline-* so they're distinct from sweep ids (trackable, and
# excluded from the regression check unless gate-verified).
seed_baseline_failures() {
  log "  seeding baseline failures into queue"
  : > "$QUEUE"
  python3 - <<'PY'
import json, re, os
QUEUE = os.environ['QUEUE']
entries = []

def parse_clippy(path):
    """Extract (file, line, message) triples from cargo clippy JSON-ish output."""
    if not os.path.exists(path): return
    # cargo clippy emits messages like: error: <msg>\n  --> <file>:<line>:<col>
    text = open(path).read()
    # Split into error/warning blocks
    blocks = re.split(r'\n(?=error:|warning:)', text)
    for b in blocks:
        m_msg = re.match(r'^(error|warning):\s*(.+)', b)
        m_loc = re.search(r'^\s*-->\s* ([^:\s]+):(\d+):', b, re.M)
        if not (m_msg and m_loc): continue
        if m_loc.group(1).startswith('/'): continue  # skip std/system paths
        file = m_loc.group(1)
        line = int(m_loc.group(2))
        msg = m_msg.group(2).strip()[:300]
        rel = file
        entries.append({
            "id": f"baseline-clippy:{os.path.basename(file)}:{line}:{re.sub(r'[^a-z0-9]+','-',msg[:30].lower())}",
            "file": rel, "line": line,
            "severity": "high" if m_msg.group(1) == "error" else "low",
            "category": "baseline-clippy",
            "description": f"[baseline] clippy {m_msg.group(1)}: {msg}",
            "status": "open"
        })

def parse_test(path):
    if not os.path.exists(path): return
    text = open(path).read()
    # test failures: "test <name> ... FAILED" or "---- <name> stdout ----"
    for m in re.finditer(r'-(?P<name>[^\s]+)\s+FAILED|test (?P<name2>[^\s]+) \.\.\. FAILED', text):
        name = m.group('name') or m.group('name2')
        entries.append({
            "id": f"baseline-test:{name[:60]}",
            "file": "", "line": 0,
            "severity": "high", "category": "baseline-test",
            "description": f"[baseline] cargo test failure: {name}",
            "status": "open"
        })

parse_clippy('/tmp/bughunt-clippy.log')
parse_test('/tmp/bughunt-test.log')
# Dedup by id
seen = set()
with open(QUEUE, 'w') as f:
    for e in entries:
        if e['id'] in seen: continue
        seen.add(e['id'])
        f.write(json.dumps(e) + '\n')
print(len(entries))
PY
}

# Capture the baseline snapshot file (set of failing ids). run_gate_delta compares
# against this to ignore pre-existing failures.
capture_baseline_snapshot() {
  python3 - <<'PY'
import json, os
QUEUE = os.environ['QUEUE']
SNAP = os.environ['STATE_DIR'] + '/baseline_snapshot.jsonl'
ids = []
if os.path.exists(QUEUE):
    for l in open(QUEUE):
        l = l.strip()
        if not l: continue
        try: ids.append(json.loads(l).get('id',''))
        except: pass
with open(SNAP, 'w') as f:
    for i in set(ids): f.write(i + '\n')
PY
}

# ── Verification gates ─────────────────────────────────────────────────────────
run_rust_gate() {
  log "  verification: cargo clippy --workspace --all-targets"
  if ! cargo clippy --workspace --all-targets -- -D warnings >/tmp/bughunt-clippy.log 2>&1; then
    log "  ✗ clippy failed — see /tmp/bughunt-clippy.log"
    return 1
  fi
  log "  verification: cargo test --workspace"
  if ! cargo test --workspace >/tmp/bughunt-test.log 2>&1; then
    log "  ✗ cargo test failed — see /tmp/bughunt-test.log"
    return 1
  fi
  log "  ✓ rust gate passed"
  return 0
}

run_ts_gate() {
  log "  verification: vitest run (npm test)"
  if ! npm test >/tmp/bughunt-vitest.log 2>&1; then
    log "  ✗ vitest failed — see /tmp/bughunt-vitest.log"
    return 1
  fi
  log "  verification: eslint (npm run lint)"
  if ! npm run lint >/tmp/bughunt-eslint.log 2>&1; then
    log "  ✗ eslint failed — see /tmp/bughunt-eslint.log"
    return 1
  fi
  log "  ✓ ts gate passed"
  return 0
}

# Baseline gate runs FULL rust + ts regardless of what changed. It NEVER aborts —
# it seeds every failure into $QUEUE as an open finding and captures the snapshot
# of failing ids for the delta gate. Returns 0 unconditionally (the loop proceeds).
run_gate_baseline() {
  run_rust_gate || true
  run_ts_gate || true
  seed_baseline_failures
  capture_baseline_snapshot
  return 0
}

# Snapshots the CURRENT failing ids (whatever's in the clippy/test logs right now).
capture_current_failures() {
  python3 - <<'PY'
import json, re, os
QUEUE_TMP = os.environ['STATE_DIR'] + '/current_failures.tmp'
entries = []
def parse_clippy(path):
    if not os.path.exists(path): return
    text = open(path).read()
    blocks = re.split(r'\n(?=error:|warning:)', text)
    for b in blocks:
        m_msg = re.match(r'^(error|warning):\s*(.+)', b)
        m_loc = re.search(r'^\s*-->\s* ([^:\s]+):(\d+):', b, re.M)
        if not (m_msg and m_loc): continue
        if m_loc.group(1).startswith('/'): continue
        msg = m_msg.group(2).strip()[:60]
        entries.append(f"baseline-clippy:{os.path.basename(m_loc.group(1))}:{m_loc.group(2)}:{re.sub(r'[^a-z0-9]+','-',msg.lower())[:30]}")
def parse_test(path):
    if not os.path.exists(path): return
    text = open(path).read()
    for m in re.finditer(r'-(?P<name>[^\s]+)\s+FAILED|test (?P<name2>[^\s]+) \.\.\. FAILED', text):
        entries.append(f"baseline-test:{(m.group('name') or m.group('name2'))[:60]}")
parse_clippy('/tmp/bughunt-clippy.log')
parse_test('/tmp/bughunt-test.log')
with open(QUEUE_TMP, 'w') as f:
    for i in set(entries): f.write(i + '\n')
PY
}

# Delta gate: run the tools, then fail ONLY on failures NOT present in the baseline
# snapshot. Pre-existing failures (recorded at setup) are acknowledged and ignored;
# only NEW failures introduced by the current fix cause this gate to fail.
run_gate() {
  local changed_files
  changed_files="$(git diff --name-only HEAD || true)"
  local new_failures=0

  log "  verification: cargo clippy --workspace --all-targets (delta)"
  cargo clippy --workspace --all-targets -- -D warnings >/tmp/bughunt-clippy.log 2>&1 || true
  log "  verification: cargo test --workspace (delta)"
  cargo test --workspace >/tmp/bughunt-test.log 2>&1 || true
  if echo "$changed_files" | grep -qE '\.(ts|tsx)$'; then
    log "  verification: npm test (delta)"
    npm test >/tmp/bughunt-vitest.log 2>&1 || true
    log "  verification: npm run lint (delta)"
    npm run lint >/tmp/bughunt-eslint.log 2>&1 || true
  fi

  # Compare current failure ids against baseline snapshot
  capture_current_failures
  SNAP="$STATE_DIR/baseline_snapshot.jsonl"
  CURR="$STATE_DIR/current_failures.tmp"
  if [[ ! -f "$SNAP" ]]; then : > "$SNAP"; fi
  if [[ ! -f "$CURR" ]]; then : > "$CURR"; fi
  # New = in current but not in baseline
  new_failures="$(comm -13 <(sort -u "$SNAP") <(sort -u "$CURR") | grep -c '' || echo 0)"
  if [[ "$new_failures" -gt 0 ]]; then
    log "  ✗ delta gate failed — $new_failures NEW failures introduced by this fix:"
    comm -13 <(sort -u "$SNAP") <(sort -u "$CURR") | tee -a "$LOG"
    return 1
  fi
  log "  ✓ delta gate passed (no new failures vs baseline)"
  return 0
}

# ── Agent launcher ──────────────────────────────────────────────────────────────
# Resolves to `claude` by default. Override with AGENT_BIN env var if using a different
# OMP agent binary. The prompt MUST instruct the agent to write to absolute state paths.
AGENT_BIN="${AGENT_BIN:-claude}"
AGENT_ARGS="${AGENT_ARGS:--p --dangerously-skip-permissions}"

spawn_agent() {
  local prompt_file="$1"
  if [[ $DRY_RUN -eq 1 ]]; then
    log "  [dry-run] would spawn $AGENT_BIN with prompt: $prompt_file"
    return 0
  fi
  # shellcheck disable=SC2086
  $AGENT_BIN $AGENT_ARGS "$(cat "$prompt_file")" 2>&1 | tee -a "$LOG" || true
}

# ── Sweep prompt ───────────────────────────────────────────────────────────────
# The OMP agent does AI reasoning over files. This prompt scopes each sweep pass and
# MUST contain an explicit instruction to write to the absolute QUEUE path, so the
# harness can verify the file changed.
write_sweep_prompt() {
  local out="$1"; local pass="$2"
  cat > "$out" <<PROMPT
You are running sweep pass $pass of a sequential bug-hunt continuous agent loop over the athenas-core codebase.

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

## Output contract — REQUIRED
You MUST write your findings as JSONL to this exact path (overwrite the file):
  $STATE_DIR/findings_queue.jsonl

One JSON object per line, no trailing comma, no wrapping array. Schema:
  {"id":"<stable-id>","file":"<path>","line":<n>,"severity":"high|med|low","category":"<category>","description":"<bug + why wrong + suggested fix>","status":"open"}

Stable id format: "<file-basename>:<line>:<category-slug>".

If you find zero issues, write an empty file (truncate to zero bytes). Do not omit the file — the harness checks it exists.

## Deduplication
Do not re-report any finding whose id appears in this file (already fixed in prior passes):
  $STATE_DIR/fixed_findings.jsonl

## Process
1. Use glob to map the source tree (use limit=400).
2. Read files in sections (use :offset:limit selectors) — avoid whole-file reads of the 148KB src-tauri/src/commands/mod.rs.
3. Reason about each file's invariants and failure modes. Prioritize the hot modules: orchestrator.rs, mcp.rs, tool_executor.rs, agent_comms.rs, commands/mod.rs, state.rs, athena-terminal session.rs.
4. Write the JSONL file as the FINAL action of your run.

Stop the sweep when you've covered the whole tree. Quality over volume — each finding must be a real, fixable issue, not speculation. Do NOT edit any source file in this pass.
PROMPT
}

# ── Fix prompt (per finding) ───────────────────────────────────────────────────
write_fix_prompt() {
  local out="$1"; local finding="$2"; local attempt="$3"
  cat > "$out" <<PROMPT
You are consuming ONE finding from the bug-hunt queue and fixing it. This is attempt $attempt of $MAX_FIX_ATTEMPTS for this finding.

## Finding (JSON)
$finding

## Your task
1. Read the file at the finding's location. Confirm the bug is real by reading enough surrounding context — do NOT blindly apply the suggested fix if the description is inaccurate.
2. If the finding is invalid (not a real bug, or already fixed), update its status to "rejected" in this file with a one-line reason appended:
     $STATE_DIR/findings_queue.jsonl
   (rewrite that one line, keeping the id, changing status to "rejected" and adding a "reason" field). Do NOT edit source. STOP.
3. If valid, apply the minimal correct fix. Follow existing code conventions. Do not reformat or refactor surrounding code.
4. Run the verification gate you invoke from the project root:
     bash .claude/bug_hunt_loop.sh --gate-only
   This runs cargo clippy + cargo test (+ npm test + lint if .ts/.tsx changed) and compares against the captured baseline snapshot. It returns 0 (pass) only if NO NEW failures were introduced. Pre-existing failures from before the loop started are acknowledged and ignored.
   - The gate MUST return 0. If it returns non-zero, iterate the fix. If you cannot make it pass after $MAX_FIX_ATTEMPTS attempts, revert your changes (git checkout the file) and update the finding's status to "blocked" in the queue file with a "reason" field. STOP.
5. Once the gate passes, commit with message:
     fix(<scope>): <one-line description>
   where scope is the crate or module name (athena-core, commands, frontend, mcp-server, etc.). Put the finding id on the second line of the commit body.
6. Update the finding's status to "fixed" in this file:
     $STATE_DIR/findings_queue.jsonl
   AND append the fixed finding (with status "fixed" AND a "gate_pass": true field) to this file:
     $STATE_DIR/fixed_findings.jsonl
7. STOP. The loop harness will hand you the next finding.

## Output contract
You MUST update $STATE_DIR/findings_queue.jsonl (the finding's status) and (on success) append to $STATE_DIR/fixed_findings.jsonl with gate_pass:true. The harness verifies these files changed AND runs a dedup pass by id, so do not emit duplicate-id rows — rewrite the existing row in place.

## Constraints
- One finding at a time. One commit per finding.
- Never suppress a warning or test failure to make the gate pass — fix the root cause.
- Never edit files outside the finding's scope unless the gate demands it (e.g., a dependent callsite).
- Never run project-wide test suites beyond the gate command above.
}

# ── Helpers ──────────────────────────────────────────────────────────────────────
queue_line_count() {
  [[ -f "$QUEUE" ]] || { echo 0; return; }
  local n
  n="$(grep -c '' "$QUEUE" 2>/dev/null || echo 0)"
  echo "$n"
}

count_open_findings() {
  [[ -f "$QUEUE" ]] || { echo 0; return; }
  grep -c '"status":"open"' "$QUEUE" 2>/dev/null || echo 0
}

# Regression: a finding marked fixed AND gate_pass:true in fixed_findings.jsonl
# reappears as open (any status) in the current sweep's queue. A finding that was
# "fixed" but never gate-verified is NOT a regression if it reappears — it's the
# sweep correctly re-listing an incompletely-fixed issue.
check_regression() {
  [[ -f "$FIXED" ]] || return 1
  [[ -f "$QUEUE" ]] || return 1
  local reopened_ids
  # IDs that are gate-passed-fixed AND present in the current queue (re-emitted)
  local gate_fixed_ids queue_ids
  gate_fixed_ids="$(python3 -c "
import json,sys
ids=set()
try:
  for l in open('$FIXED'):
    l=l.strip()
    if not l: continue
    o=json.loads(l)
    if o.get('status')=='fixed' and o.get('gate_pass') is True:
      ids.add(o['id'])
except Exception: pass
print('\n'.join(sorted(ids)))
" 2>/dev/null)"
  queue_ids="$(grep -o '"id":"[^"]*"' "$QUEUE" | sed 's/"id":"//;s/"//' | sort -u)"
  reopened_ids="$(comm -12 <(echo "$gate_fixed_ids" | sort -u) <(echo "$queue_ids"))"
  if [[ -n "$reopened_ids" ]]; then
    return 0  # regression
  fi
  return 1
}

# Verify the queue file actually changed after a sweep (agent honored the contract).
verify_sweep_output() {
  if [[ ! -f "$QUEUE" ]]; then
    log "  ✗ sweep produced no queue file — agent did not honor output contract"
    return 1
  fi
  return 0
}

# ── Main loop ──────────────────────────────────────────────────────────────────
main() {
  : > "$LOG"
  : > "$FIXED"   # cumulative across passes; start empty
  log "=== BUG HUNT LOOP START ==="
  log "branch=$BRANCH_NAME dry_run=$DRY_RUN max_fixes_per_pass=$MAX_FIXES_PER_PASS max_passes=$MAX_PASSES max_fix_attempts=$MAX_FIX_ATTEMPTS agent=$AGENT_BIN"
  setup_base

  while true; do
    PASS=$((PASS + 1))
    if [[ $PASS -gt $MAX_PASSES ]]; then
      log "  ⚑ hit MAX_PASSES=$MAX_PASSES ceiling — stopping to avoid unbounded runtime"
      break
    fi
    log "--- PASS $PASS: sweep ---"

    # Fresh sweep: reset the queue for this pass (FIXED is cumulative, preserved)
    : > "$QUEUE"

    SWEEP_PROMPT="$STATE_DIR/sweep_prompt_pass${PASS}.md"
    write_sweep_prompt "$SWEEP_PROMPT" "$PASS"
    spawn_agent "$SWEEP_PROMPT"

    # Verify agent honored the output contract
    if ! verify_sweep_output; then
      log "  aborting pass $PASS — no queue file. Re-running sweep once."
      spawn_agent "$SWEEP_PROMPT"
      verify_sweep_output || {
        log "  ✗ sweep still produced no queue after retry — treating as zero findings and converging"
        PASS_FINDINGS=0
        OPEN_COUNT=0
      }
    fi

    OPEN_COUNT="$(count_open_findings)"
    PASS_FINDINGS="$(queue_line_count)"
    log "  pass $PASS surfaced $PASS_FINDINGS findings ($OPEN_COUNT open)"

    # Regression check — only for gate-passed-fixed findings that reappear
    if check_regression; then
      log "  ⚠ REGRESSION DETECTED — a gate-passed-fixed finding reappeared. Stopping."
      log "  regression details (these were fixed + gate-passed, then re-emitted):"
      comm -12 \
        <(python3 -c "
import json
ids=set()
for l in open('$FIXED'):
  l=l.strip()
  if not l: continue
  try:
    o=json.loads(l)
    if o.get('status')=='fixed' and o.get('gate_pass') is True: ids.add(o['id'])
  except: pass
for i in sorted(ids): print(i)
" 2>/dev/null) \
        <(grep -o '"id":"[^"]*"' "$QUEUE" | sed 's/"id":"//;s/"//' | sort -u) | tee -a "$LOG"
      break
    fi

    # Zero findings → clean sweep → converged
    if [[ "${PASS_FINDINGS:-0}" -eq 0 ]]; then
      log "  ✓ pass $PASS found zero issues — converged"
      CONVERGED=1
      break
    fi

    # Convergence with floor: stop if findings dropped to ≤ max(1, last×0.5)
    # (a 1-finding variance alone is too fragile to declare convergence)
    if [[ -n "$LAST_PASS_FINDING_COUNT" ]]; then
      local_floor=$(( LAST_PASS_FINDING_COUNT / 2 ))
      [[ $local_floor -lt 1 ]] && local_floor=1
      if [[ "$PASS_FINDINGS" -le $local_floor ]]; then
        log "  ✓ convergence: pass $PASS ($PASS_FINDINGS) ≤ floor ($local_floor) of pass $((PASS-1)) ($LAST_PASS_FINDING_COUNT)"
        CONVERGED=1
        break
      fi
    fi
    LAST_PASS_FINDING_COUNT="$PASS_FINDINGS"

    # Consume the queue: fix each finding (bounded)
    log "--- PASS $PASS: fix queue ($OPEN_COUNT open findings, max $MAX_FIXES_PER_PASS) ---"
    FIX_IDX=0
    while [[ $FIX_IDX -lt $MAX_FIXES_PER_PASS ]]; do
      FINDING_LINE="$(grep -m1 '"status":"open"' "$QUEUE" 2>/dev/null || true)"
      [[ -z "$FINDING_LINE" ]] && break
      FIX_IDX=$((FIX_IDX + 1))
      log "  fix $FIX_IDX: $FINDING_LINE"

      # Per-finding attempt loop with stall detection
      QUEUE_HASH_BEFORE="$(md5 -q "$QUEUE" 2>/dev/null || md5sum "$QUEUE" | awk '{print $1}')"
      attempt=0
      while [[ $attempt -lt $MAX_FIX_ATTEMPTS ]]; do
        attempt=$((attempt + 1))
        FIX_PROMPT="$STATE_DIR/fix_prompt_pass${PASS}_fix${FIX_IDX}_att${attempt}.md"
        write_fix_prompt "$FIX_PROMPT" "$FINDING_LINE" "$attempt"
        spawn_agent "$FIX_PROMPT"

        # Did the queue change? (status flipped away from "open")
        QUEUE_HASH_AFTER="$(md5 -q "$QUEUE" 2>/dev/null || md5sum "$QUEUE" | awk '{print $1}')"
        if [[ "$QUEUE_HASH_BEFORE" == "$QUEUE_HASH_AFTER" ]]; then
          log "  ⚑ queue unchanged after attempt $attempt — agent may have stalled, retrying"
          continue
        fi
        # The finding line should no longer be "open"
        NEW_STATUS="$(echo "$FINDING_LINE" | python3 -c "
import json,sys
o=json.loads(sys.stdin.read())
print(o.get('status',''))
" 2>/dev/null)"
        # Re-read the finding's current status from the queue (it may have been rewritten)
        FINDING_ID="$(echo "$FINDING_LINE" | python3 -c "import json,sys;print(json.loads(sys.stdin.read()).get('id',''))" 2>/dev/null)"
        CURRENT_STATUS="$(python3 -c "
import json
for l in open('$QUEUE'):
  l=l.strip()
  if not l: continue
  try:
    o=json.loads(l)
    if o.get('id')=='$FINDING_ID':
      print(o.get('status',''))
      break
  except: pass
" 2>/dev/null)"
        if [[ "$CURRENT_STATUS" != "open" ]]; then
          log "    finding $FINDING_ID → $CURRENT_STATUS (attempt $attempt)"
          break
        fi
        log "    finding still open after attempt $attempt (status=$CURRENT_STATUS), retrying"
      done

      if [[ $attempt -ge $MAX_FIX_ATTEMPTS ]]; then
        log "  ⚑ finding $FINDING_ID hit MAX_FIX_ATTEMPTS=$MAX_FIX_ATTEMPTS — marking blocked, moving on"
        # Mark blocked so we don't loop forever on the same line
        python3 -c "
import json
path='$QUEUE'
lines=[json.loads(l) for l in open(path) if l.strip()]
with open(path,'w') as f:
  for o in lines:
    if o.get('id')=='$FINDING_ID' and o.get('status')=='open':
      o['status']='blocked'; o['reason']='max attempts exhausted'
    f.write(json.dumps(o)+'\n')
" 2>/dev/null || true
      fi
    done
    log "  pass $PASS fix phase complete: $FIX_IDX findings processed"
  done

  log "=== BUG HUNT LOOP END ==="
  log "  passes=$PASS converged=$CONVERGED"
  if [[ $CONVERGED -eq 1 ]]; then
    log "  ✅ loop converged — no regressions, diminishing findings"
  else
    log "  ⚠ loop stopped via regression detection or pass ceiling"
  fi
  log "  branch: $BRANCH_NAME"
  log "  state:  $STATE_DIR"
  log "  log:    $LOG"
  log "  fixed:  $FIXED ($(grep -c '' "$FIXED" 2>/dev/null || echo 0) entries)"
}

main "$@"
