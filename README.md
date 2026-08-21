# CCMModel

CCMModel is a deterministic research simulator and browser visualizer for two
mobile-agent dispersion state machines:

- Drop-and-Freeze;
- Help-by-Scouts.

Both native experiments and browser playback execute the same authoritative
Rust algorithms. The production browser is client-only Rust/WebAssembly in a
Web Worker with Canvas 2D rendering—no Python, Pyodide, NetworkX, Cytoscape,
CDN, or application backend.

> The repository implementations preserve the behavior of the historical
> Python files. Their names and transitions are not literal ports of the
> algorithms in Sudo et al., *Near-linear Time Dispersion of Mobile Agents*.
> Read [the behavior specification](docs/behavior-spec.md) before relating
> empirical results to the paper's assumptions or theorems.

## Quick start: native research runs

Install stable Rust, then build the optimized CLI:

```sh
rustup target add wasm32-unknown-unknown
cargo build --release -p ccm-cli
```

Run one deterministic experiment and emit CSV:

```sh
./target/release/ccm run \
  --algorithm drop-and-freeze \
  --graph path --nodes 100 --agents 80 \
  --ports canonical --placement single:0 \
  --seed 42 --round-limit 10000
```

Run a Cartesian sweep in parallel; output order remains deterministic:

```sh
./target/release/ccm sweep \
  --algorithm help-by-scouts \
  --graph tree:2 \
  --nodes 100,200,400 --agents 50,100 \
  --seeds 1,2,3,4,5 --ports adversarial \
  --placement single:0 --workers 8 > results.csv
```

Available graph families are `path`, `cycle`, `star`, `complete`,
`tree[:branching]`, `grid[:columns]`, `random-connected:extra-edges`, and
`random-bounded:max-degree`. Port policies are `canonical`, `random`, and
`adversarial`; the library also accepts explicit reciprocal port tables.
The browser additionally offers a seeded `random` connected graph built from
a random spanning tree plus reproducible extra edges.
Placements are rooted (`single[:node]`), `uniform:K`, `clustered:n,n`, or
`explicit:n,n`.

For archival runs, compile the commit into every result row:

```sh
CCM_GIT_COMMIT="$(git rev-parse HEAD)" cargo build --release -p ccm-cli
```

CSV contains one row per run with full configuration, starts, termination,
logical rounds, traversals, probes, settlements, algorithm-specific counters,
maxima, and logical memory. Wall-clock runtime is never substituted for an
algorithmic complexity counter. See [experiments/README.md](experiments/README.md)
and [docs/complexity-metrics.md](docs/complexity-metrics.md).

## Quick start: browser visualization

Install the bindings generator version pinned by `ccm-wasm`, build the package,
and serve the repository:

```sh
cargo install wasm-bindgen-cli --version 0.2.100 --locked
./scripts/build-wasm.sh
python3 -m http.server 8000 --bind 127.0.0.1
```

Open <http://127.0.0.1:8000>. The UI supports both algorithms, deterministic
graphs and placement, full/off/bounded traces, run/cancel, timeline playback,
filters, canonical local port labels, node tooltips with agent positions,
JSON import/export, responsive controls, and light/dark themes. Random graphs
use a deterministic spring layout that is cached across playback frames.
Port badges are drawn for sparse views where they remain legible; the complete
canonical `pN→neighbor` table is always available in each node tooltip. Imported
executions are validated for graph and agent-state invariants before rendering.
Full tracing is for small simulations. The UI automatically uses bounded trace
and reduced visual detail for large cases; 10,000 agents is an upper-bound
engineering mode, not the normal playback target.

`scripts/build-wasm.sh` replaces the single package in `wasm/web` and writes a
content-derived `manifest.json`. The page and worker load both generated modules
and their `.wasm` binaries with that build ID, preventing mixed cached builds
without accumulating manually numbered package directories.

This workspace also includes a persistent macOS development service. Its
source plist is [`dev/com.ccmmodel.local-server.plist`](dev/com.ccmmodel.local-server.plist);
the installed `com.ccmmodel.local-server` LaunchAgent serves this checkout at
port 8000 and restarts at login independently of a Codex session.

## Tests and verification

Run the complete Rust suite and strict lints:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo check -p ccm-wasm --target wasm32-unknown-unknown
cargo check -p ccm-web --target wasm32-unknown-unknown
```

The historical Python code is retained only as a reference, differential-test
source, and benchmark baseline. In an environment with `pytest` and
`networkx`:

```sh
python -m pytest -q
python stress_test.py --algo both --num-tests 10
```

## Engineering benchmark

On the recorded Apple Silicon environment, rooted path cases with `n = k`
showed median speedups of 16.89×–26.78× for Drop-and-Freeze and
1,109.85×–5,176.19× for Help-by-Scouts at sizes 25–100. Every sample had an
equal final-position checksum. The legacy Python path necessarily records full
snapshot history while optimized Rust uses `NoTrace`; this is a research
throughput comparison, not an algorithmic-complexity result or equal-tracing
microbenchmark. Method, dispersion, caveats, and raw CSV are in
[docs/benchmarks.md](docs/benchmarks.md).

Reproduce it with:

```sh
python benchmarks/compare_legacy.py \
  --sizes 25,50,100 --iterations 3 --samples 5
```

## Workspace map

- `ccm-core`: dense IDs, reciprocal port graph, deterministic RNG, indexes,
  termination, metrics, and scheduler primitives.
- `ccm-algorithms`: Drop-and-Freeze.
- `ccm-help-scouts`: Help-by-Scouts.
- `ccm-trace`: shared `NoTrace`, `FullTrace`, and deterministic bounded trace.
- `ccm-experiments`: graph/port/placement families and parallel sweeps.
- `ccm-cli`: native CSV command line.
- `ccm-compat`: validated typed import/export for the historical five-slot JSON
  mismatch.
- `ccm-wasm`: typed bindings and compact versioned binary render stream.
- `ccm-web`: Rust/web-sys controls, trace decoding, playback, import/export,
  responsive behavior, and Canvas 2D renderer.
- `index.html` and `app.css`: static browser shell and styling; `main.js` is the
  manifest-aware generated-module bootstrap and `worker.js` is the narrow,
  typed-array worker/WASM bridge.

See [architecture](docs/architecture.md),
[simulation model](docs/simulation-model.md), and
[migration notes](docs/migration.md) for the boundaries and compatibility
decisions.

## Known semantic limitation

The preserved Help-by-Scouts reference behavior is rooted. Its historical
multi-start path currently fails, and Rust intentionally records that case as
a parity test instead of silently inventing new semantics. Use rooted placement
for Help-by-Scouts until a separately specified general-dispersion algorithm is
implemented.

## License

MIT
