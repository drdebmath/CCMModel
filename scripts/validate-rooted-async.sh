#!/usr/bin/env bash
# Validates RootedAsync against the properties the paper claims, and prints
# one PASS/FAIL line per property.
#
#   ./scripts/validate-rooted-async.sh          # the test suites, ~2 minutes
#   ./scripts/validate-rooted-async.sh --full   # also the 8,208-run sweeps
#
#   0. The paper's own example run (Appendix B) ends as the paper says.
#   1. Dispersion: every run ends with at most one agent per node.
#   2. Asynchrony: random orders, with each cycle atomic or split into a look
#      and a later write, and any one slow agent, all give the same placement
#      as synchronous rounds.
#   3. Time: O(1) epochs per DFS step whatever k or the degree, at most 4k
#      steps, linear Retrace; hence O(k) epochs in all.
#   4. Memory: O(log(k + Δ)) bits per agent between cycles (Theorem 2).
#
# The tests behind each line are in crates/ccm-experiments/tests/
# rooted_async.rs and rooted_async_validation.rs. Exits non-zero if any fails.
set -uo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"

full=false
[ "${1:-}" = "--full" ] && full=true

log_dir="$(mktemp -d)"
trap 'rm -rf "$log_dir"' EXIT
failed=0
results=()
last_log=""

# Runs the tests whose names contain any of the given filters.
check() {
  local label="$1" suite="$2"
  shift 2
  local log="$log_dir/$((${#results[@]} + 1)).log"
  last_log="$log"
  if cargo test --quiet --release -p ccm-experiments --test "$suite" -- \
      --include-ignored --nocapture --test-threads=1 "$@" >"$log" 2>&1; then
    local passed
    passed=$(grep -Eo '[0-9]+ passed' "$log" | awk '{n+=$1} END {print n+0}')
    results+=("PASS  $label  ($passed tests)")
  else
    results+=("FAIL  $label  (log: see below)")
    failed=1
    echo "---- $label failed ----"
    grep -E 'panicked|assertion|left:|right:|FAILED' "$log" | head -20
  fi
}

echo "Building and running the RootedAsync validation suites (release)..."
check "0. The paper's example run (Appendix B)" rooted_async the_papers_example
check "1. Dispersion: at most one agent per node" rooted_async_validation \
  every_run_ends_with_at_most_one_agent_per_node
check "2. Asynchrony: schedule-independent, interleaved, robust to a slow agent" \
  rooted_async_validation random_schedules_really_interleave \
  the_schedule_never_changes_the_placement any_one_slow_agent_only_delays_the_run \
  cycles_really_overlap_under_the_interleaved_schedule
check "3. Time: O(1) epochs per step, O(k) in total" rooted_async_validation \
  the_next_node_costs_constant_epochs the_time_bounds_hold_up_to_256_agents \
  a_step_costs_the_same_whatever_the_degree
time_log="$last_log"
check "4. Memory: O(log(k+Δ)) bits per agent" rooted_async_validation \
  memory_stays_logarithmic
memory_log="$last_log"

if $full; then
  for schedule in "" 11 i11; do
    label="Local view vs truth, 8,208 runs, schedule ${schedule:-synchronous}"
    out=$(cargo run --quiet --release -p ccm-experiments --example rooted_async_sweep -- $schedule 2>&1)
    if echo "$out" | grep -q ', 0 failed'; then
      results+=("PASS  $label")
    else
      results+=("FAIL  $label")
      failed=1
      echo "$out"
    fi
  done
fi

# The per-run figures behind property 3.
if [ -f "$time_log" ]; then
  echo
  echo "Property 3, per run (step = longest DFS step in epochs; bounds: step <= 40,"
  echo "steps/k <= 4, retrace/k <= 10, batches <= 3, scouts >= ceil((k-2)/3)):"
  grep -E '^(case|[a-z])' "$time_log" | grep -Ev '^(test|running)' || true
fi

# The per-run figures behind property 4.
if [ -f "$memory_log" ]; then
  echo
  echo "Property 4, per run (most bits any agent held between cycles; bound: 48 x log2(k+Δ)):"
  grep -E ' bits ' "$memory_log" || true
fi

echo
printf '%s\n' "${results[@]}"
exit "$failed"
