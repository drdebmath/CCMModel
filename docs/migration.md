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

## Compatibility decisions

The Rust ports preserve the current dedicated Python files before attempting
paper-level semantic changes. In particular, Drop-and-Freeze canonicalizes
ports because Python does, and the current Help-by-Scouts multi-start failure
is retained and covered by a test. Neither implementation is presented as a
proof-compatible port of the named algorithms in Sudo et al.; see the behavior
and metrics documents before interpreting empirical results.

Python remains in the repository as a reference and benchmark baseline. It is
not loaded by the production browser application and is not required for
native Rust experiments.
