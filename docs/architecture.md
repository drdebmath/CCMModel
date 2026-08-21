# Architecture

CCMModel is a simulation library first. The authoritative state transitions
are native-compatible Rust and are shared by research runs and browser
visualization.

```text
ccm-core ───────────── dense IDs, reciprocal port graph, RNG, metrics
   ├── ccm-algorithms ─ Drop-and-Freeze
   ├── ccm-help-scouts  Help-by-Scouts
   ├── ccm-trace ────── optional semantic recording
   ├── ccm-experiments  graph/placement generation and parallel sweeps
   │      └── ccm-cli ─ native CSV interface
   ├── ccm-compat ───── isolated legacy JSON boundary
   ├── ccm-wasm ────── typed browser adapter and compact render protocol
   │                          │
   │                          └── Web Worker
   └── ccm-web ─────── Rust/web-sys UI → Canvas 2D
```

The dependency arrows point away from `ccm-core`. The core has no DOM, file,
CLI, CSV, JavaScript, Pyodide, or rendering dependency. Browser code prepares
graph inputs and renders outputs; it never implements an algorithm transition.

## Native execution

`ccm-cli` prepares a complete `RunRequest`, deterministically generates the
graph, port assignment, and placement, and invokes one of the two Rust
algorithms. Sweep parallelism is across independent simulations. Results are
returned in input order regardless of worker count.

Instrumentation and tracing are orthogonal. `ComplexityMetrics` counts modeled
operations. `NoTrace` allocates no playback history; `FullTrace` is intended
for small cases; `BoundedTrace` samples and evicts deterministically under
explicit budgets.

## Browser execution

The `ccm-web` Rust/WASM module owns main-thread controls, graph inputs,
playback, filters, import/export, responsive behavior, and Canvas rendering.
The JavaScript entry point only resolves the content-derived build manifest and
initializes that generated module. A small module Web Worker owns `ccm-wasm`
and runs the same Rust algorithms as the CLI. A run transfers typed final-state
arrays and one compact, versioned binary trace buffer directly to `ccm-web`;
the transport does not expand them through ordinary arrays or JSON and does not
send one message per agent action.

Random-graph coordinates are computed once per graph/canvas size and reused by
playback. Port badges have edge-count and maximum-degree level-of-detail limits,
while node tooltips retain the complete canonical port table. Browser JSON
imports pass graph, placement, result, and frame invariant checks before they
can reach the renderer.

Cancellation terminates and recreates the worker because WebAssembly calls are
synchronous. This guarantees that the UI remains responsive even when an
algorithm is still executing.

## Compatibility boundary

The old Python wrapper emitted five parallel histories whose last three slots
have different meanings for the two algorithms. `ccm-compat` decodes them only
when the algorithm is explicitly supplied and maps them into named variants.
Legacy types do not leak into `ccm-core`.

## Determinism

Dense vectors, stable iteration, explicit tie breaking, reciprocal local port
tables, and the repository-owned SplitMix64 generator define reproducible
execution. Browser animation timing and native thread scheduling cannot change
simulation results.
