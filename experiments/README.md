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
operation counters, maxima, and logical peak memory. Preserve raw rows; compute
means, quantiles, and dispersion as a separate analysis step. Random averages
are not evidence of worst-case complexity, so analyze structured and
adversarial cases separately.

The `CCM_GIT_COMMIT` value is compiled into the binary. If omitted, the CSV
uses `unknown`; that is suitable for local exploration but not an archival
experiment.
