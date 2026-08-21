# CCMModel Rust Migration Agent Guide

## 1. Mission

Migrate `drdebmath/CCMModel` to a **single Rust simulation core** that supports two distinct use cases:

### Case 1 — Native research simulations

Run large numbers of simulations locally from native Rust binaries in order to study the algorithmic behavior of CCM algorithms under different assumptions.

Primary measurements include:

- round complexity;
- move complexity;
- edge-traversal complexity;
- probe complexity;
- message/communication complexity where modeled;
- memory/state complexity;
- maximum simultaneous auxiliary state;
- tree depth / exploration depth;
- number and size of groups;
- algorithm-specific operation counts;
- termination behavior;
- worst-case and distributional behavior across graph families, port assignments, placements, and seeds.

**Wall-clock time is not a complexity result.**

Runtime may be benchmarked for engineering purposes, but it must never be substituted for round, move, message, or memory complexity.

### Case 2 — Browser visualization

Use the **same Rust simulation code** to generate visualizable executions in the browser.

Typical visualizations are expected to involve fewer than 1,000 agents.

The visualization system should remain usable up to approximately 10,000 agents, but 10,000-agent rendering is an upper-bound engineering target, not the normal research execution mode.

The browser application must:

- run entirely on the client;
- require no Python runtime;
- require no application backend;
- use the same algorithm implementation as native experiments;
- allow small simulations to retain detailed step-by-step playback;
- avoid forcing visualization overhead into native research simulations.

---

# 2. Core architectural principle

The project is:

> **a simulation library first,  
> an experimental research system second,  
> and a visualization application third.**

Do not architect it as a web application that happens to contain simulation code.

There must be exactly one authoritative implementation of each algorithm.

That implementation lives in pure Rust and must compile both:

```text
native Rust target
```

and:

```text
wasm32 browser target
```

No separate browser implementation of an algorithm is allowed.

---

# 3. Target architecture

```text
                           ┌────────────────────────┐
                           │       ccm-core         │
                           │                        │
                           │ graph model            │
                           │ port labels            │
                           │ agents                 │
                           │ algorithm state        │
                           │ schedulers             │
                           │ deterministic RNG      │
                           │ complexity counters    │
                           │ simulation engine      │
                           └───────────┬────────────┘
                                       │
                    ┌──────────────────┴───────────────────┐
                    │                                      │
                    ▼                                      ▼

          CASE 1: NATIVE RESEARCH                 CASE 2: VISUALIZATION

          ccm-cli / ccm-experiments               ccm-wasm / ccm-web
                    │                                      │
          parameter sweeps                              Web Worker
          graph families                                  │
          adversarial cases                               ▼
          repeated trials                           trace / snapshots
          complexity output                                │
                    │                                      ▼
          CSV / Parquet / JSON                      Canvas/WebGL
          aggregate summaries
```

The dependency direction must always point outward from the simulation kernel.

`ccm-core` must never depend on:

- browser APIs;
- DOM;
- Canvas;
- WebGL;
- Yew;
- CSV output;
- CLI argument parsing;
- filesystem paths;
- plotting;
- Pyodide;
- JavaScript;
- visualization-specific state.

---

# 4. Recommended workspace

A reasonable workspace is:

```text
CCMModel/
├── Cargo.toml
├── rust-toolchain.toml
├── crates/
│   ├── ccm-core/
│   ├── ccm-algorithms/
│   ├── ccm-metrics/
│   ├── ccm-trace/
│   ├── ccm-experiments/
│   ├── ccm-cli/
│   ├── ccm-compat/
│   ├── ccm-wasm/
│   └── ccm-web/
├── tests/
│   └── fixtures/
├── experiments/
│   ├── configs/
│   └── README.md
├── docs/
│   ├── behavior-spec.md
│   ├── simulation-model.md
│   ├── complexity-metrics.md
│   ├── architecture.md
│   └── migration.md
└── agent.md
```

Fewer crates are acceptable if the boundaries remain clear.

The important separation is:

```text
simulation semantics
!=
experiment orchestration
!=
trace recording
!=
visualization
```

---

# 5. Non-negotiable rules

## 5.1 One algorithm implementation

Do not maintain:

```text
Python research algorithm
+
Rust browser algorithm
```

or:

```text
native Rust algorithm
+
separate WASM algorithm
```

There must be one Rust implementation.

---

## 5.2 Complexity is measured by algorithmic counters

Never infer algorithmic complexity solely from elapsed time.

Instrument explicit logical operations.

Runtime benchmarking is allowed only as an implementation-performance measurement.

---

## 5.3 Visualization must be optional

The core simulator must run with:

```text
trace = off
visualization = absent
```

A headless experiment must not allocate visual frames, labels, graph coordinates, JSON history, or browser-oriented state.

---

## 5.4 Determinism is required

For a fixed:

```text
algorithm version
graph
port assignment
initial placement
algorithm parameters
seed
```

the simulator must produce deterministic algorithmic results.

Do not allow observable semantics to depend on:

- hash iteration order;
- OS thread scheduling;
- browser timing;
- rendering timing;
- wall-clock time;
- nondeterministic worker scheduling.

---

## 5.5 Port semantics are part of the model

The graph is not merely an undirected adjacency structure.

It is a **port-labeled graph**.

The core representation must make:

```text
(node, local_port)
    ->
(neighbor_node, neighbor_port)
```

a direct and explicit operation.

---

## 5.6 Preserve behavior before changing semantics

The current Python implementation is initially the behavioral reference.

Do not blindly port bugs, but do not silently change semantics either.

When Rust and Python differ:

1. construct the smallest reproducer;
2. identify whether the Python behavior is intended or erroneous;
3. document the decision;
4. encode the intended behavior in a test;
5. only then change the implementation.

---

# 6. Known issue in the current code

Before porting algorithms, verify the current repository.

At the time this document was written, the Python wrapper treats the five return values of both algorithms as though they have the same meaning.

They do not.

Drop-and-Freeze and Help-by-Scouts expose different state in the corresponding slots.

Do not reproduce this implicit parallel-array contract in Rust.

Define explicit typed structures.

Algorithm-specific state must have algorithm-specific names.

---

# 7. Core simulation model

## 7.1 Dense IDs

Use dense integer IDs.

Illustrative types:

```rust
#[repr(transparent)]
pub struct AgentId(pub u32);

#[repr(transparent)]
pub struct NodeId(pub u32);

#[repr(transparent)]
pub struct PortId(pub u16);
```

Prefer IDs that permit direct array indexing.

Avoid hashing when the domain is naturally dense.

---

## 7.2 Agent storage

The simulator may contain 10,000 or more agents in native experiments.

Do not model each agent as a large heap-owned object containing multiple maps, sets, strings, and dynamically allocated structures.

Benchmark:

```text
Array of Structs
```

versus:

```text
Structure of Arrays
```

or a hybrid.

Candidate SoA layout:

```rust
struct Agents {
    node: Vec<NodeId>,
    status: Vec<AgentStatus>,
    home: Vec<Option<NodeId>>,
    parent: Vec<Option<AgentId>>,
    arrival_port: Vec<Option<PortId>>,
    flags: Vec<u8>,
}
```

Candidate AoS layout:

```rust
#[derive(Clone, Copy)]
struct AgentState {
    node: NodeId,
    status: AgentStatus,
    home: Option<NodeId>,
    parent: Option<AgentId>,
    arrival_port: Option<PortId>,
}
```

Select based on representative algorithm benchmarks and clarity.

Do not select a layout merely because it is theoretically fashionable.

---

## 7.3 Avoid hot-path heap structures

Unless profiling proves otherwise, avoid these per agent in the hot path:

- `HashMap`
- `HashSet`
- `String`
- boxed trait objects
- repeatedly allocated `Vec`
- reference-counted object graphs

Prefer:

- dense arrays;
- bit sets;
- compact enums;
- reusable scratch buffers;
- node-indexed arrays;
- agent-indexed arrays;
- bounded small buffers when maximum degree is known.

---

# 8. Port graph representation

A scalable representation may use a CSR-like layout.

Example:

```rust
pub struct PortEdge {
    pub neighbor: NodeId,
    pub remote_port: PortId,
}

pub struct NodePorts {
    pub start: u32,
    pub len: u16,
}

pub struct PortGraph {
    pub nodes: Vec<NodePorts>,
    pub ports: Vec<PortEdge>,
}
```

A `Vec<Vec<PortEdge>>` implementation is acceptable initially if simpler and if benchmarks show no important disadvantage.

Required invariants:

- each local port exists;
- every port has a reciprocal port;
- reciprocal traversal returns to the source;
- no invalid node ID exists;
- graph constraints match the declared graph model;
- connectedness requirements are enforced where applicable.

---

# 9. Ownership and occupancy indexes

Do not repeatedly scan all agents to answer queries that can be maintained incrementally.

Examples:

```rust
home_owner: Vec<Option<AgentId>>,
settled_agent_at: Vec<Option<AgentId>>,
tree_parent: Vec<Option<AgentId>>,
```

Tree children must not be rediscovered by scanning all agents every time.

Use either:

- explicit child lists;
- compact adjacency storage;
- rebuildable indexes with documented complexity.

For agents grouped by current node, consider a flat reusable bucket representation:

```text
count agents per node
prefix sum
fill one flat AgentId array
```

giving:

```text
O(A + V)
```

rebuild complexity with good locality.

Incremental occupancy maintenance is also acceptable if it benchmarks better for the real algorithms.

---

# 10. Simulation scheduler

"Each agent runs independently" describes the distributed algorithm model.

It does **not** imply one operating-system thread, Web Worker, async task, or browser component per agent.

The simulator should model independence explicitly.

For synchronous phases, prefer a bulk-synchronous structure:

```text
CURRENT STATE
     │
     ▼
COMPUTE LOGICAL ACTIONS / INTENTS
     │
     ▼
RESOLVE INTERACTIONS DETERMINISTICALLY
     │
     ▼
COMMIT
     │
     ▼
NEXT PHASE / ROUND
```

This makes round semantics explicit and supports later parallelization without changing the mathematical model.

---

# 11. Case 1 — Native research simulations

This is the primary performance path.

The native simulator must compile as optimized Rust with no WASM or browser dependencies.

It should support commands conceptually similar to:

```text
ccm run
ccm sweep
ccm compare
ccm validate
ccm trace
ccm benchmark
```

Names may change.

---

# 12. Experiment configuration

Different assumptions must be first-class configuration, not hardcoded forks.

Example conceptual configuration:

```rust
pub struct ExperimentConfig {
    pub algorithm: Algorithm,
    pub graph_model: GraphModel,
    pub port_assignment: PortAssignment,
    pub starting_model: StartingModel,
    pub scheduler_model: SchedulerModel,
    pub nodes: usize,
    pub agents: usize,
    pub seed: u64,
}
```

Possible graph families:

```text
Path
Cycle
Star
Grid
Tree
Complete
RandomConnected
RandomBoundedDegree
ExplicitGraph
Adversarial
```

Possible port assignments:

```text
Random
Canonical
Adversarial
Explicit
```

Possible initial placements:

```text
SingleNode
UniformRandom
FixedCount
Clustered
Adversarial
Explicit
```

Do not require every category above immediately.

Design the interfaces so new assumptions can be introduced without modifying algorithm implementations.

---

# 13. Complexity metrics

Define the meaning of every counter in:

```text
docs/complexity-metrics.md
```

Metrics must correspond to operations in the modeled algorithm.

Potential common metrics:

```text
rounds
macro_rounds
agent_moves
edge_traversals
port_probes
probe_out_traversals
probe_back_traversals
settlements
backtracks
group_moves
maximum_group_size
maximum_simultaneously_unsettled
maximum_tree_depth
peak_owned_nodes
```

Help-by-Scouts may additionally count:

```text
scout_operations
vacates
chase_operations
follow_operations
retraces
parallel_probe_phases
```

Add algorithm-specific counters when required by the theoretical analysis.

Do not invent "message complexity" unless the simulator has a clear mapping from operations to conceptual messages.

If message complexity is needed, explicitly specify what counts as one message.

---

# 14. Round complexity

Round accounting must be part of the simulation semantics.

Do not increment a round counter based on implementation loops without first documenting what one theoretical round means.

If an algorithm has:

```text
macro-round
sub-round
probe-out
probe-back
move
```

represent these explicitly.

Example:

```rust
struct ComplexityMetrics {
    rounds: u64,
    macro_rounds: u64,
    probe_rounds: u64,
    movement_rounds: u64,
    ...
}
```

Only expose counters that have a clear theoretical meaning.

---

# 15. Memory complexity

Memory complexity must not be measured simply as process RSS.

The experiment system should expose **logical algorithmic memory usage** separately from implementation memory.

For example, count:

- agent-local state words;
- entries in probe structures;
- tree/parent state;
- node ownership state;
- algorithm-maintained sets;
- maximum active auxiliary entries;
- maximum stack/retrace depth;
- maximum per-agent auxiliary state;
- total distributed local memory if theoretically relevant.

Define a logical memory model.

For example:

```text
one NodeId           = one word
one AgentId          = one word
one PortId           = one word
one boolean/status   = one word or specified bit model
one stored tuple     = sum of its fields
```

The chosen model must be documented and used consistently across experiments.

Separately, implementation memory may be profiled for engineering purposes.

Never conflate:

```text
algorithmic memory complexity
```

with:

```text
Rust allocator / process memory usage
```

---

# 16. Instrumentation architecture

Instrumentation must not require rewriting the algorithms.

A useful conceptual design is:

```rust
simulate::<Metrics, Recorder>(...)
```

or equivalent static composition.

Research execution:

```text
Metrics  = ComplexityMetrics
Recorder = NoTrace
```

Pure performance benchmark:

```text
Metrics  = NoMetrics
Recorder = NoTrace
```

Visualization:

```text
Metrics  = optional
Recorder = EventTrace
```

The exact Rust API may differ.

The important property is that trace recording and complexity counters are orthogonal.

---

# 17. Experiment sweeps

The research runner should support parameter grids and repeated trials.

Conceptually:

```text
nodes       = [100, 200, 400, 800, ...]
agents      = [...]
degree      = [...]
start_model = [...]
port_model  = [...]
seeds       = range(...)
```

Results should be emitted one simulation per row where practical.

Suggested fields:

```text
algorithm
algorithm_version
graph_model
nodes
edges
agents
max_degree
start_model
port_model
seed
completed
termination_reason
rounds
moves
probes
retraces
logical_peak_memory
...
```

---

# 18. Parallelization strategy for Case 1

Parallelize **independent simulations first**.

This is the safest and most useful form of parallelism.

For a sweep:

```text
seed 1  ── core 1
seed 2  ── core 2
seed 3  ── core 3
...
```

Use a bounded native thread pool, potentially Rayon.

This must not alter single-simulation semantics.

Priority:

```text
1. parallelize independent runs
2. optimize one simulation
3. consider intra-simulation parallelism only if necessary
```

Do not introduce parallel execution inside an algorithm merely because logical agents are independent.

---

# 19. Intra-simulation parallelism

Do not implement this initially.

First make single-simulation execution:

- correct;
- deterministic;
- asymptotically efficient;
- allocation-conscious.

Only if a single very large simulation becomes a research bottleneck should the agent evaluate parallel phases.

A suitable boundary may be:

```text
parallel:
    compute read-only intents

deterministic:
    resolve conflicts
    commit
```

Parallel execution must produce the same logical result as the deterministic reference execution.

---

# 20. Research output formats

Scientific result output is not the same thing as visualization output.

## Summary results

Preferred:

```text
CSV
```

for small/medium studies and interoperability.

Consider:

```text
Parquet
```

for very large experiment datasets.

JSON may be provided for metadata/configuration but should not be the only large-scale tabular output.

---

# 21. Reproducibility

Every experiment output should contain enough metadata to reproduce the run.

Include where relevant:

```text
algorithm
algorithm version
git commit
graph model
port model
starting model
node count
edge count
agent count
seed
configuration parameters
termination condition
```

For especially important runs, support saving the explicit:

```text
graph
port labels
initial agent positions
```

so reproduction does not depend on a future RNG implementation.

---

# 22. Adversarial and structured cases

Random simulations alone are insufficient for empirical worst-case investigation.

Support structured instances such as:

- paths;
- stars;
- cycles;
- bounded-degree high-diameter graphs;
- trees;
- grids;
- clustered initial placements;
- endpoint concentration;
- carefully chosen port orders;
- explicit hand-authored graphs;
- adversarial placement generators.

The experiment framework should make it easy to add new generators.

The algorithm code must remain unchanged.

---

# 23. Case 2 — Browser visualization

The browser application is a consumer of `ccm-core`.

It must not contain a separate algorithm.

Compile the same core to WebAssembly.

Run simulations inside a Web Worker so algorithm execution never blocks UI interaction.

Architecture:

```text
MAIN THREAD

Rust UI
controls
playback
selection
renderer

        │
        │ compact trace/render messages
        ▼

WEB WORKER

Rust/WASM
ccm-core
trace recorder
```

---

# 24. Visualization scale

Optimize UX for:

```text
< 1,000 agents
```

while keeping approximately:

```text
10,000 agents
```

as an upper-bound supported scenario.

Do not distort the native research architecture merely to make every 10,000-agent execution fully animatable at every logical step.

---

# 25. Trace modes

Visualization requires history.

Research simulations generally do not.

Support multiple trace policies.

## Full trace

For small simulations.

Provides detailed step-by-step playback.

```text
TraceMode::Full
```

## Bounded / sampled trace

For large visualizations.

```text
TraceMode::Bounded {
    memory_budget,
    checkpoint_interval,
    sampling_policy,
}
```

## No trace

For native experiments and headless runs.

```text
TraceMode::Off
```

Never create complete world snapshots after every individual agent mutation in the general simulation core.

---

# 26. Trace representation

Prefer semantic events.

Examples:

```rust
pub enum SimulationEvent {
    PhaseStarted { /* ... */ },
    AgentMoved { /* ... */ },
    GroupMoved { /* ... */ },
    AgentSettled { /* ... */ },
    AgentStateChanged { /* ... */ },
    TreeEdgeAdded { /* ... */ },
    NodeStateChanged { /* ... */ },
}
```

Where the algorithm performs a group operation, record a group-level event when this preserves replay semantics.

Do not deep-copy all agents per individual move.

Periodic full checkpoints may be used to make seeking efficient.

---

# 27. Browser transport

Do not serialize the entire simulation state to JSON for every step.

JSON is for:

- import;
- export;
- compatibility;
- debugging.

Worker-to-renderer communication should use compact typed/binary data.

Do not send one browser message per agent.

Simulation may execute many logical transitions between visual updates.

Rendering frequency and simulation frequency are independent.

---

# 28. Renderer choice

Because normal visualization sizes are below 1,000 agents, start with the simplest renderer that satisfies the required interaction quality.

Preferred implementation order:

```text
1. Canvas 2D
2. benchmark at 1k and 10k
3. WebGL2 only if Canvas becomes the measured bottleneck
```

Do not begin with WebGL solely because 10k is a theoretical maximum.

Do not use SVG/DOM elements as the primary moving-agent representation at the 10k limit.

Keep renderer interfaces separable from simulation.

---

# 29. Visualization level of detail

At large scales, preserve capability through zoom-dependent detail.

Example:

```text
zoomed out:
    graph geometry
    agent markers / density
    major state

medium:
    individual nodes
    agents
    selected labels

zoomed in:
    node IDs
    port labels
    algorithm-specific state

selected entity:
    complete details
```

Do not attempt to draw every port label and agent ID simultaneously at 10k scale.

---

# 30. Layout

Graph layout is a visualization concern, not simulation semantics.

Do not place layout code in `ccm-core`.

The browser may:

- use stored coordinates;
- compute a deterministic layout;
- progressively refine layout;
- use different layouts for different graph sizes.

Large-graph layout must not block simulation execution.

---

# 31. Compatibility

Existing useful simulation JSON should remain loadable where practical.

Create a compatibility layer:

```text
legacy JSON
      │
      ▼
canonical Rust trace/model
```

Do not force `ccm-core` to use the old schema internally.

The historical parallel-array output is a compatibility format, not the new domain model.

---

# 32. Canonical result model

Avoid APIs such as:

```text
positions
statuses
array3
array4
array5
```

with meanings that vary by algorithm.

Use explicit types.

Example:

```rust
pub struct SimulationResult {
    pub metadata: SimulationMetadata,
    pub termination: Termination,
    pub final_state: SimulationState,
    pub metrics: ComplexityMetrics,
}
```

Trace is a separate output:

```rust
pub struct SimulationTrace {
    pub checkpoints: Vec<Checkpoint>,
    pub events: Vec<SimulationEvent>,
}
```

Algorithm-specific data should use variants or clearly named optional structures.

---

# 33. Termination

Never treat reaching a configured limit as successful completion.

Use explicit reasons:

```rust
pub enum Termination {
    Completed,
    RoundLimitReached,
    InvalidConfiguration,
    Cancelled,
    InvariantViolation,
}
```

Research outputs must record termination status.

---

# 34. Correctness invariants

Apply invariants appropriate to each algorithm.

Universal examples:

- number of agents is conserved;
- every agent occupies a valid node;
- every movement follows a graph edge;
- every used port exists;
- reciprocal ports are consistent;
- agent IDs remain unique;
- deterministic configurations remain deterministic;
- algorithm state is internally consistent.

Settlement examples:

- homes are valid;
- owner indexes agree with agent state;
- settled ownership is unique where required;
- parent relationships refer to valid edges;
- tree indexes agree with parent state.

Algorithm-specific invariants belong next to the corresponding algorithm tests.

---

# 35. Complexity review of hot operations

For every important operation, document expected complexity in:

```text
A = number of agents
V = number of nodes
E = number of edges
D = local degree
```

Examples:

```text
neighbor by port                 O(1)
home owner lookup               O(1)
settled agent lookup            O(1)
agent lookup                    O(1)
move k agents                   O(k)
iterate tree children           O(number of children)
rebuild occupancy buckets       O(A + V)
```

Common-path accidental `O(A²)` behavior must be removed or justified.

---

# 36. Current implementation patterns that must not be copied

When inspecting the Python code, specifically look for:

- complete-state snapshots during individual moves;
- deep copies of all-agent state;
- owner lookup by scanning every agent;
- tree-child discovery by scanning every agent;
- repeated construction of sets/maps in inner loops;
- sorting full agent collections where stable indexed ordering would suffice;
- graph operations implemented through heavyweight general-purpose dictionaries;
- visualization bookkeeping embedded in algorithm transitions.

Do not mechanically translate these patterns into Rust.

Port the semantics while improving representation.

---

# 37. Performance engineering policy

Although wall-clock is not the research metric, implementation performance still matters because it determines how large a study is practical.

Benchmark engineering performance separately.

Measure:

```text
simulations / second
agent transitions / second
memory allocations
implementation peak memory
trace-generation overhead
native vs WASM throughput
```

These metrics are for engineering decisions, not algorithmic complexity claims.

---

# 38. Migration plan

## Phase 0 — Repository and semantic audit

Read:

- README;
- both algorithm implementations;
- agent representation;
- graph utilities;
- simulation wrapper;
- browser runner;
- visualizer;
- stress tests;
- open relevant PRs.

Write:

```text
docs/behavior-spec.md
docs/complexity-metrics.md
```

Identify:

- theoretical rounds;
- sub-phases;
- movement definitions;
- probe definitions;
- memory state counted by the algorithms;
- centralized simulator shortcuts;
- current output-schema mismatch.

---

## Phase 1 — Establish Python reference fixtures

Create deterministic tests using explicit graphs and port assignments.

Include:

- paths;
- cycles;
- stars;
- trees;
- small dense graphs;
- single agent;
- agents equal to nodes;
- one starting node;
- multiple starting nodes;
- algorithm-specific probe/scout/retrace situations;
- termination cases.

Record expected:

- final state;
- rounds;
- moves;
- relevant algorithm counters;
- important intermediate states where necessary.

---

## Phase 2 — Build the pure Rust core

Create:

```text
PortGraph
AgentStore
SimulationConfig
deterministic RNG ownership
scheduler
termination model
complexity metrics interface
```

No browser.

No WASM.

No visualization.

---

## Phase 3 — Build the native experiment runner early

Before completing the web migration, create:

```text
ccm run
ccm sweep
ccm validate
```

The native tool should already support:

- parameterized graph models;
- port assumptions;
- starting assumptions;
- repeated seeds;
- CSV result output;
- parallel independent runs.

This is the first major product milestone.

---

## Phase 4 — Port Drop-and-Freeze

Port semantics into the dense Rust model.

Add:

- final-state parity tests;
- round-count parity tests;
- move-count tests;
- memory-counter tests;
- invariants;
- structured/adversarial tests.

Do not generate visualization state unless a recorder is explicitly supplied.

---

## Phase 5 — Port Help-by-Scouts

Before implementation, enumerate existing potentially expensive operations.

At minimum inspect:

- owner lookup;
- group discovery;
- scout state;
- probe structures;
- vacating;
- chase/follow;
- tree queries;
- retrace;
- snapshots.

Replace searches with indexes where semantics allow.

Pass differential tests against Python fixtures.

---

## Phase 6 — Validate research measurements

For each algorithm, confirm that:

- round counters correspond to the documented theoretical model;
- movement counters correspond to actual modeled traversals;
- probe counters have explicit definitions;
- logical memory accounting is documented;
- results are reproducible.

Do not proceed to large experimental studies until these definitions are trustworthy.

---

## Phase 7 — Add experiment families

Implement structured and randomized scenario generation.

Support geometric sweeps such as:

```text
100
200
400
800
1600
3200
6400
...
```

Provide repeated trials and aggregate helpers.

Keep raw per-run results.

Do not store only averages.

---

## Phase 8 — Add trace recording around the Rust core

Implement:

```text
NoTrace
FullTrace
BoundedTrace
```

Tracing is an observer/recorder layer around the same state transitions.

Verify that enabling tracing does not change algorithmic results or counters.

---

## Phase 9 — Compile the same core to WASM

Create the browser adapter.

Temporarily keep the existing UI if useful for parity testing.

Do not reimplement algorithms for WASM.

---

## Phase 10 — Move browser simulation into a Web Worker

The main thread handles:

- controls;
- rendering;
- playback;
- selection.

The worker handles:

- graph generation;
- simulation;
- trace generation;
- export preparation where useful.

Do not send one message per logical agent action.

---

## Phase 11 — Build the visualization renderer

Start with Canvas 2D.

Target normal use:

```text
< 1,000 agents
```

Test upper-bound use:

```text
10,000 agents
```

If Canvas is insufficient based on measurements, replace the renderer with WebGL2 behind the same interface.

Do not redesign the simulation engine to accommodate renderer shortcomings.

---

## Phase 12 — Port application UI to Rust

Port:

- parameters;
- algorithm selection;
- run controls;
- play/pause;
- previous/next;
- speed;
- filters;
- hide/show agents;
- tooltips;
- import/export;
- dark/light mode;
- responsive layout.

UI framework choice is secondary.

Do not model every moving agent as an individual reactive UI component.

---

## Phase 13 — Retire production Python and Pyodide

Only after:

- both Rust algorithms pass parity;
- native experiments work;
- complexity metrics are validated;
- browser visualization uses Rust/WASM;
- useful legacy files load;
- feature parity is acceptable.

Then remove:

- Pyodide from production;
- Python algorithm execution from production;
- NetworkX from production;
- old JS simulation bridge;
- old visualizer when replaced.

Python reference code may remain archived for provenance if useful.

---

# 39. Testing strategy

Required categories:

## Unit tests

For:

- graph ports;
- reciprocal edges;
- IDs;
- ownership indexes;
- occupancy structures;
- counters;
- trace replay.

## Differential tests

Python reference versus Rust.

## Invariant tests

Properties that must always hold.

## Determinism tests

Same input produces same output.

## Scenario tests

Named graph/placement cases.

## Experiment tests

Sweep configurations produce expected numbers of runs and reproducible rows.

## WASM tests

Core browser adapter and trace correctness.

Visualization pixel-perfect tests are optional; semantic renderer tests are preferred where possible.

---

# 40. Experimental analysis discipline

When using generated data to reason about asymptotic behavior:

- retain raw per-run counters;
- use multiple graph sizes;
- use repeated seeds;
- report dispersion, not only means;
- test structured/adversarial families separately from random graphs;
- separate parameters such as `n`, `k`, degree, number of starting nodes, and port assumptions;
- do not infer worst-case complexity from random averages alone.

The simulator produces empirical evidence.

It does not replace theoretical proof.

---

# 41. Git/PR strategy

Use reviewable stages.

Suggested PR order:

```text
1. test: specify current algorithm behavior and complexity counters
2. chore: create Rust workspace
3. feat(core): add port graph and dense IDs
4. feat(core): add deterministic simulation model
5. feat(metrics): add complexity instrumentation
6. feat(cli): add native run and sweep commands
7. feat(drop-freeze): port algorithm
8. feat(help-scouts): port algorithm
9. feat(experiments): add graph and placement families
10. feat(trace): add optional event tracing
11. feat(wasm): expose shared simulator to browser
12. feat(worker): move browser execution off main thread
13. feat(web): add Rust visualization
14. feat(compat): complete legacy import/export
15. chore: remove Pyodide/Python production execution
16. docs: finalize research and architecture documentation
```

Do not combine:

- algorithm port;
- semantics changes;
- complexity-definition changes;
- UI redesign;
- legacy deletion;

into one large PR.

---

# 42. PR completion report

Every simulation/optimization PR must report:

```text
Scope:
Behavioral changes:
Complexity counters affected:
Round semantics affected:
Logical memory accounting affected:
Fixtures added:
Invariants added:
Python parity:
Native tests:
WASM tests:
Known discrepancies:
Next step:
```

Performance-focused PRs may additionally report wall-clock engineering benchmarks, but those must be labeled as implementation benchmarks.

---

# 43. Definition of done

The migration is complete when:

- both algorithms have one authoritative Rust implementation;
- native Rust research simulations require no browser;
- parameter sweeps can run in parallel across independent trials;
- round complexity is explicitly counted and documented;
- movement/probe/algorithm-specific operation counts are explicitly defined;
- logical memory complexity instrumentation is defined and documented;
- graph/port/placement assumptions are configurable;
- structured and random graph families are supported;
- deterministic fixtures pass;
- native and WASM use the same core code;
- browser execution requires no backend;
- browser execution requires no Python/Pyodide;
- visualization tracing is optional;
- normal visualizations below 1,000 agents are smooth and usable;
- approximately 10,000-agent visualization is supported with appropriate level of detail;
- existing useful simulation JSON can be imported or explicitly migrated;
- production no longer depends on NetworkX or Cytoscape for simulation semantics;
- README explains native experiments and web visualization separately.

---

# 44. First tasks for an autonomous coding agent

Execute in this order:

1. Inspect the current repository and relevant open PRs.
2. Write `docs/behavior-spec.md`.
3. Write `docs/complexity-metrics.md`.
4. Define exact round semantics for Drop-and-Freeze.
5. Define exact round semantics for Help-by-Scouts.
6. Define logical memory accounting for both algorithms.
7. Add deterministic Python reference fixtures.
8. Add a Rust workspace.
9. Implement `PortGraph`.
10. Implement dense IDs and agent storage.
11. Implement ownership/occupancy indexes.
12. Implement deterministic RNG ownership.
13. Implement metrics hooks with `NoMetrics` and complexity counters.
14. Build `ccm run`.
15. Build `ccm sweep`.
16. Parallelize independent native runs.
17. Port Drop-and-Freeze.
18. Validate round/move/memory counters.
19. Port Help-by-Scouts.
20. Validate its round/move/probe/memory counters.
21. Add structured/adversarial generators.
22. Add trace recorder interfaces.
23. Compile `ccm-core` to WASM.
24. Run the simulator in a browser worker.
25. Implement Canvas visualization.
26. Benchmark visualization at 1k and 10k agents.
27. Move to WebGL2 only if Canvas is the measured bottleneck.
28. Port remaining UI features.
29. Remove Python/Pyodide from production.
30. Update research and deployment documentation.

---

# 45. Final guiding rule

When choosing between two designs, prefer the one that makes this statement true:

> **The exact same Rust state transition that is counted in a native complexity experiment is the state transition that can later be visualized in the browser.**

The research simulator and the visualizer must never drift into separate algorithm implementations.
