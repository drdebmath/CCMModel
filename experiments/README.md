# Reproducible experiments

Build optimized binaries before a study:

```sh
CCM_GIT_COMMIT="$(git rev-parse HEAD)" cargo build --release -p ccm-cli
```

One run:

```sh
./target/release/ccm run \
  --algorithm drop-and-freeze \
  --graph random-bounded:4 \
  --nodes 1000 --agents 800 --seed 42 \
  --ports random --placement uniform:25
```

A deterministic Cartesian sweep, with raw rows retained:

```sh
./target/release/ccm sweep \
  --algorithm help-by-scouts \
  --graph tree:2 \
  --nodes 100,200,400,800 \
  --agents 50,100 --seeds 1,2,3,4,5 \
  --ports adversarial --placement single:0 \
  --workers 8 > results.csv
```

Every CSV row includes the algorithm/version, optional build commit, graph and
port model, placement, explicit starting vector, seed, termination reason,
operation counters, maxima, and logical peak memory. The last three columns
were appended in turn, so column positions in older result files are
unchanged:

- `algorithm_kind` is `paper` for an algorithm written from a publication and
  `legacy_baseline` for Drop-and-Freeze and Help-by-Scouts, which preserve this
  repository's historical code.
- `scheduler` is `synchronous`, `asynchronous`, `interleaved` or
  `starved:AGENT:PERIOD`.
- `time_unit` says what `rounds` counts: synchronous `round`s, or asynchronous
  `epoch`s, the shortest intervals in which every agent completes a cycle. The
  two are not comparable; never mix them in one analysis.

Preserve raw rows; compute
means, quantiles, and dispersion as a separate analysis step. Random averages
are not evidence of worst-case complexity, so analyze structured and
adversarial cases separately.

## RootedAsync under asynchrony

`rooted-async` is the only algorithm that runs under a non-synchronous
scheduler; the others reject one. The random order is drawn from the run's
seed, so a row is reproducible from its own values:

```sh
./target/release/ccm sweep   --algorithm rooted-async --scheduler asynchronous   --graph grid:8 --nodes 64,128 --agents 32,64 --seeds 1,2,3   --ports random --workers 8 > rooted-async.csv
```

`--scheduler interleaved` is the paper's ASYNC in full: each cycle's look and
its write are separate steps of the random order, so other agents act in
between. `--scheduler starved:AGENT:PERIOD` activates one agent only every
`PERIOD` steps, to watch a slow scout or a slow lead.

`ccm events` traces a single run instead: every probe the head concluded (with
the rule that decided it, R1 to R3c-ii) and every DFS step (direction, probe
batches, scouts), each with the epoch it happened at.

```sh
./target/release/ccm events --algorithm rooted-async --scheduler asynchronous   --graph path --nodes 8 --agents 8 > trace.csv
```

The `CCM_GIT_COMMIT` value is compiled into the binary. If omitted, the CSV
uses `unknown`; that is suitable for local exploration but not an archival
experiment.
