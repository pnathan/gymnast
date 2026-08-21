#!/usr/bin/env bash
set -euo pipefail

# Synthesis benchmark: 10 specs (5 languages x {todo,twitter}) x N trials
# each, driven through the Rust CLI's `synthesize` subcommand.
#
# CRITICAL: this invokes a REAL language model via
# runner::ClaudeSubprocessProvider and requires ANTHROPIC_API_KEY to be
# set. It is NOT invoked by CI (see .github/workflows/ci.yml) — run it
# manually only.
#
# All trials across all specs must pass (exit code 0 from the CLI) for
# this script to exit zero.

cd "$(dirname "$0")/.."
REPO_ROOT="$(pwd)"

RUST_DIR="$REPO_ROOT/rust"
TRIAL_COUNT="${TRIAL_COUNT:-3}"
MAX_ATTEMPTS="${MAX_ATTEMPTS:-3}"
OUTPUT_DIR="$REPO_ROOT/build/benchmark"

SPECS=(
  "examples/todo.gym             todo-ruby"
  "examples/todo-go.gym          todo-go"
  "examples/todo-java.gym        todo-java"
  "examples/todo-python.gym      todo-python"
  "examples/todo-rust.gym        todo-rust"
  "examples/twitter.gym          twitter-ruby"
  "examples/twitter-go.gym       twitter-go"
  "examples/twitter-java.gym     twitter-java"
  "examples/twitter-python.gym   twitter-python"
  "examples/twitter-rust.gym     twitter-rust"
)

# Fail fast and clearly if any spec file is missing, rather than
# silently skipping it (specs may still be under construction by other
# agents).
missing=0
for entry in "${SPECS[@]}"; do
  read -r spec_file _label <<< "$entry"
  if [ ! -f "$REPO_ROOT/$spec_file" ]; then
    echo "ERROR: spec file missing: $spec_file" >&2
    missing=1
  fi
done
if [ "$missing" -ne 0 ]; then
  echo "ERROR: one or more spec files are missing; aborting benchmark." >&2
  exit 1
fi

if [ -z "${ANTHROPIC_API_KEY:-}" ]; then
  echo "ERROR: ANTHROPIC_API_KEY is not set — the synthesize subcommand invokes" >&2
  echo "       a real model and requires it." >&2
  exit 1
fi

mkdir -p "$OUTPUT_DIR"

echo "=== Gymnast Synthesis Benchmark (Rust CLI) ==="
echo "Building release binary..."
(cd "$RUST_DIR" && cargo build --release)
GYMNAST_BIN="$RUST_DIR/target/release/gymnast-rs"
if [ ! -x "$GYMNAST_BIN" ]; then
  echo "ERROR: expected release binary not found at $GYMNAST_BIN" >&2
  exit 1
fi

echo "Trials per target: $TRIAL_COUNT"
echo "Max attempts per trial: $MAX_ATTEMPTS"
echo "Targets: ${#SPECS[@]}"
echo ""

# Runs TRIAL_COUNT independent `synthesize` invocations for one spec,
# each into its own trial subdirectory (the CLI overwrites out_dir
# contents on each run, so trials cannot share one). Writes a per-target
# log and a per-target result summary file; returns non-zero if any
# trial failed.
run_one() {
  local spec_file="$1" label="$2"
  local target_dir="$OUTPUT_DIR/$label"
  local log="$OUTPUT_DIR/${label}.log"
  local result_file="$OUTPUT_DIR/${label}.result"
  mkdir -p "$target_dir"
  : > "$log"

  echo "[start] $label ($TRIAL_COUNT trials, max $MAX_ATTEMPTS attempts)" | tee -a "$log"

  local passed=0
  local trial
  for trial in $(seq 1 "$TRIAL_COUNT"); do
    local trial_dir="$target_dir/trial-$trial"
    rm -rf "$trial_dir"
    mkdir -p "$trial_dir"
    echo "-- trial $trial --" >> "$log"
    if (cd "$REPO_ROOT" && "$GYMNAST_BIN" synthesize "$spec_file" "$trial_dir" "$MAX_ATTEMPTS") >> "$log" 2>&1; then
      echo "trial $trial: succeeded" >> "$log"
      passed=$((passed + 1))
    else
      echo "trial $trial: failed" >> "$log"
    fi
  done

  echo "$label: $passed/$TRIAL_COUNT" > "$result_file"

  if [ "$passed" -eq "$TRIAL_COUNT" ]; then
    echo "[pass]  $label ($passed/$TRIAL_COUNT)"
  else
    echo "[FAIL]  $label ($passed/$TRIAL_COUNT) — see $log"
    return 1
  fi
}

PIDS=()
LABELS=()
FAILURES=0

for entry in "${SPECS[@]}"; do
  read -r spec_file label <<< "$entry"
  run_one "$spec_file" "$label" &
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
