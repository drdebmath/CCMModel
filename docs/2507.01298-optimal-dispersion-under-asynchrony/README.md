# Optimal Dispersion Under Asynchrony (arXiv:2507.01298)

Work on Pattanayak, Kshemkalyani, Kumar, Molla and Sharma,
*Optimal Dispersion Under Asynchrony*,
[arXiv:2507.01298](https://arxiv.org/abs/2507.01298), for assignment A2:
`RootedAsync`, the paper's asynchronous rooted dispersion algorithm
(Sections 4-5, Algorithms 2-6).

## In this folder

- [gap-and-trace-note.md](gap-and-trace-note.md): how the algorithm is
  modelled, which decisions in the existing P1Tree consult the global
  ownership index, a hand-worked vacated-node probe, the eleven places the
  pseudocode needed repair, verification, and measurements.

## Elsewhere in the repository

| What | Where |
|---|---|
| The algorithm | [`crates/ccm-algorithms/src/rooted_async/`](../../crates/ccm-algorithms/src/rooted_async/) |
| Its tests, including the paper's Appendix B run | [`crates/ccm-experiments/tests/rooted_async.rs`](../../crates/ccm-experiments/tests/rooted_async.rs) |
| Validation of the paper's claims (dispersion, asynchrony, O(1) per step and O(k) total, O(log(k+Δ)) memory) | [`crates/ccm-experiments/tests/rooted_async_validation.rs`](../../crates/ccm-experiments/tests/rooted_async_validation.rs), run by [`scripts/validate-rooted-async.sh`](../../scripts/validate-rooted-async.sh) |
| The sweep that checks local decisions against the truth | [`crates/ccm-experiments/examples/rooted_async_sweep.rs`](../../crates/ccm-experiments/examples/rooted_async_sweep.rs) |
| The scaling measurements | [`crates/ccm-experiments/examples/rooted_async_scaling.rs`](../../crates/ccm-experiments/examples/rooted_async_scaling.rs) |
| The synchronous P1Tree construction this builds on, unchanged | [`../p1tree.md`](../p1tree.md) |

## Reproduce

```sh
./scripts/validate-rooted-async.sh          # each claim, PASS/FAIL (~2 min)
./scripts/validate-rooted-async.sh --full   # also the 8,208-run truth-check sweeps
cargo test -p ccm-experiments --test rooted_async
cargo run --release -p ccm-experiments --example rooted_async_sweep        # synchronous
cargo run --release -p ccm-experiments --example rooted_async_sweep -- 11  # random schedule, seed 11
cargo run --release -p ccm-experiments --example rooted_async_sweep -- i11 # interleaved, seed 11
cargo run --release -p ccm-experiments --example rooted_async_scaling

# The CLI (build with: cargo build --release -p ccm-cli)
./target/release/ccm run    --algorithm rooted-async --scheduler asynchronous --graph grid:4 --nodes 16 --agents 10
./target/release/ccm run    --algorithm rooted-async --scheduler interleaved  --graph grid:4 --nodes 16 --agents 10
./target/release/ccm events --algorithm rooted-async --scheduler asynchronous --graph path --nodes 8 --agents 8
```

In the browser, choose *RootedAsync* from the simulator's algorithm menu,
then *Asynchronous* from the *Schedule* menu that appears below it, which
runs the paper's ASYNC with each agent's look and write apart. During
playback:

- an agent on an edge is drawn halfway along it, outlined;
- a dashed amber ring marks the home of each scout that is away from it;
- a blue ring marks the head while it probes, and the line under the timeline
  names each port it probed, the rule (R1 to R3c-ii) that decided it, and
  what it found.

*Compare schedules* runs the same input synchronously and under three seeded
asynchronous orders, and tabulates time, moves, probes and whether each final
placement matches the synchronous one. The placement is locked to *Rooted at
node 0*, which the algorithm requires.

## A2 checklist

- [x] Gap and trace note
- [x] RootedAsync tests
- [x] CLI (Stage 2): `--algorithm rooted-async`, `--scheduler`, `ccm events`
- [x] Web demo (Stage 3): *RootedAsync* in the simulator's menu, with a *Schedule* menu
- [x] Web extras: agents on edges, scouts' homes, probe rules per frame, schedule comparison
- [x] Non-atomic cycles: the `Interleaved` schedule, look and write apart
- [x] Memory: Theorem 2's `O(log(k + Δ))` bits, measured and validated
- [x] Simulator cost: indexed lookups, k = 1024 in seconds
- [x] Flowchart: RootedAsync on the Flowcharts page and in `docs/algorithm-flowcharts.md`
