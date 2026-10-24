#!/usr/bin/env bash
# Generates docs.html: the flowcharts, an explanation of every box, and the
# source of every function a box stands for. The same run also writes
# docs/algorithm-flowcharts.md. Re-run after changing a chart or any function a
# chart names.
#
#   ./scripts/gen-docs.sh
#
# CI checks the result is current.
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
cargo run --quiet --release -p ccm-docgen
