#!/usr/bin/env bash
# Generates docs.html: the flowcharts, and the source of every function a box
# stands for. Re-run after changing any function a chart names.
#
#   ./scripts/gen-docs.sh
#
# CI checks the result is current.
set -euo pipefail
repo_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_dir"
cargo run --quiet --release -p ccm-docgen
