#!/usr/bin/env bash
# Generates docs/algorithm-flowcharts.md (and docs.html).
#
# Both come from one place: the chart data in crates/ccm-docgen/src/spec.rs.
# The code blocks are lifted straight out of the crates, so a function shown in
# the document is the function that runs. Re-run after changing a chart or any
# function it quotes:
#
#   ./scripts/gen-flowcharts.sh
#
# CI checks the result is current, the same way it checks the WASM package.
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
cargo run --quiet --release -p ccm-docgen
