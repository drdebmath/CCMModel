# Migration status

The production architecture has moved from Python executed through Pyodide to
one Rust simulation core shared by native and browser consumers.

## Completed

- Audited both Python algorithms, wrapper behavior, stress tests, and the
  relation to Sudo et al.; documented the five-slot schema mismatch.
- Added deterministic Python reference fixtures for structured graphs,
  termination, and important intermediate states.
- Added a Rust workspace with dense IDs, reciprocal port graph, deterministic
  RNG, occupancy/ownership indexes, termination types, metrics, and scheduler
  primitives.
- Ported Drop-and-Freeze and Help-by-Scouts with parity, invariant, and
  determinism tests.
- Added deterministic experiment families, placements, port policies,
  parallel sweeps, stable one-row-per-run CSV, and native CLI commands.
- Added trace-off, full, and bounded semantic recorders.
- Added a typed WASM adapter and compact binary render protocol.
- Added typed legacy JSON import/export outside the core.
- Replaced the production Pyodide/Cytoscape execution path with a Rust/web-sys
  application controller and Canvas renderer. The remaining JavaScript is a
  module bootstrap plus a narrow Web Worker transport; it contains no
  simulation or application state machine.
- Added content-derived WASM package loading, transferable typed worker arrays,
  cached deterministic random-graph layouts, legibility-aware port badges, and
  invariant validation for browser execution imports.
- Laid out families by their real structure: tidy layered trees derived from
  the graph's own edges at any branching factor, hub-centred stars, and
  per-node port label rings.
- Sized the browser shell to the viewport so the canvas always ends on screen,
  across a full set of responsive tiers.
- Removed the Python implementation, its fixtures, and its benchmark harness.
  Replaced the machine-specific launchd plist with `ccm-serve`, a
  dependency-free static server in the workspace.
- Added CI covering format, lints, and tests, and failing if the committed
  `wasm/web` package does not match a fresh build of the current source.
- Added P1Tree dispersion (`ccm-p1tree`), implemented from Pattanayak et al.
  rather than from prior code in this repository, and a control-panel glossary
  describing each algorithm.
- Added a dashboard page that sweeps agent counts per graph class and plots the
  cost of dispersion, sharing the simulator's shell, worker and theme.

## Compatibility decisions

The Rust ports preserve the behavior of the dedicated Python files they
replaced, before attempting paper-level semantic changes. In particular,
Drop-and-Freeze canonicalizes ports because the Python reference did, and that
reference's Help-by-Scouts multi-start failure is retained and covered by a
test. Neither implementation is presented as a
proof-compatible port of the named algorithms in Sudo et al.; see the behavior
and metrics documents before interpreting empirical results.

Python has been removed. The repository is Rust end to end: simulation core,
native CLI, browser application, and the `ccm-serve` development server. The
behavior it defined is preserved in the Rust ports and recorded in
[behavior-spec.md](behavior-spec.md) and
[complexity-metrics.md](complexity-metrics.md), both archival.
