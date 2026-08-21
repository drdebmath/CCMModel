# Engineering benchmark: legacy Python versus Rust

This benchmark measures implementation runtime, not round, move, message, or
memory complexity. Algorithmic conclusions must use the explicit counters
defined in [complexity-metrics.md](complexity-metrics.md).

## Method

`benchmarks/compare_legacy.py` runs both implementations on the same canonical
path, with all `k = n` agents initially at node 0 and a `40k` limit. Graph
construction and Rust process startup are outside the timed regions. Each
sample contains three complete simulations; both implementations are warmed
before five samples. The table reports medians plus min/max and population
standard deviation. A final-position checksum is compared for every sample;
full semantic parity is covered separately by the fixture suites.

The comparison is intentionally end-to-end for each simulation implementation.
The legacy functions always build their complete snapshot histories, while the
Rust benchmark uses the normal optimized native `NoTrace` path. This is the
relevant research-throughput comparison, but it is not a trace-on versus
trace-on microbenchmark. Much of the Help-by-Scouts gain comes from removing
Python deep copies and repeated owner scans as well as from native compilation.

Environment: 20 August 2026, Apple Silicon macOS 26.6.2, Rust 1.97.1 release
profile, Python 3.13.14, repository baseline commit
`131a750d225d92512f7d79200f600d131d5f1df9` plus the migration worktree.

## Median results

| Algorithm | n = k | Python | Rust | Speedup | Checksum |
| --- | ---: | ---: | ---: | ---: | --- |
| Drop-and-Freeze | 25 | 4.94 ms | 0.293 ms | 16.89× | equal |
| Drop-and-Freeze | 50 | 10.26 ms | 0.497 ms | 20.63× | equal |
| Drop-and-Freeze | 100 | 35.37 ms | 1.321 ms | 26.78× | equal |
| Help-by-Scouts | 25 | 131.61 ms | 0.119 ms | 1,109.85× | equal |
| Help-by-Scouts | 50 | 890.35 ms | 0.346 ms | 2,573.89× | equal |
| Help-by-Scouts | 100 | 6.230 s | 1.204 ms | 5,176.19× | equal |

Raw dispersion is retained in
[`benchmarks/results-2026-08-20.csv`](../benchmarks/results-2026-08-20.csv).
Reproduce with:

```sh
python benchmarks/compare_legacy.py \
  --sizes 25,50,100 --iterations 3 --samples 5
```

## Browser scale checks

The Rust UI was also exercised in Edge against the persistent local server on
21 August 2026. A 1,000-node/1,000-agent path with bounded tracing and a short
round limit reached a rendered, interactive 16-frame result in 299 ms. A
10,000-node/10,000-agent path with tracing off and a one-activation limit
reached the two-frame Canvas level-of-detail view in 313 ms. Both timings are
single end-to-end smoke measurements, not statistically controlled runtime
benchmarks or algorithmic-complexity results. They verify the intended UI
scale paths; normal detailed playback remains targeted below 1,000 agents.
