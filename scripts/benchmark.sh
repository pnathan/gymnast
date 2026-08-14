#!/usr/bin/env bash
set -euo pipefail

# Synthesis benchmark: 5 languages × 2 specs × 3 trials each.
# Runs via the Lamedh synthesis-trials infrastructure with Claude Haiku.
# All 30 trials must pass for a zero exit code.

LAMEDH=".tools/bin/lamedh"
TRIAL_COUNT="${TRIAL_COUNT:-3}"
MAX_ATTEMPTS="${MAX_ATTEMPTS:-3}"
OUTPUT_DIR="build/benchmark"

if [ ! -x "$LAMEDH" ]; then
  echo "ERROR: Lamedh not found at $LAMEDH — run scripts/bootstrap-lamedh.sh first"
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

SPECS=(
  "examples/todo.lisp         todo-spec          todo-ruby"
  "examples/todo-go.lisp      todo-go-spec       todo-go"
  "examples/todo-java.lisp    todo-java-spec     todo-java"
  "examples/todo-python.lisp  todo-python-spec   todo-python"
  "examples/todo-rust.lisp    todo-rust-spec     todo-rust"
  "examples/twitter.lisp      twitter-spec       twitter-ruby"
  "examples/twitter-go.lisp   twitter-go-spec    twitter-go"
  "examples/twitter-java.lisp twitter-java-spec  twitter-java"
  "examples/twitter-python.lisp twitter-python-spec twitter-python"
  "examples/twitter-rust.lisp twitter-rust-spec  twitter-rust"
)

run_one() {
  local spec_file="$1" spec_name="$2" label="$3"
  local out="$OUTPUT_DIR/${label}.sexpr"
  local log="$OUTPUT_DIR/${label}.log"

  echo "[start] $label ($TRIAL_COUNT trials, max $MAX_ATTEMPTS attempts)"
  "$LAMEDH" scripts/run-benchmark-target.lisp \
    "$spec_file" "$spec_name" "$label" "$TRIAL_COUNT" "$MAX_ATTEMPTS" \
    > "$log" 2>&1
  local rc=$?
  if [ $rc -ne 0 ]; then
    echo "[FAIL]  $label — see $log"
    return $rc
  fi

  local passed
  passed=$(grep -c 'succeeded' "$log" || true)
  local total=$TRIAL_COUNT

  if [ "$passed" -eq "$total" ]; then
    echo "[pass]  $label ($passed/$total)"
  else
    echo "[FAIL]  $label ($passed/$total) — see $log"
    return 1
  fi
}

echo "=== Gymnast Synthesis Benchmark ==="
echo "Trials per target: $TRIAL_COUNT"
echo "Max attempts per trial: $MAX_ATTEMPTS"
echo "Targets: ${#SPECS[@]}"
echo ""

PIDS=()
LABELS=()
FAILURES=0

for entry in "${SPECS[@]}"; do
  read -r spec_file spec_name label <<< "$entry"
  run_one "$spec_file" "$spec_name" "$label" &
  PIDS+=($!)
  LABELS+=("$label")
done

echo ""
echo "Launched ${#PIDS[@]} targets in parallel. Waiting..."
echo ""

for i in "${!PIDS[@]}"; do
  if ! wait "${PIDS[$i]}"; then
    FAILURES=$((FAILURES + 1))
  fi
done

echo ""
echo "=== RESULTS ==="
echo "Targets: ${#SPECS[@]}"
echo "Trials per target: $TRIAL_COUNT"
echo "Total trials: $((${#SPECS[@]} * TRIAL_COUNT))"
echo "Failed targets: $FAILURES"

if [ "$FAILURES" -gt 0 ]; then
  echo ""
  echo "BENCHMARK FAILED: $FAILURES target(s) did not achieve 100% pass rate."
  echo "Logs in $OUTPUT_DIR/"
  exit 1
fi

echo ""
echo "BENCHMARK PASSED: all targets achieved 100% pass rate."
exit 0
