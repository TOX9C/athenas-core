#!/usr/bin/env bash
# Overnight autonomous improvement loop for athenas-core.
# Runs `omp -p` iterations against .plans/improve/LOOP.md until:
#   - the time budget expires (default 8h, override: HOURS=6)
#   - .plans/improve/DONE exists (agent finished the backlog)
#   - .plans/improve/STOP exists (touch this file to stop it gracefully)
# Logs: .plans/improve/log/iteration-<n>.log
set -u
cd "$(dirname "$0")/.."

HOURS="${HOURS:-8}"
DEADLINE=$(( $(date +%s) + HOURS * 3600 ))
LOG_DIR=".plans/improve/log"
mkdir -p "$LOG_DIR"

# Isolation: everything lands on a dedicated branch.
if ! git rev-parse --verify agent/overnight-improve >/dev/null 2>&1; then
  git branch agent/overnight-improve
  echo "[loop] created branch agent/overnight-improve"
fi
git checkout agent/overnight-improve

i=0
while true; do
  i=$((i + 1))
  NOW=$(date +%s)
  if [ "$NOW" -ge "$DEADLINE" ]; then
    echo "[loop] time budget (${HOURS}h) exhausted after $((i - 1)) iterations"
    break
  fi
  if [ -f .plans/improve/STOP ]; then
    echo "[loop] STOP file found after $((i - 1)) iterations"
    break
  fi
  if [ -f .plans/improve/DONE ]; then
    echo "[loop] DONE sentinel found — backlog complete"
    break
  fi

  echo "[loop] iteration $i starting $(date)"
  omp -p "@.plans/improve/LOOP.md" >"$LOG_DIR/iteration-$i.log" 2>&1
  rc=$?
  echo "[loop] iteration $i exited rc=$rc $(date)"

  # Safety: if the iteration left the tree broken AND uncommitted, stash it.
  if ! git diff --quiet || ! git diff --cached --quiet; then
    if [ "$rc" -ne 0 ]; then
      git stash push -m "loop-iter-$i-uncommitted" >/dev/null 2>&1 || true
      echo "[loop] iteration $i failed with uncommitted changes; stashed"
    fi
  fi

  # Back off on repeated failures (e.g. API down) instead of hot-spinning.
  if [ "$rc" -ne 0 ]; then
    echo "[loop] backing off 120s after failure"
    sleep 120
  fi
done

echo "[loop] done. Review with: git log main..agent/overnight-improve --oneline"
